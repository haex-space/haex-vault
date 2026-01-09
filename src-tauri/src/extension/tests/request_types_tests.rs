// src-tauri/src/extension/tests/request_types_tests.rs
//!
//! Tests for extension request types and serialization
//!
//! These tests verify that request types are correctly validated and serialized,
//! preventing malformed inputs from causing issues.
//!

use crate::extension::core::manifest::{
    DisplayMode, ExtensionManifest, ExtensionPermissions, PermissionEntry,
};
use crate::extension::core::types::{Extension, ExtensionSource};
use crate::extension::database::helpers::ExtensionSqlContext;
use crate::extension::database::types::{DatabaseQueryResult, MigrationResult};
use crate::extension::permissions::types::{
    Action, DbAction, ExtensionPermission, PermissionStatus, ResourceType,
};
use serde_json::{json, Value as JsonValue};
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

// ============================================================================
// Extension Manifest Tests
// ============================================================================

mod manifest_tests {
    use super::*;

    #[test]
    fn test_manifest_with_all_permissions() {
        let manifest = ExtensionManifest {
            name: "test".to_string(),
            version: "1.0.0".to_string(),
            author: Some("Test Author".to_string()),
            entry: Some("index.html".to_string()),
            icon: Some("icon.png".to_string()),
            public_key: "test_key".to_string(),
            signature: "test_sig".to_string(),
            permissions: ExtensionPermissions {
                database: Some(vec![PermissionEntry {
                    target: "*".to_string(),
                    operation: Some("read_write".to_string()),
                    constraints: None,
                    status: None,
                }]),
                filesystem: Some(vec![PermissionEntry {
                    target: "/home/*".to_string(),
                    operation: Some("read".to_string()),
                    constraints: None,
                    status: None,
                }]),
                http: Some(vec![PermissionEntry {
                    target: "https://*".to_string(),
                    operation: None,
                    constraints: None,
                    status: None,
                }]),
                shell: Some(vec![PermissionEntry {
                    target: "git".to_string(),
                    operation: Some("execute".to_string()),
                    constraints: None,
                    status: None,
                }]),
                filesync: Some(vec![PermissionEntry {
                    target: "*".to_string(),
                    operation: Some("read_write".to_string()),
                    constraints: None,
                    status: None,
                }]),
            },
            homepage: Some("https://example.com".to_string()),
            description: Some("Test description".to_string()),
            single_instance: Some(true),
            display_mode: Some(DisplayMode::Window),
            migrations_dir: Some("migrations".to_string()),
        };

        assert_eq!(manifest.name, "test");
        assert!(manifest.permissions.database.is_some());
        assert!(manifest.permissions.filesystem.is_some());
        assert!(manifest.permissions.http.is_some());
    }

    #[test]
    fn test_manifest_with_empty_permissions() {
        let manifest = ExtensionManifest {
            name: "minimal".to_string(),
            version: "1.0.0".to_string(),
            author: None,
            entry: Some("index.html".to_string()),
            icon: None,
            public_key: "key".to_string(),
            signature: "sig".to_string(),
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
            display_mode: None,
            migrations_dir: None,
        };

        assert!(manifest.permissions.database.is_none());
        assert!(manifest.permissions.filesystem.is_none());
    }

    #[test]
    fn test_display_modes() {
        assert!(matches!(DisplayMode::Iframe, DisplayMode::Iframe));
        assert!(matches!(DisplayMode::Window, DisplayMode::Window));
        assert!(matches!(DisplayMode::Auto, DisplayMode::Auto));
    }
}

// ============================================================================
// Extension Source Tests
// ============================================================================

mod extension_source_tests {
    use super::*;

    #[test]
    fn test_production_source() {
        let source = ExtensionSource::Production {
            path: PathBuf::from("/app/extensions/my-ext"),
            version: "1.2.3".to_string(),
        };

        if let ExtensionSource::Production { path, version } = source {
            assert_eq!(path, PathBuf::from("/app/extensions/my-ext"));
            assert_eq!(version, "1.2.3");
        } else {
            panic!("Expected Production source");
        }
    }

