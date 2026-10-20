//! Which program a shell may start, and what an answer tells before the permission (spec 017,
//! US11, T109): the extension hears the program as it named it, the user is asked about its
//! canonical path.

use std::path::Path;

use serde_json::{json, Value};

use super::{code, setup, sh, Setup};
use crate::extensions::bridge::dispatch::{call, CallContext};
use crate::extensions::commands::permissions::{resolve, PermissionResolveArgs};
use crate::extensions::error::{Asks, BridgeError};
use crate::extensions::permissions::prompts::PermissionDecision;
use crate::extensions::permissions::store as permission_store;
use crate::extensions::registry::remove::set_enabled;
use crate::extensions::shell::{create, resolve_program};

fn create_named(ctx: &CallContext, shell: &str) -> Result<Value, BridgeError> {
    call(
        ctx,
        "extension_shell_create",
        &json!({ "options": { "shell": shell } }),
    )
}

fn named_events(s: &Setup, event: &str) -> Vec<Value> {
    s.recorded
        .all()
        .into_iter()
        .filter(|(e, _)| e == event)
        .map(|(_, p)| p)
        .collect()
}

fn resolved_targets(s: &Setup) -> Vec<Value> {
    named_events(s, crate::extensions::bridge::events::FRAME_EVENT)
        .into_iter()
        .filter(|p| p["type"] == "extension:permission-resolved")
        .map(|p| p["data"]["target"].clone())
        .collect()
}

#[test]
fn without_a_permission_holzi_asks_about_the_canonical_program_by_the_name_given() {
    let s = setup();
    let canonical = resolve_program("sh").unwrap();
    let asked = create_named(&s.notes, "sh").unwrap_err();
    assert_eq!(asked.code.as_u16(), 1004);
    assert_eq!(
        asked.details,
        Some(json!({ "resourceType": "shell", "action": "execute", "target": "sh" })),
        "the extension hears its own name, not where it leads"
    );
    assert_eq!(
        asked.asks.as_deref(),
        Some(&Asks::Target(canonical.to_string_lossy().into_owned()))
    );
    // Named by its own path, the program is the same question.
    create_named(&s.notes, canonical.to_str().unwrap()).unwrap_err();
    let requests = named_events(&s, "extension-permission-request");
    assert_eq!(requests.len(), 1, "both names ask the one question");
    assert_eq!(requests[0]["target"], canonical.to_string_lossy().as_ref());

    resolve(
        &s.vault,
        &s.host,
        &*s.recorded,
        s.notes.device,
        PermissionResolveArgs {
            request_id: requests[0]["requestId"].as_str().unwrap().to_owned(),
            decision: PermissionDecision::Allow,
            remember: true,
            model: None,
        },
        2,
    )
    .unwrap();
    let ext = s.notes.session.extension_id;
    let rows = s
        .vault
        .read_blocking(move |q| {
            permission_store::rows_of(q, ext).map_err(|e| haex_crdt::Error::consumer(e.to_string()))
        })
        .unwrap();
    let remembered = rows
        .iter()
        .find(|r| r.kind == "shell" && r.target == canonical.to_string_lossy())
        .unwrap();
    assert_eq!(
        remembered.vault_device_uuid, s.notes.device,
        "a remembered shell answer holds on this device only"
    );
    // The SDK waits for the target its 1004 named.
    assert_eq!(
        resolved_targets(&s),
        [json!(canonical.to_string_lossy()), json!("sh")]
    );
    let started = create_named(&s.notes, "sh").unwrap();
    call(
        &s.notes,
        "extension_shell_close",
        &json!({ "sessionId": started["sessionId"] }),
    )
    .unwrap();

    s.allow(&s.minimal, Path::new("/bin/false-not-sh"));
    assert_eq!(
        code(create_named(&s.minimal, "/bin/sh")),
        1004,
        "another program's permission does not count"
    );
}

#[test]
fn a_link_is_asked_about_by_its_target_and_told_by_its_name() {
    let s = setup();
    let dir = tempfile::tempdir().unwrap();
    let link = dir.path().join("my-shell");
    std::os::unix::fs::symlink(sh(), &link).unwrap();
    let asked = create_named(&s.notes, link.to_str().unwrap()).unwrap_err();
    assert_eq!(asked.details.unwrap()["target"], link.to_str().unwrap());
    assert_eq!(
        asked.asks.as_deref(),
        Some(&Asks::Target(sh().to_string_lossy().into_owned()))
    );
}

#[test]
fn a_missing_program_is_told_only_after_the_permission() {
    let s = setup();
    // Without a permission a missing file answers like an existing one: with a question.
    for named in ["/no/such/shell", "/no/such/../such/./shell"] {
        let asked = create_named(&s.notes, named).unwrap_err();
        assert_eq!(asked.code.as_u16(), 1004, "{named}");
        assert_eq!(asked.details.unwrap()["target"], named);
        assert_eq!(
            asked.asks.as_deref(),
            Some(&Asks::Target("/no/such/shell".into()))
        );
    }
    // A bare name that is not on `PATH` answers as one that is, but nobody is asked: no
    // permission can name it.
    let asked = create_named(&s.notes, "no-such-program-holzi").unwrap_err();
    assert_eq!(asked.code.as_u16(), 1004);
    assert_eq!(asked.details.unwrap()["target"], "no-such-program-holzi");
    assert_eq!(asked.asks.as_deref(), Some(&Asks::Nothing));
    assert!(named_events(&s, "extension-permission-request")
        .iter()
        .all(|r| r["target"] != "no-such-program-holzi"));

    s.allow(&s.notes, Path::new("/no/such/shell"));
    assert_eq!(
        code(create_named(&s.notes, "/no/such/shell")),
        2003,
        "granted, then looked at"
    );
    s.allow(&s.notes, Path::new("*"));
    assert_eq!(code(create_named(&s.notes, "no-such-program-holzi")), 2003);
}

#[test]
fn a_shell_started_while_its_extension_is_disabled_does_not_stay() {
    let s = setup();
    s.allow(&s.notes, &sh());
    // The call passed the bridge's check; the extension is disabled before the shell is listed.
    set_enabled(&s.vault, s.notes.session.extension_id, false, 3).unwrap();
    let refused = create(&s.notes, &json!({ "options": { "shell": "/bin/sh" } })).unwrap_err();
    assert_eq!(refused.code.as_u16(), 8002);
    assert_eq!(s.host.shells.count(s.notes.session.extension_id), 0);
}
