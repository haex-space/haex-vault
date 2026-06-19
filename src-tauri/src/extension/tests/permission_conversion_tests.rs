// src-tauri/src/extension/tests/permission_conversion_tests.rs
//!
//! Tests for the manifest -> internal -> DB permission conversion, focused on
//! preserving the passwords default-label marker (`{"default":true}`) end to
//! end. The typed (untagged) `PermissionConstraints` enum cannot represent that
//! free-form JSON, so passwords constraints are carried raw and must survive to
//! the DB `constraints` column intact.

use crate::database::generated::HaexPrincipalPermissions;
use crate::extension::core::manifest::{ExtensionPermissions, PermissionEntry};
use crate::extension::permissions::types::ResourceType;
use serde_json::json;

fn passwords_entry(target: &str, default: bool) -> PermissionEntry {
    PermissionEntry {
        target: target.to_string(),
        operation: Some("read_write".to_string()),
        constraints: if default {
            Some(json!({ "default": true }))
        } else {
            None
        },
        status: None,
    }
}

#[test]
fn passwords_default_marker_survives_manifest_to_internal() {
    // The raw `{"default":true}` JSON must land in `raw_constraints` (the typed
    // enum can't hold it) and `constraints` must stay None.
    let perms = ExtensionPermissions {
        passwords: Some(vec![passwords_entry("work", true)]),
        ..Default::default()
    };

    let internal = perms.to_internal_permissions("ext_1");
    assert_eq!(internal.len(), 1);
    let perm = &internal[0];
    assert_eq!(perm.resource_type, ResourceType::Passwords);
    assert!(
        perm.constraints.is_none(),
        "typed constraints must be None for passwords"
    );
    assert_eq!(perm.raw_constraints, Some(json!({ "default": true })));
}

#[test]
fn passwords_default_marker_survives_internal_to_db() {
    // The DB `constraints` column string must be exactly the raw default marker.
    let perms = ExtensionPermissions {
        passwords: Some(vec![passwords_entry("work", true)]),
        ..Default::default()
    };

    let internal = perms.to_internal_permissions("ext_1");
    let db_perm: HaexPrincipalPermissions = (&internal[0]).into();

    let stored: serde_json::Value =
        serde_json::from_str(db_perm.constraints.as_deref().expect("constraints set"))
            .expect("valid json");
    assert_eq!(stored, json!({ "default": true }));
}

#[test]
fn passwords_without_marker_has_no_constraints() {
    // A non-default passwords row carries no constraints at all.
    let perms = ExtensionPermissions {
        passwords: Some(vec![passwords_entry("personal", false)]),
        ..Default::default()
    };

    let internal = perms.to_internal_permissions("ext_1");
    let perm = &internal[0];
    assert!(perm.constraints.is_none());
    assert!(perm.raw_constraints.is_none());

    let db_perm: HaexPrincipalPermissions = perm.into();
    assert!(db_perm.constraints.is_none());
}

#[test]
fn passwords_db_roundtrip_preserves_marker() {
    // DB struct -> ExtensionPermission must restore the raw marker so a
    // get -> edit -> update UI round-trip never drops the default.
    let perms = ExtensionPermissions {
        passwords: Some(vec![passwords_entry("work", true)]),
        ..Default::default()
    };
    let internal = perms.to_internal_permissions("ext_1");
    let db_perm: HaexPrincipalPermissions = (&internal[0]).into();

    let restored: crate::extension::permissions::types::ExtensionPermission = db_perm.into();
    assert_eq!(restored.resource_type, ResourceType::Passwords);
    assert!(restored.constraints.is_none());
    assert_eq!(restored.raw_constraints, Some(json!({ "default": true })));
}
