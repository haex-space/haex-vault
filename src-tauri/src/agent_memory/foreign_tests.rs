//! Tests for what counts as foreign content (spec 097 research R6, T050, T054).

use serde_json::json;

use super::{is_foreign, names_remote_storage};

#[test]
fn own_actions_are_not_foreign() {
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
        assert!(!is_foreign("action", Some(scope), "", false), "{scope}");
    }
    assert!(!is_foreign("action", None, "find_actions", false));
    assert!(!is_foreign("action", None, "ask_user", false));
}

#[test]
fn the_web_received_messages_and_anything_unknown_are_foreign() {
    for scope in [
        "browser",
        "chat.read",
        "files.shares",
        "newScope",
        "guardrails",
    ] {
        assert!(is_foreign("action", Some(scope), "", false), "{scope}");
    }
    assert!(
        is_foreign("action", None, "unknown", false),
        "an action without a scope"
    );
    for source in ["haextension", "mcp", "cli", "something new"] {
        assert!(
            is_foreign(source, Some("files.read"), "ask_user", false),
            "{source}"
        );
    }
}

#[test]
fn a_file_of_a_share_from_a_contact_is_foreign() {
    let share = json!({ "source": "storage:share:rezepte", "path": "notiz.txt" });
    let share_name = json!({ "source": "storage:Rezepte", "path": "notiz.txt" });
    let device = json!({ "source": "device", "path": "notiz.txt" });
    assert!(names_remote_storage(&share));
    assert!(names_remote_storage(&share_name));
    assert!(!names_remote_storage(&device));
    let copy = json!({ "source": "device", "toSource": "storage:share:x" });
    assert!(names_remote_storage(&copy));
    assert!(is_foreign("action", Some("files.read"), "files_read", true));
    assert!(!is_foreign(
        "action",
        Some("files.read"),
        "files_read",
        false
    ));
}
