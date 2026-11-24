//! Tests for what counts as foreign content (spec 097 research R6, T050, T054).

use serde_json::json;

use super::is_foreign;

#[test]
fn own_actions_are_not_foreign() {
    let input = json!({});
    for scope in [
        "wm.read",
        "wm.layout",
        "settings.read",
        "passwords.read",
        "contacts.read",
        "contacts.write",
        "files.read",
        "files.write",
        "files.syncRules",
        "chat.write",
        "memory.read",
        "memory.write",
    ] {
        assert!(!is_foreign("action", Some(scope), &input), "{scope}");
    }
}

#[test]
fn the_web_received_messages_and_anything_unknown_are_foreign() {
    let input = json!({});
    for scope in [
        "browser",
        "chat.read",
        "files.shares",
        "newScope",
        "guardrails",
    ] {
        assert!(is_foreign("action", Some(scope), &input), "{scope}");
    }
    assert!(
        is_foreign("action", None, &input),
        "an action without a scope"
    );
    for source in ["haextension", "mcp", "cli", "something new"] {
        assert!(is_foreign(source, Some("files.read"), &input), "{source}");
    }
}

#[test]
fn a_file_of_a_share_from_a_contact_is_foreign() {
    let share = json!({ "source": { "storageId": "share:rezepte" }, "path": "notiz.txt" });
    let own = json!({ "source": { "storageId": "s3-home" }, "path": "notiz.txt" });
    let device = json!({ "source": "device", "path": "notiz.txt" });
    assert!(is_foreign("action", Some("files.read"), &share));
    assert!(!is_foreign("action", Some("files.read"), &own));
    assert!(!is_foreign("action", Some("files.read"), &device));
    let copy = json!({ "items": [{ "from": { "storageId": "share:x" } }] });
    assert!(
        is_foreign("action", Some("files.write"), &copy),
        "found anywhere in the input"
    );
}
