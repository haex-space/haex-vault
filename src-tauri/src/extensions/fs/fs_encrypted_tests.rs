//! The raw directory of an encrypted folder on the device (spec 048 US7, FR-051, T079): no call of
//! an extension's file access reaches into it, even under a granted folder, and listings and
//! watches leave it out.

// The tests set permissions and write files directly.
#![allow(clippy::disallowed_methods)]

use std::time::{Duration, Instant};

use serde_json::{json, Value};

use super::lock;
use super::test_support::setup;
use crate::extensions::bridge::dispatch::call;
use crate::extensions::error::BridgeError;

const RAW: &str = "abcdefghijklmnopqrstuvwxyz.hxef";

fn code(outcome: Result<Value, BridgeError>) -> u16 {
    outcome.unwrap_err().code.as_u16()
}

#[test]
fn every_call_into_the_raw_directory_is_refused_like_holzis_own_places() {
    let s = setup();
    let ctx = s.frame("good-notes-like.xt");
    s.grant(&ctx, "readWrite", &s.root);
    let raw = s.root.join("Dokumente").join(RAW);
    std::fs::create_dir_all(raw.join("m")).unwrap();
    std::fs::write(raw.join("h"), b"HXEF").unwrap();
    let at = |rest: &str| raw.join(rest).to_string_lossy().into_owned();
    for (method, params) in [
        ("extension_filesystem_read_file", json!({ "path": at("h") })),
        ("extension_filesystem_stat", json!({ "path": at("h") })),
        ("extension_filesystem_exists", json!({ "path": at("h") })),
        ("extension_filesystem_read_dir", json!({ "path": at("m") })),
        (
            "extension_filesystem_write_file",
            json!({ "path": at("m/neu"), "data": "eA==" }),
        ),
        ("extension_filesystem_mkdir", json!({ "path": at("c") })),
        ("extension_filesystem_remove", json!({ "path": at("h") })),
        (
            "extension_filesystem_rename",
            json!({ "from": at("h"), "to": s.root.join("h").to_string_lossy() }),
        ),
        (
            "extension_filesystem_copy",
            json!({ "from": at("h"), "to": s.root.join("kopie").to_string_lossy() }),
        ),
        (
            "extension_filesystem_watch",
            json!({ "ruleId": "r", "path": at("") }),
        ),
    ] {
        assert_eq!(code(call(&ctx, method, &params)), 1002, "{method}");
    }
    assert!(raw.join("h").exists(), "nothing was removed");
    assert!(!raw.join("c").exists() && !raw.join("m/neu").exists());
}

#[test]
fn listings_and_watches_leave_the_raw_directory_out() {
    let s = setup();
    let ctx = s.frame("good-notes-like.xt");
    s.grant(&ctx, "read", &s.root);
    let folder = s.root.join("Dokumente");
    std::fs::create_dir_all(folder.join(RAW)).unwrap();
    std::fs::write(folder.join("brief.txt"), "x").unwrap();
    let listed = call(
        &ctx,
        "extension_filesystem_read_dir",
        &json!({ "path": folder.to_string_lossy() }),
    )
    .unwrap();
    let names: Vec<&str> = listed
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|entry| entry["name"].as_str())
        .collect();
    assert_eq!(names, ["brief.txt"]);

    let watch = json!({ "ruleId": "w", "path": folder.to_string_lossy() });
    call(&ctx, "extension_filesystem_watch", &watch).unwrap();
    std::fs::write(folder.join(RAW).join("h"), b"HXEF").unwrap();
    std::fs::write(folder.join("danach.txt"), "x").unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let events = lock(&s.recorded.0).clone();
        if events.iter().any(|e| e["data"]["path"] == "danach.txt") {
            assert!(
                events
                    .iter()
                    .all(|e| !e["data"]["path"].as_str().unwrap_or("").contains(".hxef")),
                "{events:?}"
            );
            break;
        }
        assert!(Instant::now() < deadline, "no change reported");
        std::thread::park_timeout(Duration::from_millis(50));
    }
}
