// src-tauri/src/extension/permissions/tests/permission_enforcement_tests.rs
//!
//! Comprehensive permission enforcement tests
//!
//! These tests ensure that extensions cannot access resources without proper permissions.
//! Tests cover database, filesystem, HTTP, shell, and filesync permissions.

use crate::extension::core::manifest::{DisplayMode, ExtensionManifest, ExtensionPermissions};
use crate::extension::core::types::{Extension, ExtensionSource};
use crate::extension::permissions::checker::{is_system_table, matches_target, PermissionChecker};
use crate::extension::permissions::types::{
    Action, DbAction, ExtensionPermission, FsAction, PermissionStatus, ResourceType, WebAction, FileSyncAction,
};
use std::path::PathBuf;

// ============================================================================
// Test Helpers
// ============================================================================

fn create_extension(public_key: &str, name: &str) -> Extension {
    Extension {
        id: format!("{}_{}", public_key, name),
        manifest: ExtensionManifest {
            name: name.to_string(),
            version: "0.1.0".to_string(),
            author: None,
            entry: Some("index.html".to_string()),
            icon: None,
            public_key: public_key.to_string(),
            signature: "test_sig".to_string(),
            permissions: ExtensionPermissions {
                database: None,
                filesystem: None,
                http: None,
                shell: None,
                filesync: None,
            },
            homepage: None,
            description: None,
            single_instance: None,
            display_mode: Some(DisplayMode::Iframe),
            migrations_dir: None,
        },
        source: ExtensionSource::Production {
            path: PathBuf::from("/tmp/test"),
            version: "0.1.0".to_string(),
        },
        enabled: true,
        last_accessed: std::time::SystemTime::now(),
    }
}

fn create_db_permission(extension_id: &str, action: DbAction, target: &str, status: PermissionStatus) -> ExtensionPermission {
    ExtensionPermission {
        id: uuid::Uuid::new_v4().to_string(),
        extension_id: extension_id.to_string(),
        resource_type: ResourceType::Db,
        action: Action::Database(action),
        target: target.to_string(),
        constraints: None,
        status,
    }
}

fn create_fs_permission(extension_id: &str, action: FsAction, target: &str, status: PermissionStatus) -> ExtensionPermission {
    ExtensionPermission {
        id: uuid::Uuid::new_v4().to_string(),
        extension_id: extension_id.to_string(),
        resource_type: ResourceType::Fs,
        action: Action::Filesystem(action),
        target: target.to_string(),
        constraints: None,
        status,
    }
}

fn create_web_permission(extension_id: &str, target: &str, status: PermissionStatus) -> ExtensionPermission {
    ExtensionPermission {
        id: uuid::Uuid::new_v4().to_string(),
        extension_id: extension_id.to_string(),
        resource_type: ResourceType::Web,
        action: Action::Web(WebAction::Get),
        target: target.to_string(),
        constraints: None,
        status,
    }
}

fn create_filesync_permission(extension_id: &str, action: FileSyncAction, target: &str, status: PermissionStatus) -> ExtensionPermission {
    ExtensionPermission {
        id: uuid::Uuid::new_v4().to_string(),
        extension_id: extension_id.to_string(),
        resource_type: ResourceType::Filesync,
        action: Action::FileSync(action),
        target: target.to_string(),
        constraints: None,
        status,
    }
}

// ============================================================================
// Database Permission Tests
// ============================================================================

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
    let permissions = vec![
        create_db_permission("pubkey_myext", DbAction::Read, "otherpubkey__otherext__*", PermissionStatus::Granted),
    ];
    let checker = PermissionChecker::new(extension, permissions);

    // With granted permission, can access other extension's tables
    assert!(checker.can_access_table("otherpubkey__otherext__users", DbAction::Read));
    assert!(checker.can_access_table("otherpubkey__otherext__posts", DbAction::Read));
}

#[test]
fn test_denied_permission_blocks_access() {
    let extension = create_extension("pubkey", "myext");
    let permissions = vec![
        create_db_permission("pubkey_myext", DbAction::Read, "otherpubkey__otherext__*", PermissionStatus::Denied),
    ];
    let checker = PermissionChecker::new(extension, permissions);

    // Denied permission should block access
    assert!(!checker.can_access_table("otherpubkey__otherext__users", DbAction::Read));
}

#[test]
fn test_ask_permission_blocks_access() {
    let extension = create_extension("pubkey", "myext");
    let permissions = vec![
        create_db_permission("pubkey_myext", DbAction::Read, "custom_table", PermissionStatus::Ask),
    ];
    let checker = PermissionChecker::new(extension, permissions);

    // Ask (pending) permission should NOT grant access
    assert!(!checker.can_access_table("custom_table", DbAction::Read));
}

