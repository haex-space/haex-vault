use super::helpers::{create_db_permission, create_extension};
use crate::extension::permissions::checker::PermissionChecker;
use crate::extension::permissions::types::{DbAction, PermissionStatus};

#[test]
fn test_no_db_access_without_permission() {
    let extension = create_extension("pubkey", "myext");
    let checker = PermissionChecker::new(extension, vec![]);

    // Extension without permissions cannot access arbitrary tables
    assert!(!checker.can_access_table("random_table", DbAction::Read));
    assert!(!checker.can_access_table("random_table", DbAction::ReadWrite));
}

#[test]
fn test_own_tables_always_accessible() {
    let extension = create_extension("pubkey", "myext");
    let checker = PermissionChecker::new(extension, vec![]);

    // Extension can ALWAYS access its own tables (prefix-based)
    assert!(checker.can_access_table("pubkey__myext__users", DbAction::Read));
    assert!(checker.can_access_table("pubkey__myext__users", DbAction::ReadWrite));
    assert!(checker.can_access_table("pubkey__myext__any_table_name", DbAction::ReadWrite));
}

#[test]
fn test_other_extension_tables_require_permission() {
    let extension = create_extension("pubkey", "myext");
    let checker = PermissionChecker::new(extension, vec![]);

    // Cannot access other extension's tables without permission
    assert!(!checker.can_access_table("otherpubkey__otherext__users", DbAction::Read));
    assert!(!checker.can_access_table("anotherpubkey__anotherext__data", DbAction::Read));
}

#[test]
fn test_granted_permission_allows_access() {
    let extension = create_extension("pubkey", "myext");
    let permissions = vec![create_db_permission(
        "pubkey_myext",
        DbAction::Read,
        "otherpubkey__otherext__*",
        PermissionStatus::Granted,
    )];
    let checker = PermissionChecker::new(extension, permissions);

    // With granted permission, can access other extension's tables
    assert!(checker.can_access_table("otherpubkey__otherext__users", DbAction::Read));
    assert!(checker.can_access_table("otherpubkey__otherext__posts", DbAction::Read));
}

#[test]
fn test_denied_permission_blocks_access() {
    let extension = create_extension("pubkey", "myext");
    let permissions = vec![create_db_permission(
        "pubkey_myext",
        DbAction::Read,
        "otherpubkey__otherext__*",
        PermissionStatus::Denied,
    )];
    let checker = PermissionChecker::new(extension, permissions);

    // Denied permission should block access
    assert!(!checker.can_access_table("otherpubkey__otherext__users", DbAction::Read));
}

#[test]
fn test_ask_permission_blocks_access() {
    let extension = create_extension("pubkey", "myext");
    let permissions = vec![create_db_permission(
        "pubkey_myext",
        DbAction::Read,
        "custom_table",
        PermissionStatus::Ask,
    )];
    let checker = PermissionChecker::new(extension, permissions);

    // Ask (pending) permission should NOT grant access
    assert!(!checker.can_access_table("custom_table", DbAction::Read));
}

#[test]
fn test_read_permission_does_not_grant_write() {
    let extension = create_extension("pubkey", "myext");
    let permissions = vec![create_db_permission(
        "pubkey_myext",
        DbAction::Read,
        "shared_table",
        PermissionStatus::Granted,
    )];
    let checker = PermissionChecker::new(extension, permissions);

    assert!(checker.can_access_table("shared_table", DbAction::Read));
    assert!(!checker.can_access_table("shared_table", DbAction::ReadWrite));
}

#[test]
fn test_write_permission_includes_read() {
    let extension = create_extension("pubkey", "myext");
    let permissions = vec![create_db_permission(
        "pubkey_myext",
        DbAction::ReadWrite,
        "shared_table",
        PermissionStatus::Granted,
    )];
    let checker = PermissionChecker::new(extension, permissions);

    assert!(checker.can_access_table("shared_table", DbAction::Read));
    assert!(checker.can_access_table("shared_table", DbAction::ReadWrite));
}

#[test]
fn test_exact_table_permission() {
    let extension = create_extension("pubkey", "myext");
    let permissions = vec![create_db_permission(
        "pubkey_myext",
        DbAction::Read,
        "specific_table",
        PermissionStatus::Granted,
    )];
    let checker = PermissionChecker::new(extension, permissions);

    assert!(checker.can_access_table("specific_table", DbAction::Read));
    // Different table name should NOT be accessible
    assert!(!checker.can_access_table("other_table", DbAction::Read));
    assert!(!checker.can_access_table("specific_table_extra", DbAction::Read));
}

#[test]
fn test_wildcard_permission_does_not_grant_system_access() {
    let extension = create_extension("pubkey", "myext");
    let permissions = vec![create_db_permission(
        "pubkey_myext",
        DbAction::ReadWrite,
        "*",
        PermissionStatus::Granted,
    )];
    let checker = PermissionChecker::new(extension, permissions);

    // Wildcard should allow access to non-system tables
    assert!(checker.can_access_table("custom_table", DbAction::ReadWrite));

    // But NOT to system tables
    assert!(!checker.can_access_table("haex_extensions", DbAction::Read));
    assert!(!checker.can_access_table("haex_vault_settings", DbAction::Read));
    assert!(!checker.can_access_table("sqlite_master", DbAction::Read));
}

#[test]
fn test_prefix_wildcard_cannot_access_system_prefix() {
    let extension = create_extension("pubkey", "myext");
    let permissions = vec![
        // Try to get haex_* access via wildcard
        create_db_permission(
            "pubkey_myext",
            DbAction::Read,
            "haex_*",
            PermissionStatus::Granted,
        ),
    ];
    let checker = PermissionChecker::new(extension, permissions);

    // Should NOT be able to access system tables even with haex_* permission
    assert!(!checker.can_access_table("haex_extensions", DbAction::Read));
    assert!(!checker.can_access_table("haex_vault_settings", DbAction::Read));
}
