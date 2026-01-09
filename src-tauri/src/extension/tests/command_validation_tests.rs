// src-tauri/src/extension/tests/command_validation_tests.rs
//!
//! Command validation tests for extension APIs
//!
//! These tests validate that extension commands properly:
//! - Validate input parameters
//! - Check permissions
//! - Return appropriate errors
//!

use crate::extension::core::manifest::{DisplayMode, ExtensionManifest, ExtensionPermissions};
use crate::extension::core::types::{Extension, ExtensionSource};
use crate::extension::database::helpers::ExtensionSqlContext;
use crate::extension::permissions::checker::{is_system_table, PermissionChecker};
use crate::extension::permissions::manager::PermissionManager;
use crate::extension::permissions::types::{
    Action, DbAction, ExtensionPermission, PermissionStatus, ResourceType,
};
use std::path::PathBuf;

// ============================================================================
// Test Helpers
// ============================================================================

fn create_test_extension(public_key: &str, name: &str) -> Extension {
    Extension {
        id: format!("{}_{}", public_key, name),
        manifest: ExtensionManifest {
            name: name.to_string(),
            version: "1.0.0".to_string(),
            author: Some("Test Author".to_string()),
            entry: Some("index.html".to_string()),
            icon: None,
            public_key: public_key.to_string(),
            signature: "test_signature".to_string(),
            permissions: ExtensionPermissions {
                database: None,
                filesystem: None,
                http: None,
                shell: None,
                filesync: None,
            },
            homepage: None,
            description: Some("Test extension".to_string()),
            single_instance: None,
            display_mode: Some(DisplayMode::Iframe),
            migrations_dir: None,
        },
        source: ExtensionSource::Production {
            path: PathBuf::from("/tmp/test-extension"),
            version: "1.0.0".to_string(),
        },
        enabled: true,
        last_accessed: std::time::SystemTime::now(),
    }
}

fn create_db_permission(
    extension_id: &str,
    action: DbAction,
    target: &str,
    status: PermissionStatus,
) -> ExtensionPermission {
    ExtensionPermission {
        id: uuid::Uuid::new_v4().to_string(),
        extension_id: extension_id.to_string(),
        resource_type: ResourceType::Db,
        action: Action::Database(action),
        target: target.to_string(),
        constraints: None,
        status,
        haex_timestamp: Some("0".to_string()),
    }
}

// ============================================================================
// Database Command Validation Tests
// ============================================================================

mod database_commands {
    use super::*;

    #[test]
    fn test_extension_sql_context_table_prefix() {
        let ctx =
            ExtensionSqlContext::new("b4401f13f65e576b".to_string(), "haex-pass".to_string());

        let prefix = ctx.get_table_prefix();
        // Name is NOT sanitized - hyphens are preserved
        assert_eq!(prefix, "b4401f13f65e576b__haex-pass__");
    }

    #[test]
    fn test_extension_sql_context_preserves_special_chars() {
        // Extension name with special chars is preserved (not sanitized)
        let ctx =
            ExtensionSqlContext::new("pubkey".to_string(), "my-extension-name".to_string());

        let prefix = ctx.get_table_prefix();
        assert_eq!(prefix, "pubkey__my-extension-name__");
    }

    #[test]
    fn test_database_permission_checker_own_tables() {
        let extension = create_test_extension("pubkey123", "myext");
        let checker = PermissionChecker::new(extension, vec![]);

        // Extension should ALWAYS be able to access its own tables
        assert!(checker.can_access_table("pubkey123__myext__users", DbAction::Read));
        assert!(checker.can_access_table("pubkey123__myext__users", DbAction::ReadWrite));
        assert!(checker.can_access_table("pubkey123__myext__settings", DbAction::ReadWrite));
    }

    #[test]
    fn test_database_permission_checker_other_tables() {
        let extension = create_test_extension("pubkey123", "myext");
        let checker = PermissionChecker::new(extension, vec![]);

        // Without permissions, cannot access other extension tables
        assert!(!checker.can_access_table("otherpubkey__otherext__users", DbAction::Read));
        assert!(!checker.can_access_table("otherpubkey__otherext__data", DbAction::ReadWrite));
    }

    #[test]
    fn test_database_permission_checker_with_granted_permission() {
        let extension = create_test_extension("pubkey123", "myext");
        let permissions = vec![create_db_permission(
            "pubkey123_myext",
            DbAction::Read,
            "otherpubkey__otherext__*",
            PermissionStatus::Granted,
        )];

        let checker = PermissionChecker::new(extension, permissions);

        // With granted permission, can access other extension tables
        assert!(checker.can_access_table("otherpubkey__otherext__users", DbAction::Read));
        assert!(checker.can_access_table("otherpubkey__otherext__posts", DbAction::Read));

        // But not write access (only read permission granted)
        assert!(!checker.can_access_table("otherpubkey__otherext__users", DbAction::ReadWrite));
    }

    #[test]
    fn test_database_permission_checker_denied_permission() {
        let extension = create_test_extension("pubkey123", "myext");
        let permissions = vec![create_db_permission(
            "pubkey123_myext",
            DbAction::Read,
            "otherpubkey__otherext__*",
            PermissionStatus::Denied,
        )];

        let checker = PermissionChecker::new(extension, permissions);

        // Denied permission should block access
        assert!(!checker.can_access_table("otherpubkey__otherext__users", DbAction::Read));
    }

    #[test]
    fn test_database_permission_checker_system_tables() {
        let extension = create_test_extension("pubkey123", "myext");
        let checker = PermissionChecker::new(extension, vec![]);

        // System tables should never be accessible
        assert!(!checker.can_access_table("sqlite_master", DbAction::Read));
        assert!(!checker.can_access_table("haex_vault_settings", DbAction::Read));
        assert!(!checker.can_access_table("haex_extensions", DbAction::Read));
        assert!(!checker.can_access_table("haex_crdt_configs", DbAction::Read));
    }

