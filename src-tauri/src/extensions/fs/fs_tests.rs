// The tests set permissions and read files directly.
#![allow(clippy::disallowed_methods)]

use std::path::Path;
// Only the folder-watch tests wait on a deadline, and they do not exist on mobile.
#[cfg(not(any(target_os = "android", target_os = "ios")))]
use std::time::{Duration, Instant};

use serde_json::Value;

use super::test_support::setup;
use super::*;
use crate::extensions::bridge::dispatch::{call, Emit};

fn code(outcome: Result<Value, BridgeError>) -> u16 {
    outcome.unwrap_err().code.as_u16()
}

fn read(ctx: &CallContext, path: &Path) -> Result<Value, BridgeError> {
    call(
        ctx,
        "extension_filesystem_read_file",
        &json!({ "path": path.to_string_lossy() }),
    )
}

#[test]
fn a_granted_folder_reads_writing_needs_read_write_and_the_rest_asks() {
    let s = setup();
    let ctx = s.frame("good-notes-like.xt");
    s.grant(&ctx, "read", &s.root);

    assert_eq!(
        read(&ctx, &s.root.join("note.txt")).unwrap(),
        json!("aW5zaWRl")
    );
    let write = json!({ "path": s.root.join("new.txt").to_string_lossy(), "data": "eA==" });
    assert_eq!(
        code(call(&ctx, "extension_filesystem_write_file", &write)),
        1004
    );
    assert_eq!(code(read(&ctx, &s.outside.join("secret.txt"))), 1004);

    s.grant(&ctx, "readWrite", &s.root);
    call(&ctx, "extension_filesystem_write_file", &write).unwrap();
    assert_eq!(
        std::fs::read_to_string(s.root.join("new.txt")).unwrap(),
        "x"
    );
    let listed = call(
        &ctx,
        "extension_filesystem_read_dir",
        &json!({ "path": s.root.to_string_lossy() }),
    )
    .unwrap();
    assert!(listed
        .as_array()
        .unwrap()
        .iter()
        .any(|e| e["name"] == "note.txt"));
}

#[test]
fn every_check_runs_at_the_real_target() {
    let s = setup();
    let ctx = s.frame("good-notes-like.xt");
    s.grant(&ctx, "read", &s.root);
    let up = format!("{}/../outside/secret.txt", s.root.display());
    assert_eq!(
        code(call(
            &ctx,
            "extension_filesystem_read_file",
            &json!({ "path": up })
        )),
        1004
    );
    let spelled = format!("{}//./note.txt", s.root.display());
    assert!(call(
        &ctx,
        "extension_filesystem_read_file",
        &json!({ "path": spelled })
    )
    .is_ok());
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&s.outside, s.root.join("door")).unwrap();
        assert_eq!(
            code(read(&ctx, &s.root.join("door").join("secret.txt"))),
            1004
        );
    }
}

#[test]
fn holzis_own_places_stay_closed_even_under_a_granted_parent() {
    let s = setup();
    let ctx = s.frame("good-notes-like.xt");
    s.grant(&ctx, "readWrite", &s.root);
    assert_eq!(code(read(&ctx, &s.protected.join("vault.db"))), 1002);
    let remove = json!({ "path": s.root.to_string_lossy(), "recursive": true });
    assert_eq!(
        code(call(&ctx, "extension_filesystem_remove", &remove)),
        1002
    );
    assert!(s.protected.join("vault.db").exists());
    let check = call(
        &ctx,
        "extension_permissions_check_filesystem",
        &json!({ "path": s.protected.join("vault.db").to_string_lossy(), "operation": "read" }),
    )
    .unwrap();
    assert_eq!(check["status"], "denied");
}