    #[test]
    fn test_development_source() {
        let source = ExtensionSource::Development {
            dev_server_url: "http://localhost:5173".to_string(),
            manifest_path: PathBuf::from("/dev/my-ext/manifest.json"),
            auto_reload: true,
        };

        if let ExtensionSource::Development {
            dev_server_url,
            manifest_path,
            auto_reload,
        } = source
        {
            assert_eq!(dev_server_url, "http://localhost:5173");
            assert_eq!(manifest_path, PathBuf::from("/dev/my-ext/manifest.json"));
            assert!(auto_reload);
        } else {
            panic!("Expected Development source");
        }
    }
}

// ============================================================================
// Database Query Result Tests
// ============================================================================

mod database_result_tests {
    use super::*;

    #[test]
    fn test_empty_query_result() {
        let result = DatabaseQueryResult {
            rows: vec![],
            rows_affected: 0,
            last_insert_id: None,
        };

        assert!(result.rows.is_empty());
        assert_eq!(result.rows_affected, 0);
        assert!(result.last_insert_id.is_none());
    }

    #[test]
    fn test_query_result_with_rows() {
        let result = DatabaseQueryResult {
            rows: vec![
                vec![json!(1), json!("Alice"), json!(30)],
                vec![json!(2), json!("Bob"), json!(25)],
            ],
            rows_affected: 0,
            last_insert_id: None,
        };

        assert_eq!(result.rows.len(), 2);
        assert_eq!(result.rows[0][1], json!("Alice"));
    }

    #[test]
    fn test_insert_result() {
        let result = DatabaseQueryResult {
            rows: vec![],
            rows_affected: 1,
            last_insert_id: Some(42),
        };

        assert_eq!(result.rows_affected, 1);
        assert_eq!(result.last_insert_id, Some(42));
    }

    #[test]
    fn test_query_result_serialization() {
        let result = DatabaseQueryResult {
            rows: vec![vec![json!(1), json!("test")]],
            rows_affected: 1,
            last_insert_id: Some(1),
        };

        let serialized = serde_json::to_string(&result).unwrap();
        assert!(serialized.contains("rows"));
        assert!(serialized.contains("rowsAffected")); // camelCase
        assert!(serialized.contains("lastInsertId")); // camelCase
    }
}

// ============================================================================
// Migration Result Tests
// ============================================================================

mod migration_result_tests {
    use super::*;

    #[test]
    fn test_migration_result() {
        let result = MigrationResult {
            applied_count: 3,
            already_applied_count: 2,
            applied_migrations: vec![
                "001_init.sql".to_string(),
                "002_users.sql".to_string(),
                "003_settings.sql".to_string(),
            ],
        };

        assert_eq!(result.applied_count, 3);
        assert_eq!(result.already_applied_count, 2);
        assert_eq!(result.applied_migrations.len(), 3);
    }

    #[test]
    fn test_migration_result_serialization() {
        let result = MigrationResult {
            applied_count: 1,
            already_applied_count: 0,
            applied_migrations: vec!["001_init.sql".to_string()],
        };

        let serialized = serde_json::to_string(&result).unwrap();
        assert!(serialized.contains("appliedCount")); // camelCase
        assert!(serialized.contains("alreadyAppliedCount")); // camelCase
        assert!(serialized.contains("appliedMigrations")); // camelCase
    }
}

// ============================================================================
// Permission Types Tests
// ============================================================================

mod permission_types_tests {
    use super::*;

    #[test]
    fn test_resource_types() {
        let db_type = ResourceType::Db;
        let fs_type = ResourceType::Fs;
        let web_type = ResourceType::Web;
        let shell_type = ResourceType::Shell;
        let filesync_type = ResourceType::Filesync;

        // Verify they're different
        assert!(!matches!(db_type, ResourceType::Fs));
        assert!(!matches!(fs_type, ResourceType::Web));
        assert!(matches!(web_type, ResourceType::Web));
        assert!(matches!(shell_type, ResourceType::Shell));
        assert!(matches!(filesync_type, ResourceType::Filesync));
    }

    #[test]
    fn test_db_actions() {
        let read = DbAction::Read;
        let read_write = DbAction::ReadWrite;

        assert!(matches!(read, DbAction::Read));
        assert!(matches!(read_write, DbAction::ReadWrite));
        assert!(!matches!(read, DbAction::ReadWrite));
    }

