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
