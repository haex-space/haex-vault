// src-tauri/src/extension/limits/service.rs
//!
//! Limits service - central service for managing extension limits

use super::database::DatabaseLimitEnforcer;
use super::filesystem::FilesystemLimitEnforcer;
use super::types::{DefaultLimits, ExtensionLimits};
use super::web::WebLimitEnforcer;
use crate::database::error::DatabaseError;
use crate::database::generated::HaexExtensionLimits;
use rusqlite::Connection;

/// Central service for managing and enforcing extension limits
#[derive(Debug)]
pub struct LimitsService {
    defaults: DefaultLimits,
    database: DatabaseLimitEnforcer,
    filesystem: FilesystemLimitEnforcer,
    web: WebLimitEnforcer,
}

impl Default for LimitsService {
    fn default() -> Self {
        Self::new()
    }
}

impl LimitsService {
    pub fn new() -> Self {
        Self {
            defaults: DefaultLimits::default(),
            database: DatabaseLimitEnforcer::new(),
            filesystem: FilesystemLimitEnforcer::new(),
            web: WebLimitEnforcer::new(),
        }
    }

    pub fn with_defaults(defaults: DefaultLimits) -> Self {
        Self {
            defaults,
            database: DatabaseLimitEnforcer::new(),
            filesystem: FilesystemLimitEnforcer::new(),
            web: WebLimitEnforcer::new(),
        }
    }

    /// Get limits for an extension from database, or use defaults
    pub fn get_limits(
        &self,
        conn: &Connection,
        extension_id: &str,
    ) -> Result<ExtensionLimits, DatabaseError> {
        let result: Result<HaexExtensionLimits, _> = conn.query_row(
            "SELECT id, extension_id, query_timeout_ms, max_result_rows, \
             max_concurrent_queries, max_query_size_bytes, created_at, updated_at \
             FROM haex_extension_limits \
             WHERE extension_id = ? AND IFNULL(haex_tombstone, 0) = 0",
            [extension_id],
            |row| HaexExtensionLimits::from_row(row),
        );

        match result {
            Ok(limits) => Ok(limits.into()),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok((&self.defaults).into()),
            Err(e) => Err(DatabaseError::QueryError {
                reason: e.to_string(),
            }),
        }
    }

    /// Get the database limit enforcer
    pub fn database(&self) -> &DatabaseLimitEnforcer {
        &self.database
    }

    /// Get the filesystem limit enforcer
    pub fn filesystem(&self) -> &FilesystemLimitEnforcer {
        &self.filesystem
    }

    /// Get the web limit enforcer
    pub fn web(&self) -> &WebLimitEnforcer {
        &self.web
    }

    /// Get the default limits
    pub fn defaults(&self) -> &DefaultLimits {
        &self.defaults
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_limits_service_new() {
        let service = LimitsService::new();
        let defaults = service.defaults();

        assert_eq!(defaults.database.query_timeout_ms, 30_000);
        assert_eq!(defaults.database.max_result_rows, 10_000);
        assert_eq!(defaults.filesystem.max_storage_bytes, 100 * 1024 * 1024);
        assert_eq!(defaults.web.max_requests_per_minute, 60);
    }

    #[test]
    fn test_limits_service_with_custom_defaults() {
        use crate::extension::limits::types::{DatabaseLimits, FilesystemLimits, WebLimits};

        let custom_defaults = DefaultLimits {
            database: DatabaseLimits {
                query_timeout_ms: 60_000,
                max_result_rows: 5_000,
                max_concurrent_queries: 10,
                max_query_size_bytes: 2_000_000,
            },
            filesystem: FilesystemLimits::default(),
            web: WebLimits::default(),
        };

        let service = LimitsService::with_defaults(custom_defaults);
        let defaults = service.defaults();

        assert_eq!(defaults.database.query_timeout_ms, 60_000);
        assert_eq!(defaults.database.max_result_rows, 5_000);
    }

    #[test]
    fn test_limits_service_enforcers_accessible() {
        let service = LimitsService::new();

        // Just verify we can access the enforcers
        let _ = service.database();
        let _ = service.filesystem();
        let _ = service.web();
    }
}
