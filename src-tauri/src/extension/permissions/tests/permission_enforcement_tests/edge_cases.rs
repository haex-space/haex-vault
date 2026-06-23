use super::helpers::{create_db_permission, create_extension};
use crate::extension::permissions::checker::PermissionChecker;
use crate::extension::permissions::types::{DbAction, PermissionStatus};

#[test]
fn test_empty_permission_list() {
    let extension = create_extension("pubkey", "myext");
    let checker = PermissionChecker::new(extension, vec![]);

    // Only own tables accessible with empty permissions
    assert!(checker.can_access_table("pubkey__myext__users", DbAction::ReadWrite));
    assert!(!checker.can_access_table("other_table", DbAction::Read));
}

#[test]
fn test_permission_for_different_extension_ignored() {
    let extension = create_extension("pubkey", "myext");
    // Permission for a DIFFERENT extension
    let permissions = vec![create_db_permission(
        "different_ext",
        DbAction::Read,
        "shared_table",
        PermissionStatus::Granted,
    )];
    // Permission targets "different_ext" table, not our extension's own table.
    // PermissionChecker sees the permission but shared_table doesn't match our prefix.
    let checker = PermissionChecker::new(extension, permissions);
    // shared_table is not our own table (pubkey__myext__*) and is not a system table,
    // so even with an explicit grant it should be accessible (the grant is valid).
    // However, the key test is that OUR extension can't access other extension tables
    // without a matching permission target:
    assert!(!checker.can_access_table("different_ext__secret", DbAction::Read));
}

#[test]
fn test_special_characters_in_table_name() {
    let extension = create_extension("pubkey", "myext");
    let checker = PermissionChecker::new(extension, vec![]);

    // SQL injection in table names — these look like own tables but contain payloads.
    // The prefix check (starts_with "pubkey__myext__") will match the prefix but
    // the actual SQL execution would fail. The key is these don't bypass isolation.
    let malicious_names = [
        "pubkey__myext__users; DROP TABLE--",
        "pubkey__myext__users' OR '1'='1",
        "../../../etc/passwd",
    ];

    // Names with the correct prefix are considered "own tables" by the prefix check
    assert!(checker.can_access_table(malicious_names[0], DbAction::Read));
    assert!(checker.can_access_table(malicious_names[1], DbAction::Read));
    // Path traversal does NOT match the prefix → denied
    assert!(!checker.can_access_table(malicious_names[2], DbAction::Read));
}

#[test]
fn test_unicode_in_extension_identifiers() {
    let extension = create_extension("pubkey", "myext");
    let checker = PermissionChecker::new(extension, vec![]);

    // Unicode characters in table names
    assert!(checker.can_access_table("pubkey__myext__用户", DbAction::ReadWrite));
    assert!(checker.can_access_table("pubkey__myext__пользователи", DbAction::ReadWrite));
}

#[test]
fn test_case_sensitivity() {
    let extension = create_extension("PubKey", "MyExt");
    let checker = PermissionChecker::new(extension, vec![]);

    // Case should match exactly
    assert!(checker.can_access_table("PubKey__MyExt__users", DbAction::ReadWrite));
    // Different case should NOT match
    assert!(!checker.can_access_table("pubkey__myext__users", DbAction::ReadWrite));
    assert!(!checker.can_access_table("PUBKEY__MYEXT__users", DbAction::ReadWrite));
}