#[test]
fn a_dialog_choice_holds_for_its_frame_only_and_ends_with_it() {
    let s = setup();
    let ctx = s.frame("good-notes-like.xt");
    let other = CallContext {
        session: s
            .host
            .frames
            .open(ctx.session.extension_id, uuid::Uuid::new_v4(), "tab-2"),
        db: s.vault.clone(),
        host: Arc::clone(&s.host),
        device: s.device,
        emitter: Arc::clone(&s.recorded) as Arc<dyn Emit>,
    };
    let secret = s.outside.join("secret.txt");
    *lock(&s.dialogs.choice) = Some(secret.clone());
    let chosen = call(&ctx, "extension_filesystem_select_file", &json!({})).unwrap();
    assert_eq!(chosen, json!([secret.to_string_lossy()]));
    assert_eq!(read(&ctx, &secret).unwrap(), json!("b3V0c2lkZQ=="));
    assert_eq!(code(read(&other, &secret)), 1004, "another frame");
    let write = json!({ "path": secret.to_string_lossy(), "data": "eA==" });
    assert_eq!(
        code(call(&ctx, "extension_filesystem_write_file", &write)),
        1004,
        "opening allows reading only"
    );

    s.host
        .fs
        .frame_closed(&ctx.session.frame, ctx.session.extension_id, false);
    assert_eq!(code(read(&ctx, &secret)), 1004, "the frame is gone");

    *lock(&s.dialogs.choice) = Some(s.protected.join("vault.db"));
    assert_eq!(
        code(call(&other, "extension_filesystem_select_file", &json!({}))),
        1002
    );
}

#[test]
fn saving_writes_the_chosen_file_and_lets_the_frame_write_it_again() {
    let s = setup();
    let ctx = s.frame("good-notes-like.xt");
    let target = s.outside.join("export.txt");
    *lock(&s.dialogs.choice) = Some(target.clone());
    let saved = call(
        &ctx,
        "extension_filesystem_save_file",
        &json!({ "data": [104, 105] }),
    )
    .unwrap();
    assert_eq!(saved["success"], true);
    assert_eq!(std::fs::read_to_string(&target).unwrap(), "hi");
    let again = json!({ "path": target.to_string_lossy(), "data": "eA==" });
    call(&ctx, "extension_filesystem_write_file", &again).unwrap();

    *lock(&s.dialogs.choice) = None;
    assert_eq!(
        call(
            &ctx,
            "extension_filesystem_save_file",
            &json!({ "data": [] })
        )
        .unwrap(),
        Value::Null,
        "cancelled"
    );
}

