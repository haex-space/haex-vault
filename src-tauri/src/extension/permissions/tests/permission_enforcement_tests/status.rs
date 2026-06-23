use super::helpers::{create_db_permission, create_extension};
use crate::extension::permissions::checker::PermissionChecker;
use crate::extension::permissions::types::{DbAction, PermissionStatus};

#[test]
fn test_permission_status_granted() {
    let extension = create_extension("pubkey", "myext");
    let permissions = vec![create_db_permission(
        "pubkey_myext",
        DbAction::Read,
        "table_a",
        PermissionStatus::Granted,
    )];
    let checker = PermissionChecker::new(extension, permissions);

    assert!(checker.can_access_table("table_a", DbAction::Read));
}

#[test]
fn test_permission_status_denied() {
    let extension = create_extension("pubkey", "myext");
    let permissions = vec![create_db_permission(
        "pubkey_myext",
        DbAction::Read,
        "table_a",
        PermissionStatus::Denied,
    )];
    let checker = PermissionChecker::new(extension, permissions);

    assert!(!checker.can_access_table("table_a", DbAction::Read));
}

#[test]
fn test_permission_status_ask() {
    let extension = create_extension("pubkey", "myext");
    let permissions = vec![create_db_permission(
        "pubkey_myext",
        DbAction::Read,
        "table_a",
        PermissionStatus::Ask,
    )];
    let checker = PermissionChecker::new(extension, permissions);

    // Ask should NOT grant access - requires user confirmation
    assert!(!checker.can_access_table("table_a", DbAction::Read));
}
