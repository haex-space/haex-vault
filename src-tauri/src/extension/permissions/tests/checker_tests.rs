// src-tauri/src/extension/permissions/tests/checker_tests.rs

use crate::extension::core::manifest::{DisplayMode, ExtensionManifest, ExtensionPermissions};
use crate::extension::core::types::{Extension, ExtensionSource};
use crate::extension::permissions::checker::{is_system_table, matches_target, PermissionChecker};
use crate::extension::permissions::types::{
    Action, DbAction, ExtensionPermission, PermissionStatus, ResourceType,
};
use std::path::PathBuf;

fn create_test_extension(public_key: &str, name: &str) -> Extension {
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

fn create_permission(action: DbAction, target: &str) -> ExtensionPermission {
    ExtensionPermission {
        id: uuid::Uuid::new_v4().to_string(),
        extension_id: "test_ext".to_string(),
        resource_type: ResourceType::Db,
        action: Action::Database(action),
        target: target.to_string(),
        constraints: None,
        status: PermissionStatus::Granted,
        haex_timestamp: Some("0".to_string()),
    }
}

#[test]
fn test_can_access_own_table() {
    let extension = create_test_extension("test_key", "my_ext");
    let checker = PermissionChecker::new(extension, vec![]);

    assert!(checker.can_access_table("test_key__my_ext__users", DbAction::Read));
    assert!(checker.can_access_table("test_key__my_ext__posts", DbAction::ReadWrite));
}

#[test]
fn test_cannot_access_system_tables() {
    let extension = create_test_extension("test_key", "my_ext");
    let permissions = vec![create_permission(DbAction::Read, "*")];
    let checker = PermissionChecker::new(extension, permissions);

    assert!(!checker.can_access_table("haex_extensions", DbAction::Read));
    assert!(!checker.can_access_table("haex_vault_settings", DbAction::Read));
    assert!(!checker.can_access_table("sqlite_master", DbAction::Read));
}

#[test]
fn test_cannot_access_other_extension_tables_without_permission() {
    let extension = create_test_extension("test_key", "my_ext");
    let checker = PermissionChecker::new(extension, vec![]);

    assert!(!checker.can_access_table("other_key__other_ext__private", DbAction::Read));
}

#[test]
fn test_can_access_other_extension_tables_with_prefix_wildcard() {
    let extension = create_test_extension("test_key", "my_ext");
    // Grant access to all tables of another extension using prefix wildcard
    let permissions = vec![create_permission(DbAction::Read, "other_key__other_ext__*")];
    let checker = PermissionChecker::new(extension, permissions);

    assert!(checker.can_access_table("other_key__other_ext__users", DbAction::Read));
    assert!(checker.can_access_table("other_key__other_ext__posts", DbAction::Read));
    assert!(checker.can_access_table("other_key__other_ext__anything", DbAction::Read));

    // But not tables from a different extension
    assert!(!checker.can_access_table("different__ext__users", DbAction::Read));
}

#[test]
fn test_wildcard_permission_grants_non_system_access() {
    let extension = create_test_extension("test_key", "my_ext");
    let permissions = vec![create_permission(DbAction::Read, "*")];
    let checker = PermissionChecker::new(extension, permissions);

    // Should allow access to non-system, non-extension tables
    assert!(checker.can_access_table("custom_user_table", DbAction::Read));

    // But NOT system tables
    assert!(!checker.can_access_table("haex_extensions", DbAction::Read));
}

#[test]
fn test_prefix_wildcard_does_not_match_system_tables() {
    let extension = create_test_extension("test_key", "my_ext");
    // Try to get access to system tables via prefix wildcard
    let permissions = vec![create_permission(DbAction::Read, "haex_*")];
    let checker = PermissionChecker::new(extension, permissions);

    // Should NOT work - system tables are always blocked
    assert!(!checker.can_access_table("haex_extensions", DbAction::Read));
    assert!(!checker.can_access_table("haex_vault_settings", DbAction::Read));
}

#[test]
fn test_read_write_includes_read() {
    let extension = create_test_extension("test_key", "my_ext");
    let permissions = vec![create_permission(DbAction::ReadWrite, "custom_table")];
    let checker = PermissionChecker::new(extension, permissions);

    // ReadWrite permission should allow Read
    assert!(checker.can_access_table("custom_table", DbAction::Read));
    assert!(checker.can_access_table("custom_table", DbAction::ReadWrite));
}

#[test]
fn test_read_permission_does_not_include_write() {
    let extension = create_test_extension("test_key", "my_ext");
    let permissions = vec![create_permission(DbAction::Read, "custom_table")];
    let checker = PermissionChecker::new(extension, permissions);

    assert!(checker.can_access_table("custom_table", DbAction::Read));
    assert!(!checker.can_access_table("custom_table", DbAction::ReadWrite));
}

#[test]
fn test_exact_table_permission() {
    let extension = create_test_extension("test_key", "my_ext");
    let permissions = vec![create_permission(DbAction::Read, "specific_table")];
    let checker = PermissionChecker::new(extension, permissions);

    assert!(checker.can_access_table("specific_table", DbAction::Read));
    assert!(!checker.can_access_table("different_table", DbAction::Read));
}

#[test]
fn test_is_system_table() {
    assert!(is_system_table("haex_extensions"));
    assert!(is_system_table("haex_vault_settings"));
    assert!(is_system_table("sqlite_master"));
    assert!(is_system_table("sqlite_sequence"));

    assert!(!is_system_table("test_key__my_ext__users"));
    assert!(!is_system_table("custom_table"));
}

#[test]
fn test_matches_target_prefix_wildcard() {
    assert!(matches_target("prefix__*", "prefix__table1"));
    assert!(matches_target("prefix__*", "prefix__table2"));
    assert!(!matches_target("prefix__*", "other__table"));

    // Nested prefixes
    assert!(matches_target("key__ext__*", "key__ext__users"));
    assert!(matches_target("key__ext__*", "key__ext__posts"));
    assert!(!matches_target("key__ext__*", "key__different__users"));
}

#[test]
fn test_matches_target_full_wildcard() {
    assert!(matches_target("*", "any_table"));
    assert!(matches_target("*", "another_table"));

    // But not system tables
    assert!(!matches_target("*", "haex_extensions"));
    assert!(!matches_target("*", "sqlite_master"));
}

#[test]
fn test_matches_target_exact() {
    assert!(matches_target("exact_table", "exact_table"));
    assert!(!matches_target("exact_table", "different_table"));
}
