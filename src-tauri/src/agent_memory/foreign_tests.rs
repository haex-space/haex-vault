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
        assert!(!is_foreign("action", Some(scope), "", &input), "{scope}");
    }
    assert!(!is_foreign("action", None, "find_actions", &input));
    assert!(!is_foreign("action", None, "ask_user", &input));
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
        assert!(is_foreign("action", Some(scope), "", &input), "{scope}");
    }
    assert!(
        is_foreign("action", None, "unknown", &input),
        "an action without a scope"
    );
    for source in ["haextension", "mcp", "cli", "something new"] {
        assert!(
            is_foreign(source, Some("files.read"), "ask_user", &input),
            "{source}"
        );
    }
}

#[test]
fn a_file_of_a_share_from_a_contact_is_foreign() {
    let share = json!({ "source": "storage:share:rezepte", "path": "notiz.txt" });
    let share_name = json!({ "source": "storage:Rezepte", "path": "notiz.txt" });
    let device = json!({ "source": "device", "path": "notiz.txt" });
    assert!(is_foreign(
        "action",
        Some("files.read"),
        "files_read",
        &share
    ));
    assert!(is_foreign(
        "action",
        Some("files.read"),
        "files_read",
        &share_name
    ));
    assert!(!is_foreign(
        "action",
        Some("files.read"),
        "files_read",
        &device
    ));
    let copy = json!({ "source": "device", "toSource": "storage:share:x" });
    assert!(
        is_foreign("action", Some("files.write"), "files_copy", &copy),
        "found anywhere in the input"
    );
}
