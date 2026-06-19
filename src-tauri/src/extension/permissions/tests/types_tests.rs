// src-tauri/src/extension/permissions/tests/types_tests.rs
//!
//! Unit tests for the action enums in `permissions::types`.

use crate::extension::permissions::types::{
    combine_constraints, split_constraints, split_constraints_value, Action, IdentityAction,
    PermissionConstraints, ResourceType,
};
use serde_json::json;
use std::str::FromStr;

#[test]
fn identity_action_from_str_read_and_write() {
    assert_eq!(
        IdentityAction::from_str("read").unwrap(),
        IdentityAction::Read
    );
    assert_eq!(
        IdentityAction::from_str("write").unwrap(),
        IdentityAction::Write
    );
    assert!(IdentityAction::from_str("delete").is_err());
}

#[test]
fn identity_action_read_and_write_are_distinct_capabilities() {
    // Read grants read but not write.
    assert!(IdentityAction::Read.allows_read());
    assert!(!IdentityAction::Read.allows_write());

    // Write grants write but NOT read — no hierarchy.
    assert!(IdentityAction::Write.allows_write());
    assert!(!IdentityAction::Write.allows_read());
}

#[test]
fn identity_action_as_str_roundtrip() {
    assert_eq!(IdentityAction::Read.as_str(), "read");
    assert_eq!(IdentityAction::Write.as_str(), "write");
}

#[test]
fn action_from_str_resolves_identity_write() {
    let action = Action::from_str(&ResourceType::Identities, "write").unwrap();
    assert_eq!(action, Action::Identities(IdentityAction::Write));
}

// --- Constraint split/combine helpers (passwords-vs-other invariant) ---
//
// These helpers are the single source of truth for the passwords default-label
// rule and back the live `check_passwords_permission` read path (via
// `split_constraints` in `get_permissions` and `From<HaexPrincipalPermissions>`).

#[test]
fn split_constraints_passwords_keeps_default_marker_raw() {
    // passwords + `{"default":true}` text -> (typed None, raw Some marker).
    let (typed, raw) = split_constraints(ResourceType::Passwords, Some(r#"{"default":true}"#));
    assert!(typed.is_none(), "passwords must not parse into typed enum");
    assert_eq!(raw, Some(json!({ "default": true })));
}

#[test]
fn split_constraints_passwords_null_yields_none() {
    // passwords + null/absent text -> (None, None).
    let (typed, raw) = split_constraints(ResourceType::Passwords, None);
    assert!(typed.is_none());
    assert!(raw.is_none());
}

#[test]
fn split_constraints_non_passwords_parses_typed() {
    // A non-passwords row (db) with real constraints -> (typed Some, raw None).
    // NOTE: `PermissionConstraints` is `#[serde(untagged)]`; an object whose
    // shape matches `DbConstraints` (the first variant) deserializes as
    // `Database(..)`. We use db-shaped JSON here precisely so the typed value is
    // unambiguous and round-trips losslessly.
    let (typed, raw) = split_constraints(
        ResourceType::Db,
        Some(r#"{"where_clause":"id > 0","limit":10}"#),
    );
    assert!(raw.is_none(), "non-passwords rows never carry raw");
    match typed {
        Some(PermissionConstraints::Database(db)) => {
            assert_eq!(db.where_clause.as_deref(), Some("id > 0"));
            assert_eq!(db.limit, Some(10));
        }
        other => panic!("expected typed Database constraints, got {other:?}"),
    }
}

#[test]
fn split_constraints_value_passwords_keeps_default_marker_raw() {
    // Manifest path: input is already a Value. passwords -> raw clone, typed None.
    let (typed, raw) =
        split_constraints_value(ResourceType::Passwords, Some(&json!({ "default": true })));
    assert!(typed.is_none());
    assert_eq!(raw, Some(json!({ "default": true })));
}

#[test]
fn split_constraints_value_non_passwords_parses_typed() {
    let value = json!({ "where_clause": "id > 0", "limit": 10 });
    let (typed, raw) = split_constraints_value(ResourceType::Db, Some(&value));
    assert!(raw.is_none());
    assert!(matches!(typed, Some(PermissionConstraints::Database(_))));
}

#[test]
fn split_then_combine_roundtrip_passwords() {
    // Round-trip: split the DB text, then re-combine on the write side yields the
    // same DB constraints text for a passwords row.
    let text = r#"{"default":true}"#;
    let (typed, raw) = split_constraints(ResourceType::Passwords, Some(text));
    let combined = combine_constraints(typed.as_ref(), raw.as_ref()).expect("combined text");
    // Compare structurally — key order in the re-serialized JSON is irrelevant.
    let before: serde_json::Value = serde_json::from_str(text).unwrap();
    let after: serde_json::Value = serde_json::from_str(&combined).unwrap();
    assert_eq!(before, after);
}

#[test]
fn split_then_combine_roundtrip_non_passwords() {
    // Same round-trip for a non-passwords (db) row. Db-shaped JSON round-trips
    // losslessly through the untagged enum (see note above).
    let text = r#"{"where_clause":"id > 0","limit":10}"#;
    let (typed, raw) = split_constraints(ResourceType::Db, Some(text));
    let combined = combine_constraints(typed.as_ref(), raw.as_ref()).expect("combined text");
    let before: serde_json::Value = serde_json::from_str(text).unwrap();
    let after: serde_json::Value = serde_json::from_str(&combined).unwrap();
    assert_eq!(before, after);
}

#[test]
fn combine_constraints_prefers_raw_over_typed() {
    // The write side prefers raw (passwords) when both are somehow present, so
    // pass BOTH and assert raw still wins (otherwise precedence is never tested).
    let typed =
        PermissionConstraints::Database(crate::extension::permissions::types::DbConstraints {
            where_clause: Some("id > 0".to_string()),
            columns: None,
            limit: Some(10),
        });
    let raw = json!({ "default": true });
    let combined = combine_constraints(Some(&typed), Some(&raw)).expect("combined text");
    let parsed: serde_json::Value = serde_json::from_str(&combined).unwrap();
    assert_eq!(parsed, raw);
}