#[test]
fn opening_a_file_uses_only_its_name_in_holzis_scratch_folder() {
    let s = setup();
    let ctx = s.frame("good-notes-like.xt");
    let opened = call(
        &ctx,
        "extension_filesystem_open_file",
        &json!({ "data": [1, 2], "fileName": "../../escape.txt" }),
    )
    .unwrap();
    assert_eq!(opened["success"], true);
    let path = lock(&s.dialogs.opened)[0].clone();
    assert_eq!(path.file_name().unwrap(), "escape.txt");
    assert_eq!(path.parent().unwrap().parent().unwrap(), s.scratch);
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
#[test]
fn a_watch_belongs_to_its_extension_and_reports_only_to_its_frames() {
    let s = setup();
    let notes = s.frame("good-notes-like.xt");
    let minimal = s.frame("good-minimal.xt");
    s.grant(&notes, "read", &s.root);
    // Not the granted folder itself: it holds one of holzi's places, and a watch reaches the tree.
    let watched = s.root.join("watched");
    std::fs::create_dir(&watched).unwrap();
    let holding = json!({ "ruleId": "r0", "path": s.root.to_string_lossy() });
    assert_eq!(
        code(call(&notes, "extension_filesystem_watch", &holding)),
        1002
    );
    let watch = json!({ "ruleId": "r1", "path": watched.to_string_lossy() });
    call(&notes, "extension_filesystem_watch", &watch).unwrap();
    let asked = json!({ "ruleId": "r1" });
    assert_eq!(
        call(&minimal, "extension_filesystem_is_watching", &asked).unwrap(),
        json!(false)
    );
    call(&minimal, "extension_filesystem_unwatch", &asked).unwrap();
    assert_eq!(
        call(&notes, "extension_filesystem_is_watching", &asked).unwrap(),
        json!(true)
    );

    std::fs::write(watched.join("changed.txt"), "x").unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    let heard = loop {
        let events = lock(&s.recorded.0).clone();
        if let Some(event) = events
            .iter()
            .find(|e| e["data"]["path"] == "changed.txt")
            .cloned()
        {
            break event;
        }
        assert!(Instant::now() < deadline, "no change reported");
        std::thread::park_timeout(Duration::from_millis(50));
    };
    assert_eq!(heard["frame"], notes.session.frame);
    assert_eq!(heard["type"], "filesync:file-changed");
    assert_eq!(heard["data"]["ruleId"], "r1");
    assert!(lock(&s.recorded.0)
        .iter()
        .all(|e| e["frame"] != minimal.session.frame));

    call(&notes, "extension_filesystem_unwatch", &asked).unwrap();
    assert_eq!(
        call(&notes, "extension_filesystem_is_watching", &asked).unwrap(),
        json!(false)
    );
}

#[test]
fn known_places_are_named_and_still_need_a_permission() {
    let s = setup();
    let ctx = s.frame("good-notes-like.xt");
    let known = call(&ctx, "extension_filesystem_known_paths", &Value::Null).unwrap();
    let home = known["home"].as_str().unwrap().to_owned();
    assert_eq!(
        code(read(&ctx, &Path::new(&home).join("outside/secret.txt"))),
        1004
    );
}

#[test]
fn the_viewer_opens_documents_and_media_but_no_programs_or_pages() {
    let s = setup();
    let ctx = s.frame("good-notes-like.xt");
    let open = |name: &str| {
        call(
            &ctx,
            "extension_filesystem_open_file",
            &json!({ "data": [1], "fileName": name }),
        )
    };
    for name in ["report.PDF", "photo.jpeg", "talk.mp4"] {
        assert_eq!(open(name).unwrap()["success"], true, "{name}");
    }
    let validation = ExtensionErrorCode::Validation.as_u16();
    for name in [
        "setup.exe",
        "run.bat",
        "start.desktop",
        "page.html",
        "drawing.svg",
        "macro.docm",
        "no-extension",
        "report.pdf.",
        "report.bat:x.pdf",
    ] {
        assert_eq!(code(open(name)), validation, "{name}");
    }
    assert_eq!(lock(&s.dialogs.opened).len(), 3);
}

#[test]
fn reading_takes_only_regular_files() {
    let s = setup();
    let ctx = s.frame("good-notes-like.xt");
    s.grant(&ctx, "read", &s.root);
    assert_eq!(
        code(read(&ctx, &s.root)),
        ExtensionErrorCode::Validation.as_u16()
    );
}

#[cfg(unix)]
#[test]
fn nothing_is_written_through_a_link_nobody_checked() {
    let s = setup();
    let ctx = s.frame("good-notes-like.xt");
    s.grant(&ctx, "readWrite", &s.root);
    // A broken link inside the granted folder that points into one of holzi's places.
    std::os::unix::fs::symlink(s.protected.join("new.db"), s.root.join("dangling")).unwrap();
    let write = json!({ "path": s.root.join("dangling").to_string_lossy(), "data": "eA==" });
    assert!(call(&ctx, "extension_filesystem_write_file", &write).is_err());
    assert!(!s.protected.join("new.db").exists());

    // A link already lying in a copy's destination that leads into one of holzi's places.
    let (from, to) = (s.root.join("from"), s.root.join("to"));
    std::fs::create_dir_all(from.join("inner")).unwrap();
    std::fs::write(from.join("inner").join("vault.db"), "overwritten").unwrap();
    std::fs::create_dir(&to).unwrap();
    std::os::unix::fs::symlink(&s.protected, to.join("inner")).unwrap();
    let copy = json!({ "from": from.to_string_lossy(), "to": to.to_string_lossy() });
    assert!(call(&ctx, "extension_filesystem_copy", &copy).is_err());
    assert_eq!(
        std::fs::read_to_string(s.protected.join("vault.db")).unwrap(),
        "vault"
    );
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
#[test]
fn a_watch_a_dialog_choice_allowed_ends_with_its_frame() {
    let s = setup();
    let ctx = s.frame("good-notes-like.xt");
    let _other = s
        .host
        .frames
        .open(ctx.session.extension_id, Uuid::new_v4(), "tab-2");
    *lock(&s.dialogs.choice) = Some(s.outside.clone());
    call(&ctx, "extension_filesystem_select_folder", &json!({})).unwrap();
    let watch = json!({ "ruleId": "chosen", "path": s.outside.to_string_lossy() });
    call(&ctx, "extension_filesystem_watch", &watch).unwrap();
    assert!(s
        .host
        .fs
        .watches
        .running(ctx.session.extension_id, "chosen"));

    // Another frame of the extension stays open, so only the choice's own watch ends.
    s.host
        .fs
        .frame_closed(&ctx.session.frame, ctx.session.extension_id, false);
    assert!(!s
        .host
        .fs
        .watches
        .running(ctx.session.extension_id, "chosen"));
}