    #[test]
    fn test_is_system_table() {
        // SQLite system tables
        assert!(is_system_table("sqlite_master"));
        assert!(is_system_table("sqlite_sequence"));
        assert!(is_system_table("sqlite_stat1"));

        // Haex system tables
        assert!(is_system_table("haex_vault_settings"));
        assert!(is_system_table("haex_extensions"));
        assert!(is_system_table("haex_crdt_configs"));

        // Not system tables
        assert!(!is_system_table("pubkey__ext__users"));
        assert!(!is_system_table("my_table"));
    }
}

// ============================================================================
// Filesystem Permission Tests (using PermissionManager static methods)
// ============================================================================

mod filesystem_permissions {
    use super::*;

    #[test]
    fn test_filesystem_path_pattern_matching() {
        // Basic wildcard matching
        assert!(PermissionManager::matches_path_pattern(
            "/home/user/*",
            "/home/user/file.txt"
        ));
        assert!(PermissionManager::matches_path_pattern(
            "/home/user/*",
            "/home/user/subdir/file.txt"
        ));
        assert!(!PermissionManager::matches_path_pattern(
            "/home/user/*",
            "/home/other/file.txt"
        ));
    }

    #[test]
    fn test_filesystem_exact_path_matching() {
        assert!(PermissionManager::matches_path_pattern(
            "/home/user/specific.txt",
            "/home/user/specific.txt"
        ));
        assert!(!PermissionManager::matches_path_pattern(
            "/home/user/specific.txt",
            "/home/user/other.txt"
        ));
    }

    #[test]
    fn test_filesystem_path_traversal_blocked() {
        // Path traversal attempts should be blocked
        assert!(
            !PermissionManager::matches_path_pattern("/home/user/*", "/home/user/../etc/passwd"),
            "Basic path traversal should be blocked"
        );
        assert!(
            !PermissionManager::matches_path_pattern(
                "/home/user/*",
                "/home/user/./../../etc/passwd"
            ),
            "Multiple traversal should be blocked"
        );
    }
}

// ============================================================================
// Web Permission Tests
// ============================================================================

mod web_permissions {
    use super::*;

    #[test]
    fn test_web_url_pattern_matching() {
        assert!(PermissionManager::matches_url_pattern(
            "https://api.example.com/*",
            "https://api.example.com/users"
        ));
        assert!(PermissionManager::matches_url_pattern(
            "https://api.example.com/*",
            "https://api.example.com/users/123"
        ));
        assert!(!PermissionManager::matches_url_pattern(
            "https://api.example.com/*",
            "https://other.example.com/users"
        ));
    }

    #[test]
    fn test_web_subdomain_wildcard() {
        assert!(PermissionManager::matches_url_pattern(
            "https://*.example.com/*",
            "https://api.example.com/path"
        ));
        assert!(PermissionManager::matches_url_pattern(
            "https://*.example.com/*",
            "https://www.example.com/path"
        ));
    }

    #[test]
    fn test_web_protocol_enforcement() {
        // HTTPS pattern should not match HTTP
        assert!(!PermissionManager::matches_url_pattern(
            "https://api.example.com/*",
            "http://api.example.com/users"
        ));
    }

    #[test]
    fn test_web_url_validation() {
        // Valid HTTP(S) URLs
        let valid_urls = [
            "https://example.com",
            "https://example.com/path",
            "http://localhost:3000/api",
        ];

        for url_str in valid_urls {
            let parsed = url::Url::parse(url_str);
            assert!(parsed.is_ok(), "Should parse valid URL: {}", url_str);

            let url = parsed.unwrap();
            let scheme = url.scheme();
            assert!(
                scheme == "http" || scheme == "https",
                "Should be HTTP(S): {}",
                url_str
            );
        }
    }
}

// ============================================================================
// Extension Identification Tests
// ============================================================================

mod extension_identification {
    use super::*;

    #[test]
    fn test_extension_id_format() {
        let extension = create_test_extension("b4401f13f65e576b", "haex-pass");

        // ID should be public_key + "_" + name
        assert_eq!(extension.id, "b4401f13f65e576b_haex-pass");
    }

    #[test]
    fn test_extension_table_prefix_format() {
        let ctx = ExtensionSqlContext::new(
            "b4401f13f65e576b".to_string(),
            "haex-pass".to_string(),
        );

        let prefix = ctx.get_table_prefix();

        // Prefix format: publicKey__name__ (name is NOT sanitized)
        assert!(prefix.starts_with("b4401f13f65e576b__"));
        assert!(prefix.ends_with("__"));
        assert!(prefix.contains("haex-pass")); // dash is preserved
        assert_eq!(prefix, "b4401f13f65e576b__haex-pass__");
    }

    #[test]
    fn test_extension_uniqueness() {
        // Same public_key + different name = different extension
        let ext1 = create_test_extension("pubkey", "ext-a");
        let ext2 = create_test_extension("pubkey", "ext-b");

        assert_ne!(ext1.id, ext2.id);

        // Different public_key + same name = different extension
        let ext3 = create_test_extension("pubkey1", "myext");
        let ext4 = create_test_extension("pubkey2", "myext");

        assert_ne!(ext3.id, ext4.id);

        // Same public_key + same name = same extension
        let ext5 = create_test_extension("pubkey", "myext");
        let ext6 = create_test_extension("pubkey", "myext");

        assert_eq!(ext5.id, ext6.id);
    }
}
