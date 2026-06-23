//! Table-prefix validation and cross-extension / system-table boundaries.
//!
//! Every extension is namespaced as `<pubkey>__<ext>__<table>`. These
//! tests pin that boundary on CREATE / ALTER / CREATE INDEX / DROP TABLE
//! (including quoted variants) and verify cross-extension / system-table
//! access is blocked at the `PermissionChecker` level.

use super::helpers::{create_test_context, get_expected_prefix};
use crate::extension::database::helpers::validate_sql_table_prefix;
use crate::extension::permissions::checker::{is_system_table, matches_target, PermissionChecker};

#[test]
fn test_table_prefix_validation_create_table() {
    let ctx = create_test_context();
    let expected = get_expected_prefix();

    // Valid: correct prefix
    let valid_sql = format!("CREATE TABLE {}users (id TEXT PRIMARY KEY)", expected);
    assert!(validate_sql_table_prefix(&ctx, &valid_sql).is_ok());

    // Invalid: wrong prefix
    let invalid_sql = "CREATE TABLE other_extension__table (id TEXT PRIMARY KEY)";
    assert!(validate_sql_table_prefix(&ctx, invalid_sql).is_err());

    // Invalid: no prefix
    let no_prefix = "CREATE TABLE users (id TEXT PRIMARY KEY)";
    assert!(validate_sql_table_prefix(&ctx, no_prefix).is_err());

    // Invalid: system table prefix
    let system_prefix = "CREATE TABLE haex_extensions_copy (id TEXT PRIMARY KEY)";
    assert!(validate_sql_table_prefix(&ctx, system_prefix).is_err());
}

#[test]
fn test_table_prefix_validation_alter_table() {
    let ctx = create_test_context();
    let expected = get_expected_prefix();

    // Valid
    let valid_sql = format!("ALTER TABLE {}users ADD COLUMN email TEXT", expected);
    assert!(validate_sql_table_prefix(&ctx, &valid_sql).is_ok());

    // Invalid
    let invalid_sql = "ALTER TABLE haex_extensions ADD COLUMN evil TEXT";
    assert!(validate_sql_table_prefix(&ctx, invalid_sql).is_err());
}

#[test]
fn test_table_prefix_validation_create_index() {
    let ctx = create_test_context();
    let expected = get_expected_prefix();

    // Valid
    let valid_sql = format!("CREATE INDEX idx_users ON {}users (email)", expected);
    assert!(validate_sql_table_prefix(&ctx, &valid_sql).is_ok());

    // Invalid: index on system table
    let invalid_sql = "CREATE INDEX idx_evil ON haex_extensions (id)";
    assert!(validate_sql_table_prefix(&ctx, invalid_sql).is_err());
}

#[test]
fn test_table_prefix_validation_drop_table() {
    let ctx = create_test_context();
    let expected = get_expected_prefix();

    // Valid
    let valid_sql = format!("DROP TABLE {}users", expected);
    assert!(validate_sql_table_prefix(&ctx, &valid_sql).is_ok());

    // Invalid: drop system table
    let invalid_sql = "DROP TABLE haex_extensions";
    assert!(validate_sql_table_prefix(&ctx, invalid_sql).is_err());
}

#[test]
fn test_table_prefix_with_quoted_names() {
    let ctx = create_test_context();
    let expected = get_expected_prefix();

    // Double-quoted table names
    let valid_sql = format!(r#"CREATE TABLE "{0}users" (id TEXT PRIMARY KEY)"#, expected);
    assert!(validate_sql_table_prefix(&ctx, &valid_sql).is_ok());

    // Backtick-quoted table names
    let valid_sql = format!("CREATE TABLE `{0}users` (id TEXT PRIMARY KEY)", expected);
    assert!(validate_sql_table_prefix(&ctx, &valid_sql).is_ok());

    // Invalid quoted
    let invalid_sql = r#"CREATE TABLE "haex_extensions" (id TEXT PRIMARY KEY)"#;
    assert!(validate_sql_table_prefix(&ctx, invalid_sql).is_err());
}

#[test]
fn test_system_table_detection_comprehensive() {
    // All system tables that must be protected
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
        "sqlite_master",
        "sqlite_sequence",
        "sqlite_stat1",
        "sqlite_stat2",
        "sqlite_stat3",
        "sqlite_stat4",
        "sqlite_temp_master",
    ];

    for table in system_tables {
        assert!(
            is_system_table(table),
            "Should identify '{}' as system table",
            table
        );
    }
}

#[test]
fn test_system_table_prefix_variations() {
    // Various attempts to access system tables through prefix patterns
    let attempts = [
        ("haex_*", "haex_extensions"),
        ("haex_*", "haex_vault_settings"),
        ("haex_extension*", "haex_extensions"),
        ("sqlite_*", "sqlite_master"),
    ];

    for (pattern, target) in attempts {
        // Even with wildcard permissions, system tables should be blocked
        assert!(is_system_table(target));
        // matches_target should return false for system tables
        assert!(
            !matches_target(pattern, target),
            "Pattern '{}' should not match system table '{}'",
            pattern,
            target
        );
    }
}

#[test]
fn test_cross_extension_table_access() {
    let ctx = create_test_context();

    // Attempting to access another extension's tables
    let other_ext_sql = "CREATE TABLE otherpubkey__otherext__users (id TEXT PRIMARY KEY)";
    assert!(validate_sql_table_prefix(&ctx, other_ext_sql).is_err());
}

#[test]
fn test_permission_checker_cross_extension() {
    use crate::extension::core::manifest::{DisplayMode, ExtensionManifest, ExtensionPermissions};
    use crate::extension::core::types::{Extension, ExtensionSource};
    use std::path::PathBuf;

    // Create test extension with no extra permissions
    let extension = Extension {
        id: "testkey_myext".to_string(),
        manifest: ExtensionManifest {
            name: "myext".to_string(),
            version: "0.1.0".to_string(),
            author: None,
            entry: Some("index.html".to_string()),
            icon: None,
            public_key: "testkey".to_string(),
            signature: "test".to_string(),
            permissions: ExtensionPermissions {
                database: None,
                filesystem: None,
                http: None,
                shell: None,
                sync_servers: None,

                cloud_storage: None,

                sync_rules: None,
                spaces: None,
                identities: None,
                passwords: None,
                mail: None,
                notifications: None,
            },
            homepage: None,
            description: None,
            single_instance: None,
            display_mode: Some(DisplayMode::Iframe),
            migrations_dir: None,
            i18n: None,
        },
        source: ExtensionSource::Production {
            path: PathBuf::from("/tmp/test"),
            version: "0.1.0".to_string(),
        },
        enabled: true,
        last_accessed: std::time::SystemTime::now(),
    };

    let checker = PermissionChecker::new(extension, vec![]);

    // Own tables should be accessible
    use crate::extension::permissions::types::DbAction;
    assert!(checker.can_access_table("testkey__myext__users", DbAction::ReadWrite));

    // Other extension tables should NOT be accessible
    assert!(!checker.can_access_table("otherkey__otherext__users", DbAction::Read));

    // System tables should NOT be accessible
    assert!(!checker.can_access_table("haex_extensions", DbAction::Read));
    assert!(!checker.can_access_table("sqlite_master", DbAction::Read));
}
