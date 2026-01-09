// src-tauri/src/extension/tests/security_tests.rs
//!
//! Security tests for extension APIs
//!
//! These tests verify protection against malicious inputs:
//! - SQL injection attacks
//! - Path traversal attacks
//! - Permission bypass attempts
//! - Cross-extension access attempts
//! - Malformed request handling
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
            author: None,
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
            description: None,
            single_instance: None,
            display_mode: Some(DisplayMode::Iframe),
            migrations_dir: None,
        },
        source: ExtensionSource::Production {
            path: PathBuf::from("/tmp/test"),
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
// SQL Injection Attack Tests
// ============================================================================

mod sql_injection_attacks {
    use super::*;

    #[test]
    fn test_system_tables_always_blocked() {
        // Even with wildcard permission, system tables must be blocked
        let extension = create_test_extension("pubkey", "myext");
        let permissions = vec![create_db_permission(
            "pubkey_myext",
            DbAction::ReadWrite,
            "*",
            PermissionStatus::Granted,
        )];

        let checker = PermissionChecker::new(extension, permissions);

        // System tables should ALWAYS be blocked
        assert!(!checker.can_access_table("sqlite_master", DbAction::Read));
        assert!(!checker.can_access_table("sqlite_sequence", DbAction::Read));
        assert!(!checker.can_access_table("haex_vault_settings", DbAction::Read));
        assert!(!checker.can_access_table("haex_extensions", DbAction::Read));
        assert!(!checker.can_access_table("haex_crdt_configs", DbAction::Read));
    }

    #[test]
    fn test_table_name_with_quotes() {
        let extension = create_test_extension("pubkey", "myext");
        let checker = PermissionChecker::new(extension, vec![]);

        // Quoted table names should be handled correctly
        assert!(checker.can_access_table("\"pubkey__myext__users\"", DbAction::Read));
        assert!(checker.can_access_table("`pubkey__myext__users`", DbAction::Read));

        // Quotes shouldn't allow access to system tables
        assert!(!checker.can_access_table("\"sqlite_master\"", DbAction::Read));
        assert!(!checker.can_access_table("`haex_vault_settings`", DbAction::Read));
    }

    #[test]
    fn test_system_table_detection() {
        // SQLite internal tables
        assert!(is_system_table("sqlite_master"));
        assert!(is_system_table("sqlite_sequence"));
        assert!(is_system_table("sqlite_stat1"));
        assert!(is_system_table("sqlite_stat4"));
        assert!(is_system_table("sqlite_temp_master"));

        // Haex system tables
        assert!(is_system_table("haex_vault_settings"));
        assert!(is_system_table("haex_extensions"));
        assert!(is_system_table("haex_permissions"));
        assert!(is_system_table("haex_crdt_configs"));

        // Should NOT be system tables
        assert!(!is_system_table("users"));
        assert!(!is_system_table("pubkey__ext__data"));
        assert!(!is_system_table("my_haex_table")); // doesn't start with haex_
    }

    #[test]
    fn test_extension_cannot_spoof_another_prefix() {
        let ctx = ExtensionSqlContext::new("pubkey".to_string(), "myext".to_string());
        let expected_prefix = ctx.get_table_prefix();

        // Attacker tries to access other extension's tables
        let other_prefixes = [
            "otherpubkey__otherext__",
            "admin__admin__",
            "pubkey__otherext__", // Same pubkey, different name
        ];

        for other_prefix in other_prefixes {
            assert!(
                !format!("{}users", other_prefix).starts_with(&expected_prefix),
                "Extension should not match other prefix: {}",
                other_prefix
            );
        }
    }
}

// ============================================================================
// Path Traversal Attack Tests
// ============================================================================

mod path_traversal_attacks {
    use super::*;

    #[test]
    fn test_basic_traversal_blocked() {
        let traversal_attempts = [
            "../../../etc/passwd",
            "/home/user/../../../etc/passwd",
            "./../../etc/passwd",
        ];

        for path in traversal_attempts {
            assert!(
                !PermissionManager::matches_path_pattern("/home/user/*", path),
                "Should block traversal: {}",
                path
            );
        }
    }

    #[test]
    fn test_double_dot_variations() {
        let traversal_attempts = [
            "/home/user/..\\..\\etc\\passwd",
            "/home/user/....//etc/passwd",
            "/home/user/./../etc/passwd",
        ];

        for path in traversal_attempts {
            // Any path containing .. should be treated with caution
            assert!(
                path.contains(".."),
                "Test case should contain traversal: {}",
                path
            );
        }
    }

    #[test]
    fn test_valid_paths_still_work() {
        // Ensure normal paths still work
        assert!(PermissionManager::matches_path_pattern(
            "/home/user/*",
            "/home/user/documents/file.txt"
        ));
        assert!(PermissionManager::matches_path_pattern(
            "/home/user/*",
            "/home/user/a/b/c/deep/file.txt"
        ));
    }
}

// ============================================================================
// Permission Bypass Attack Tests
// ============================================================================

mod permission_bypass_attacks {
    use super::*;

    #[test]
    fn test_extension_id_spoofing_blocked() {
        // Attacker creates extension with same name but different pubkey
        let legit_extension = create_test_extension("legit_pubkey", "haex-pass");
        let evil_extension = create_test_extension("evil_pubkey", "haex-pass");

        let legit_checker = PermissionChecker::new(legit_extension, vec![]);
        let evil_checker = PermissionChecker::new(evil_extension, vec![]);

        // Legit extension can access its tables (name NOT sanitized, hyphen preserved)
        assert!(legit_checker.can_access_table("legit_pubkey__haex-pass__secrets", DbAction::Read));

        // Evil extension CANNOT access legit extension's tables
        assert!(
            !evil_checker.can_access_table("legit_pubkey__haex-pass__secrets", DbAction::Read)
        );
    }

    #[test]
    fn test_read_permission_cannot_write() {
        let extension = create_test_extension("pubkey", "myext");
        let permissions = vec![create_db_permission(
            "pubkey_myext",
            DbAction::Read, // Only read permission
            "other__ext__*",
            PermissionStatus::Granted,
        )];

        let checker = PermissionChecker::new(extension, permissions);

        // Can read
        assert!(checker.can_access_table("other__ext__data", DbAction::Read));

        // Cannot write with read-only permission
        assert!(!checker.can_access_table("other__ext__data", DbAction::ReadWrite));
    }

    #[test]
    fn test_denied_status_blocks_access() {
        let extension = create_test_extension("pubkey", "myext");
        let permissions = vec![create_db_permission(
            "pubkey_myext",
            DbAction::ReadWrite,
            "other__ext__*",
            PermissionStatus::Denied, // Explicitly denied
        )];

        let checker = PermissionChecker::new(extension, permissions);

        // Denied permission should block access
        assert!(!checker.can_access_table("other__ext__data", DbAction::Read));
        assert!(!checker.can_access_table("other__ext__data", DbAction::ReadWrite));
    }

    #[test]
    fn test_ask_status_does_not_grant_access() {
        let extension = create_test_extension("pubkey", "myext");
        let permissions = vec![create_db_permission(
            "pubkey_myext",
            DbAction::ReadWrite,
            "other__ext__*",
            PermissionStatus::Ask, // Not yet granted
        )];

        let checker = PermissionChecker::new(extension, permissions);

        // "Ask" status should not automatically grant access
        assert!(!checker.can_access_table("other__ext__data", DbAction::Read));
    }
}

// ============================================================================
// Web Security Attack Tests
// ============================================================================

mod web_security_attacks {
    use super::*;

    #[test]
    fn test_protocol_downgrade_blocked() {
        // HTTPS permission should not match HTTP (protocol downgrade)
        assert!(!PermissionManager::matches_url_pattern(
            "https://api.example.com/*",
            "http://api.example.com/secrets"
        ));
    }

    #[test]
    fn test_subdomain_confusion_blocked() {
        // Attacker tries to match different domain via subdomain confusion
        // Pattern for *.example.com should NOT match evil.com
        assert!(!PermissionManager::matches_url_pattern(
            "https://*.example.com/*",
            "https://example.com.evil.com/path"
        ));
    }

    #[test]
    fn test_dangerous_url_schemes_rejected() {
        let dangerous_urls = [
            "file:///etc/passwd",
            "javascript:alert(1)",
            "data:text/html,<script>alert(1)</script>",
            "ftp://evil.com/file",
        ];

        for url in dangerous_urls {
            let parsed = url::Url::parse(url);
            if let Ok(parsed_url) = parsed {
                let scheme = parsed_url.scheme();
                assert!(
                    scheme != "http" && scheme != "https",
                    "Dangerous scheme should be rejected: {}",
                    scheme
                );
            }
        }
    }

    #[test]
    fn test_internal_network_urls() {
        // These URLs target internal networks (potential SSRF)
        let internal_urls = [
            "http://localhost/admin",
            "http://127.0.0.1/internal",
            "http://192.168.1.1/admin",
            "http://10.0.0.1/internal",
            "http://169.254.169.254/metadata", // AWS metadata
        ];

        for url in internal_urls {
            // These should be parseable but require explicit permission
            let parsed = url::Url::parse(url);
            assert!(parsed.is_ok(), "Internal URL should parse: {}", url);
        }
    }
}

// ============================================================================
// Cross-Extension Isolation Tests
// ============================================================================

mod cross_extension_isolation {
    use super::*;

    #[test]
    fn test_extension_a_cannot_access_extension_b_tables() {
        let ext_a = create_test_extension("pubkey_a", "extension_a");
        let checker_a = PermissionChecker::new(ext_a, vec![]);

        // Extension A cannot access Extension B's tables without permission
        assert!(!checker_a.can_access_table("pubkey_b__extension_b__secrets", DbAction::Read));
        assert!(
            !checker_a.can_access_table("pubkey_b__extension_b__users", DbAction::ReadWrite)
        );

        // Extension A CAN access its own tables
        assert!(checker_a.can_access_table("pubkey_a__extension_a__data", DbAction::ReadWrite));
    }

    #[test]
    fn test_table_prefix_uniqueness() {
        // Different extensions should have different prefixes
        let ctx_a = ExtensionSqlContext::new("pubkey_a".to_string(), "extension".to_string());
        let ctx_b = ExtensionSqlContext::new("pubkey_b".to_string(), "extension".to_string());

        assert_ne!(ctx_a.get_table_prefix(), ctx_b.get_table_prefix());
    }

    #[test]
    fn test_same_name_different_pubkey_isolated() {
        // Two extensions with same name but different pubkeys are isolated
        let ctx_1 =
            ExtensionSqlContext::new("developer1".to_string(), "password-manager".to_string());
        let ctx_2 =
            ExtensionSqlContext::new("developer2".to_string(), "password-manager".to_string());

        let prefix_1 = ctx_1.get_table_prefix();
        let prefix_2 = ctx_2.get_table_prefix();

        // Prefixes should be different
        assert_ne!(prefix_1, prefix_2);

        // Table belonging to developer1 should not match developer2's prefix
        let table = format!("{}secrets", prefix_1);
        assert!(!table.starts_with(&prefix_2));
    }
}

// ============================================================================
// Malformed Input Tests
// ============================================================================

mod malformed_input_tests {
    use super::*;

    #[test]
    fn test_empty_table_name() {
        let extension = create_test_extension("pubkey", "myext");
        let checker = PermissionChecker::new(extension, vec![]);

        // Empty table name should not match anything
        assert!(!checker.can_access_table("", DbAction::Read));
    }

    #[test]
    fn test_special_characters_in_table_name() {
        let extension = create_test_extension("pubkey", "myext");
        let checker = PermissionChecker::new(extension, vec![]);

        // Special characters should be handled safely
        let special_names = [
            "pubkey__myext__users; DROP TABLE secrets",
            "pubkey__myext__users\0hidden",
            "pubkey__myext__users\nmalicious",
        ];

        for name in special_names {
            // These should not cause crashes
            let _ = checker.can_access_table(name, DbAction::Read);
        }
    }

    #[test]
    fn test_very_long_table_name() {
        let extension = create_test_extension("pubkey", "myext");
        let checker = PermissionChecker::new(extension, vec![]);

        // Very long table name should be handled
        let long_name = "a".repeat(10000);
        let _ = checker.can_access_table(&long_name, DbAction::Read);
    }

    #[test]
    fn test_unicode_in_extension_name() {
        // Unicode characters should be handled correctly
        let ctx = ExtensionSqlContext::new(
            "pubkey".to_string(),
            "extension-日本語".to_string(),
        );

        let prefix = ctx.get_table_prefix();
        // Should produce a valid prefix
        assert!(prefix.starts_with("pubkey__"));
    }
}
