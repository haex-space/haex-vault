use super::helpers::{create_db_permission, create_extension};
use crate::extension::permissions::checker::{is_system_table, PermissionChecker};
use crate::extension::permissions::types::{DbAction, PermissionStatus};

#[test]
fn test_all_system_tables_protected() {
    let system_tables = [
        "haex_extensions",
        "haex_vault_settings",
        "haex_principal_permissions",
        crate::table_names::TABLE_EXTENSION_MIGRATIONS,
        "haex_crdt_migrations",
        "haex_crdt_tombstones",
        "haex_filesync_backends",
        "haex_filesync_spaces",
        "haex_filesync_files",
        "haex_filesync_sync_rules",
        "haex_authorized_clients",
        "haex_blocked_clients",
        "sqlite_master",
        "sqlite_sequence",
        "sqlite_stat1",
    ];

    let extension = create_extension("pubkey", "myext");
    // Give wildcard permission
    let permissions = vec![create_db_permission(
        "pubkey_myext",
        DbAction::ReadWrite,
        "*",
        PermissionStatus::Granted,
    )];
    let checker = PermissionChecker::new(extension, permissions);

    for table in system_tables {
        assert!(
            is_system_table(table),
            "Table '{}' should be recognized as system table",
            table
        );
        assert!(
            !checker.can_access_table(table, DbAction::Read),
            "Should NOT be able to access system table '{}' even with wildcard permission",
            table
        );
    }
}