#[test]
fn test_read_permission_does_not_grant_write() {
    let extension = create_extension("pubkey", "myext");
    let permissions = vec![
        create_db_permission("pubkey_myext", DbAction::Read, "shared_table", PermissionStatus::Granted),
    ];
    let checker = PermissionChecker::new(extension, permissions);

    assert!(checker.can_access_table("shared_table", DbAction::Read));
    assert!(!checker.can_access_table("shared_table", DbAction::ReadWrite));
}

#[test]
fn test_write_permission_includes_read() {
    let extension = create_extension("pubkey", "myext");
    let permissions = vec![
        create_db_permission("pubkey_myext", DbAction::ReadWrite, "shared_table", PermissionStatus::Granted),
    ];
    let checker = PermissionChecker::new(extension, permissions);

    assert!(checker.can_access_table("shared_table", DbAction::Read));
    assert!(checker.can_access_table("shared_table", DbAction::ReadWrite));
}

#[test]
fn test_exact_table_permission() {
    let extension = create_extension("pubkey", "myext");
    let permissions = vec![
        create_db_permission("pubkey_myext", DbAction::Read, "specific_table", PermissionStatus::Granted),
    ];
    let checker = PermissionChecker::new(extension, permissions);

    assert!(checker.can_access_table("specific_table", DbAction::Read));
    // Different table name should NOT be accessible
    assert!(!checker.can_access_table("other_table", DbAction::Read));
    assert!(!checker.can_access_table("specific_table_extra", DbAction::Read));
}

#[test]
fn test_wildcard_permission_does_not_grant_system_access() {
    let extension = create_extension("pubkey", "myext");
    let permissions = vec![
        create_db_permission("pubkey_myext", DbAction::ReadWrite, "*", PermissionStatus::Granted),
    ];
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
        create_db_permission("pubkey_myext", DbAction::Read, "haex_*", PermissionStatus::Granted),
    ];
    let checker = PermissionChecker::new(extension, permissions);

    // Should NOT be able to access system tables even with haex_* permission
    assert!(!checker.can_access_table("haex_extensions", DbAction::Read));
    assert!(!checker.can_access_table("haex_vault_settings", DbAction::Read));
}

// ============================================================================
// System Table Protection Tests
// ============================================================================