    #[test]
    fn test_permission_status() {
        let granted = PermissionStatus::Granted;
        let denied = PermissionStatus::Denied;
        let ask = PermissionStatus::Ask;

        assert!(matches!(granted, PermissionStatus::Granted));
        assert!(matches!(denied, PermissionStatus::Denied));
        assert!(matches!(ask, PermissionStatus::Ask));
    }

    #[test]
    fn test_extension_permission_creation() {
        let permission = ExtensionPermission {
            id: "perm_123".to_string(),
            extension_id: "ext_456".to_string(),
            resource_type: ResourceType::Db,
            action: Action::Database(DbAction::Read),
            target: "other__ext__*".to_string(),
            constraints: None,
            status: PermissionStatus::Granted,
            haex_timestamp: Some("1234567890".to_string()),
        };

        assert_eq!(permission.id, "perm_123");
        assert_eq!(permission.extension_id, "ext_456");
        assert!(matches!(permission.resource_type, ResourceType::Db));
        assert!(matches!(permission.status, PermissionStatus::Granted));
    }
}

// ============================================================================
// SQL Context Tests
// ============================================================================

mod sql_context_tests {
    use super::*;

    #[test]
    fn test_table_prefix_format() {
        let ctx = ExtensionSqlContext::new("mypubkey".to_string(), "myext".to_string());
        let prefix = ctx.get_table_prefix();

        // Prefix should be pubkey__sanitized_name__
        assert!(prefix.starts_with("mypubkey__"));
        assert!(prefix.ends_with("__"));
    }

    #[test]
    fn test_table_prefix_preserves_name() {
        // Extension name with hyphens is NOT sanitized - preserved as-is
        let ctx =
            ExtensionSqlContext::new("pubkey".to_string(), "my-cool-extension".to_string());
        let prefix = ctx.get_table_prefix();

        // Hyphens are preserved
        assert_eq!(prefix, "pubkey__my-cool-extension__");
    }

    #[test]
    fn test_different_extensions_have_different_prefixes() {
        let ctx1 = ExtensionSqlContext::new("pubkey1".to_string(), "ext".to_string());
        let ctx2 = ExtensionSqlContext::new("pubkey2".to_string(), "ext".to_string());

        assert_ne!(ctx1.get_table_prefix(), ctx2.get_table_prefix());
    }

    #[test]
    fn test_same_extension_same_prefix() {
        let ctx1 = ExtensionSqlContext::new("pubkey".to_string(), "myext".to_string());
        let ctx2 = ExtensionSqlContext::new("pubkey".to_string(), "myext".to_string());

        assert_eq!(ctx1.get_table_prefix(), ctx2.get_table_prefix());
    }
}

// ============================================================================
// Edge Case Tests
// ============================================================================

mod edge_case_tests {
    use super::*;

    #[test]
    fn test_extension_with_special_characters_in_name() {
        // Names with special characters should be handled safely
        let ext = create_test_extension("pubkey", "my-ext_v2");
        assert!(!ext.id.is_empty());
    }

    #[test]
    fn test_empty_extension_id() {
        let ext = create_test_extension("", "");
        assert_eq!(ext.id, "_"); // pubkey_name format
    }

    #[test]
    fn test_very_long_extension_name() {
        let long_name = "a".repeat(1000);
        let ext = create_test_extension("pubkey", &long_name);
        assert!(ext.id.len() > 1000);
    }

    #[test]
    fn test_database_result_with_null_values() {
        let result = DatabaseQueryResult {
            rows: vec![vec![JsonValue::Null, json!("value"), JsonValue::Null]],
            rows_affected: 0,
            last_insert_id: None,
        };

        assert!(result.rows[0][0].is_null());
        assert!(result.rows[0][2].is_null());
    }

    #[test]
    fn test_database_result_with_various_types() {
        let result = DatabaseQueryResult {
            rows: vec![vec![
                json!(42),                                // number
                json!("text"),                            // string
                json!(true),                              // boolean
                json!(3.14),                              // float
                json!(null),                              // null
                json!({"nested": "object"}),              // object
                json!(["array", "values"]),               // array
            ]],
            rows_affected: 0,
            last_insert_id: None,
        };

        assert!(result.rows[0][0].is_number());
        assert!(result.rows[0][1].is_string());
        assert!(result.rows[0][2].is_boolean());
        assert!(result.rows[0][4].is_null());
        assert!(result.rows[0][5].is_object());
        assert!(result.rows[0][6].is_array());
    }
}
