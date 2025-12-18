// src-tauri/src/extension/database/tests/types_tests.rs
//!
//! Tests for extension database types
//!

use crate::extension::database::types::MigrationResult;

#[cfg(test)]
mod tests {
    use super::*;

    // ============================================================================
    // MigrationResult Tests
    // ============================================================================

    #[test]
    fn test_migration_result_serialization_empty() {
        let result = MigrationResult {
            applied_count: 0,
            already_applied_count: 0,
            applied_migrations: vec![],
        };

        let json = serde_json::to_string(&result).unwrap();

        assert!(json.contains("\"appliedCount\":0"));
        assert!(json.contains("\"alreadyAppliedCount\":0"));
        assert!(json.contains("\"appliedMigrations\":[]"));
    }

    #[test]
    fn test_migration_result_serialization_with_migrations() {
        let result = MigrationResult {
            applied_count: 3,
            already_applied_count: 2,
            applied_migrations: vec![
                "001_create_users".to_string(),
                "002_create_posts".to_string(),
                "003_add_index".to_string(),
            ],
        };

        let json = serde_json::to_string(&result).unwrap();

        assert!(json.contains("\"appliedCount\":3"));
        assert!(json.contains("\"alreadyAppliedCount\":2"));
        assert!(json.contains("\"001_create_users\""));
        assert!(json.contains("\"002_create_posts\""));
        assert!(json.contains("\"003_add_index\""));
    }

    #[test]
    fn test_migration_result_camel_case_keys() {
        let result = MigrationResult {
            applied_count: 1,
            already_applied_count: 5,
            applied_migrations: vec!["migration_name".to_string()],
        };

        let json = serde_json::to_string(&result).unwrap();

        // Ensure camelCase serialization
        assert!(json.contains("appliedCount"));
        assert!(!json.contains("applied_count"));
        assert!(json.contains("alreadyAppliedCount"));
        assert!(!json.contains("already_applied_count"));
        assert!(json.contains("appliedMigrations"));
        assert!(!json.contains("applied_migrations"));
    }

    #[test]
    fn test_migration_result_debug() {
        let result = MigrationResult {
            applied_count: 2,
            already_applied_count: 1,
            applied_migrations: vec!["test".to_string()],
        };

        let debug_str = format!("{:?}", result);

        assert!(debug_str.contains("MigrationResult"));
        assert!(debug_str.contains("applied_count: 2"));
        assert!(debug_str.contains("already_applied_count: 1"));
    }

    #[test]
    fn test_migration_result_large_numbers() {
        let result = MigrationResult {
            applied_count: 999999,
            already_applied_count: 888888,
            applied_migrations: (0..1000)
                .map(|i| format!("migration_{:04}", i))
                .collect(),
        };

        let json = serde_json::to_string(&result).unwrap();

        assert!(json.contains("\"appliedCount\":999999"));
        assert!(json.contains("\"alreadyAppliedCount\":888888"));
        assert!(json.contains("\"migration_0000\""));
        assert!(json.contains("\"migration_0999\""));
    }

    #[test]
    fn test_migration_result_special_chars_in_names() {
        let result = MigrationResult {
            applied_count: 2,
            already_applied_count: 0,
            applied_migrations: vec![
                "001_create_users_table".to_string(),
                "002-add-email-column".to_string(),
            ],
        };

        let json = serde_json::to_string(&result).unwrap();

        assert!(json.contains("001_create_users_table"));
        assert!(json.contains("002-add-email-column"));
    }

    #[test]
    fn test_migration_result_unicode_names() {
        let result = MigrationResult {
            applied_count: 1,
            already_applied_count: 0,
            applied_migrations: vec!["001_создать_таблицу".to_string()],
        };

        let json = serde_json::to_string(&result).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert_eq!(
            parsed["appliedMigrations"][0].as_str().unwrap(),
            "001_создать_таблицу"
        );
    }
}
