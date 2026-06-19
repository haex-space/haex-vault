// src-tauri/src/extension/permissions/tests/types_tests.rs
//!
//! Unit tests for the action enums in `permissions::types`.

use crate::extension::permissions::types::{Action, IdentityAction, ResourceType};
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