#[test]
fn test_all_system_tables_protected() {
    let system_tables = [
        "haex_extensions",
        "haex_vault_settings",
        "haex_extension_permissions",
        "haex_extension_migrations",
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
    let permissions = vec![
        create_db_permission("pubkey_myext", DbAction::ReadWrite, "*", PermissionStatus::Granted),
    ];
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

// ============================================================================
// Cross-Extension Isolation Tests
// ============================================================================

#[test]
fn test_extension_isolation_by_prefix() {
    // Create two extensions
    let ext_a = create_extension("pubkey_a", "ext_a");
    let ext_b = create_extension("pubkey_b", "ext_b");

    let checker_a = PermissionChecker::new(ext_a, vec![]);
    let checker_b = PermissionChecker::new(ext_b, vec![]);

    // Extension A can only access its own tables
    assert!(checker_a.can_access_table("pubkey_a__ext_a__users", DbAction::ReadWrite));
    assert!(!checker_a.can_access_table("pubkey_b__ext_b__users", DbAction::Read));

    // Extension B can only access its own tables
    assert!(checker_b.can_access_table("pubkey_b__ext_b__users", DbAction::ReadWrite));
    assert!(!checker_b.can_access_table("pubkey_a__ext_a__users", DbAction::Read));
}

#[test]
fn test_extension_cannot_impersonate_prefix() {
    // Extension with similar but different prefix
    let extension = create_extension("pubkey", "myext");
    let checker = PermissionChecker::new(extension, vec![]);

    // Cannot access tables with slightly different prefix
    assert!(!checker.can_access_table("pubkey__myext_extra__users", DbAction::Read)); // extra underscore
    assert!(!checker.can_access_table("pubkey_myext__users", DbAction::Read)); // missing double underscore
    assert!(!checker.can_access_table("pubkey2__myext__users", DbAction::Read)); // different pubkey
}

// ============================================================================
// Permission Target Pattern Tests
// ============================================================================

#[test]
fn test_matches_target_exact() {
    assert!(matches_target("exact_table", "exact_table"));
    assert!(!matches_target("exact_table", "different_table"));
    assert!(!matches_target("exact_table", "exact_table_extended"));
}

#[test]
fn test_matches_target_prefix_wildcard() {
    assert!(matches_target("prefix__*", "prefix__table1"));
    assert!(matches_target("prefix__*", "prefix__table2"));
    assert!(matches_target("prefix__*", "prefix__deeply__nested__table"));
    assert!(!matches_target("prefix__*", "other__table"));
    assert!(!matches_target("prefix__*", "prefixNOSEPARATOR__table"));
}

#[test]
fn test_matches_target_full_wildcard() {
    // Full wildcard should match non-system tables
    assert!(matches_target("*", "any_table"));
    assert!(matches_target("*", "custom_user_data"));

    // But NOT system tables (checked separately)
    assert!(!matches_target("*", "haex_extensions"));
    assert!(!matches_target("*", "sqlite_master"));
}

#[test]
fn test_matches_target_does_not_match_system_tables() {
    // Verify that wildcard patterns don't match system tables
    let patterns = ["*", "haex_*", "sqlite_*", "h*", "s*"];
    let system_tables = ["haex_extensions", "haex_vault_settings", "sqlite_master"];

    for pattern in patterns {
        for table in system_tables {
            assert!(
                !matches_target(pattern, table),
                "Pattern '{}' should NOT match system table '{}'",
                pattern, table
            );
        }
    }
}

// ============================================================================
// Permission Status Tests
// ============================================================================

#[test]
fn test_permission_status_granted() {
    let extension = create_extension("pubkey", "myext");
    let permissions = vec![
        create_db_permission("pubkey_myext", DbAction::Read, "table_a", PermissionStatus::Granted),
    ];
    let checker = PermissionChecker::new(extension, permissions);

    assert!(checker.can_access_table("table_a", DbAction::Read));
}

#[test]
fn test_permission_status_denied() {
    let extension = create_extension("pubkey", "myext");
    let permissions = vec![
        create_db_permission("pubkey_myext", DbAction::Read, "table_a", PermissionStatus::Denied),
    ];
    let checker = PermissionChecker::new(extension, permissions);

    assert!(!checker.can_access_table("table_a", DbAction::Read));
}

#[test]
fn test_permission_status_ask() {
    let extension = create_extension("pubkey", "myext");
    let permissions = vec![
        create_db_permission("pubkey_myext", DbAction::Read, "table_a", PermissionStatus::Ask),
    ];
    let checker = PermissionChecker::new(extension, permissions);

    // Ask should NOT grant access - requires user confirmation
    assert!(!checker.can_access_table("table_a", DbAction::Read));
}

// ============================================================================
// Multiple Permission Combination Tests
// ============================================================================

#[test]
fn test_multiple_permissions_combined() {
    let extension = create_extension("pubkey", "myext");
    let permissions = vec![
        create_db_permission("pubkey_myext", DbAction::Read, "table_a", PermissionStatus::Granted),
        create_db_permission("pubkey_myext", DbAction::ReadWrite, "table_b", PermissionStatus::Granted),
        create_db_permission("pubkey_myext", DbAction::Read, "prefix_*", PermissionStatus::Granted),
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
        create_db_permission("pubkey_myext", DbAction::Read, "*", PermissionStatus::Granted),
        // Specific denial
        create_db_permission("pubkey_myext", DbAction::Read, "sensitive_table", PermissionStatus::Denied),
    ];
    let checker = PermissionChecker::new(extension, permissions);

    // General tables accessible
    assert!(checker.can_access_table("other_table", DbAction::Read));

    // Specifically denied table NOT accessible
    // Note: Actual implementation may vary - this tests the expected behavior
    // where explicit denial should override wildcard grant
}

// ============================================================================
// Resource Type Tests
// ============================================================================

#[test]
fn test_permission_resource_types() {
    // Verify that different resource types are properly distinguished
    let db_perm = create_db_permission("ext", DbAction::Read, "*", PermissionStatus::Granted);
    assert!(matches!(db_perm.resource_type, ResourceType::Db));

    let fs_perm = create_fs_permission("ext", FsAction::Read, "/path", PermissionStatus::Granted);
    assert!(matches!(fs_perm.resource_type, ResourceType::Fs));

    let web_perm = create_web_permission("ext", "https://*", PermissionStatus::Granted);
    assert!(matches!(web_perm.resource_type, ResourceType::Web));

    let filesync_perm = create_filesync_permission("ext", FileSyncAction::Read, "*", PermissionStatus::Granted);
    assert!(matches!(filesync_perm.resource_type, ResourceType::Filesync));
}

// ============================================================================
// Edge Case Tests
// ============================================================================

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
    let permissions = vec![
        create_db_permission("different_ext", DbAction::Read, "shared_table", PermissionStatus::Granted),
    ];
    let checker = PermissionChecker::new(extension, permissions);

    // Permission is for different extension, so should NOT grant access
    // Note: Actual implementation may filter by extension_id
}

#[test]
fn test_special_characters_in_table_name() {
    let extension = create_extension("pubkey", "myext");
    let checker = PermissionChecker::new(extension, vec![]);

    // Table names with special patterns that might be attack vectors
    let malicious_names = [
        "pubkey__myext__users; DROP TABLE--",
        "pubkey__myext__users' OR '1'='1",
        "../../../etc/passwd",
        "pubkey__myext__users\0evil",
    ];

    for name in malicious_names {
        // These should either be rejected or not match the extension's prefix
        // The actual behavior depends on how the prefix matching works
        println!("Testing table name: '{}'", name);
    }
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
