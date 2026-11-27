//! Unit tests for the preference-scope wire conversion + key
//! validation. DB-behaviour of get/set/clear is covered by the
//! integration suite (`tests/preferences_roundtrip.rs`); these tests
//! ensure the wire-shape rejections keep the sync semantics safe.

use uuid::Uuid;

use super::preferences_commands::{validate_value, PrefScopeWire};
use crate::identity::VAULT_SCOPE_UUID;
use crate::storage::preferences::PrefScope;

#[test]
fn vault_wire_maps_to_vault_scope() {
    let scope = PrefScope::try_from(PrefScopeWire::Vault).expect("vault scope");
    assert_eq!(scope, PrefScope::Vault);
}

#[test]
fn device_wire_maps_to_device_scope() {
    let uuid = Uuid::new_v4();
    let scope = PrefScope::try_from(PrefScopeWire::Device { uuid }).expect("device scope");
    assert_eq!(scope, PrefScope::Device(uuid));
}

#[test]
fn device_wire_rejects_the_nil_uuid() {
    let err = PrefScope::try_from(PrefScopeWire::Device {
        uuid: VAULT_SCOPE_UUID,
    })
    .expect_err("nil UUID must be rejected in device scope");
    assert!(format!("{err:?}").contains("nil UUID"));
}

#[test]
fn the_clipboard_setting_takes_only_its_choices() {
    for value in ["0", "15", "30", "60", "120"] {
        validate_value("passwords.clipboard_clear_seconds", value).expect("a choice");
    }
    for value in ["", "10", "-1", "abc", "300"] {
        let err =
            validate_value("passwords.clipboard_clear_seconds", value).expect_err("not a choice");
        assert!(format!("{err:?}").contains("InvalidInput"));
    }
    // Other keys take any value.
    validate_value("chat.permission_mode", "whatever").expect("unrelated key");
}

#[test]
fn the_sync_log_depth_takes_a_whole_number_up_to_a_hundred() {
    for value in ["0", "10", "100"] {
        validate_value("syncRules.logDepth", value).expect("a depth");
    }
    for value in ["", "-1", "101", "zehn"] {
        validate_value("syncRules.logDepth", value).expect_err("not a depth");
    }
}

#[test]
fn the_chat_file_server_is_none_or_an_address_a_file_may_go_to() {
    for value in [
        "",
        "  ",
        "https://blossom.example.com/",
        "http://127.0.0.1:3000",
    ] {
        validate_value("chat.blossom.server", value).expect("none or an address");
    }
    for value in [
        "http://blossom.example.com/",
        "https://user:secret@blossom.example.com/",
        "blossom.example.com",
    ] {
        validate_value("chat.blossom.server", value).expect_err("not an address");
    }
}

#[test]
fn the_added_blossom_servers_are_a_list_of_addresses_with_a_short_name() {
    for value in [
        "[]",
        r#"[{"url":"https://files.example.com"}]"#,
        r#"[{"url":"https://files.example.com","name":"Mein Server"}]"#,
        // Saved before the name was called a name.
        r#"[{"url":"https://files.example.com","badge":"Mein Server"}]"#,
    ] {
        validate_value("chat.blossom.servers", value).expect("a list of servers");
    }
    let too_long = format!(
        r#"[{{"url":"https://files.example.com","name":"{}"}}]"#,
        "x".repeat(41)
    );
    for value in [
        "",
        "https://files.example.com",
        r#"[{"url":"http://files.example.com"}]"#,
        r#"[{"url":"https://user:secret@files.example.com"}]"#,
        r#"[{"address":"https://files.example.com"}]"#,
        too_long.as_str(),
    ] {
        validate_value("chat.blossom.servers", value).expect_err("not a list of servers");
    }
    let many = format!(
        "[{}]",
        vec![r#"{"url":"https://files.example.com"}"#; 21].join(",")
    );
    validate_value("chat.blossom.servers", &many).expect_err("too many");
}

#[test]
fn the_chat_switches_take_true_or_false_and_the_download_limit_a_number() {
    for key in ["chat.receipts.read", "chat.notifications"] {
        for value in ["true", "false"] {
            validate_value(key, value).expect("a switch");
        }
        for value in ["", "yes", "1", "True"] {
            validate_value(key, value).expect_err("not a switch");
        }
    }
    for value in ["0", "5000000", "209715200"] {
        validate_value("chat.download.autoLimit", value).expect("a limit");
    }
    for value in ["", "-1", "25 MB", "1.5"] {
        validate_value("chat.download.autoLimit", value).expect_err("not a limit");
    }
}

#[test]
fn the_background_takes_only_a_webp_data_url_within_the_limit() {
    let key = "appearance.background";
    validate_value(key, "data:image/webp;base64,UklGRg==").expect("a WebP data URL");
    let too_large = format!("data:image/webp;base64,{}", "A".repeat(4 * 1024 * 1024));
    for value in [
        "",
        "https://example.com/image.webp",
        "data:image/png;base64,iVBORw0KGgo=",
        "data:image/webp;base64,",
        "data:image/webp;base64,UklGRg==\") ; color: red",
        too_large.as_str(),
    ] {
        let err = validate_value(key, value).expect_err("not a background");
        assert!(format!("{err:?}").contains("InvalidInput"));
    }
}

#[test]
fn file_browser_preferences_take_only_their_choices() {
    for (key, value) in [
        ("files.view", "list"),
        ("files.view", "grid"),
        ("files.hidden", "true"),
        ("files.hidden", "false"),
        ("files.sort", "name:asc"),
        ("files.sort", "modified:desc"),
        ("files.iconSet", "builtin:lucide"),
        ("files.iconSet", "builtin:phosphor"),
        (
            "files.iconSet",
            "custom:0b9a2c52-6a53-4f4e-9a1b-3c1a1d0a77aa",
        ),
    ] {
        validate_value(key, value).expect("a choice");
    }
    for (key, value) in [
        ("files.view", "tiles"),
        ("files.hidden", "yes"),
        ("files.sort", "name"),
        ("files.sort", "colour:asc"),
        ("files.sort", "size:up"),
        ("files.iconSet", "builtin:future"),
        ("files.iconSet", "custom:not-a-uuid"),
        ("files.iconSet", "phosphor"),
    ] {
        validate_value(key, value).expect_err("not a choice");
    }
}
