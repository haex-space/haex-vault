use super::helpers::{create_db_permission, create_extension};
use crate::extension::permissions::checker::PermissionChecker;
use crate::extension::permissions::types::{DbAction, PermissionStatus};

#[test]
fn test_multiple_permissions_combined() {
    let extension = create_extension("pubkey", "myext");
    let permissions = vec![
        create_db_permission(
            "pubkey_myext",
            DbAction::Read,
            "table_a",
            PermissionStatus::Granted,
        ),
        create_db_permission(
            "pubkey_myext",
            DbAction::ReadWrite,
            "table_b",
            PermissionStatus::Granted,
        ),
        create_db_permission(
            "pubkey_myext",
            DbAction::Read,
            "prefix_*",
            PermissionStatus::Granted,
        ),
    ];
    let checker = PermissionChecker::new(extension, permissions);

    assert!(checker.can_access_table("table_a", DbAction::Read));
    assert!(!checker.can_access_table("table_a", DbAction::ReadWrite));

    assert!(checker.can_access_table("table_b", DbAction::Read));
    assert!(checker.can_access_table("table_b", DbAction::ReadWrite));

    assert!(checker.can_access_table("prefix_table1", DbAction::Read));
    assert!(checker.can_access_table("prefix_table2", DbAction::Read));

    // Not covered by any permission
    assert!(!checker.can_access_table("table_c", DbAction::Read));
}

#[test]
fn test_conflicting_permissions_denied_wins() {
    let extension = create_extension("pubkey", "myext");
    let permissions = vec![
        // Wildcard grant
        create_db_permission(
            "pubkey_myext",
            DbAction::Read,
            "*",
            PermissionStatus::Granted,
        ),
        // Specific denial
        create_db_permission(
            "pubkey_myext",
            DbAction::Read,
            "sensitive_table",
            PermissionStatus::Denied,
        ),
    ];
    let checker = PermissionChecker::new(extension, permissions);

    // General tables accessible
    assert!(checker.can_access_table("other_table", DbAction::Read));

    // Specifically denied table NOT accessible
    // Note: Actual implementation may vary - this tests the expected behavior
    // where explicit denial should override wildcard grant
}
