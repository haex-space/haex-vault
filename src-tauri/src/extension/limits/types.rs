// src-tauri/src/extension/limits/types.rs
//!
//! Common type definitions for extension limits

use crate::database::error::DatabaseError;
use crate::database::generated::HaexExtensionLimits;

/// Database-specific limits
#[derive(Debug, Clone)]
pub struct DatabaseLimits {
    /// Query timeout in milliseconds (default: 30000 = 30 seconds)
    pub query_timeout_ms: i64,
    /// Maximum rows returned per query (default: 10000)
    pub max_result_rows: i64,
    /// Maximum concurrent queries per extension (default: 5)
    pub max_concurrent_queries: i64,
    /// Maximum query SQL size in bytes (default: 1MB)
    pub max_query_size_bytes: i64,
}

impl Default for DatabaseLimits {
    fn default() -> Self {
        Self {
            query_timeout_ms: 30_000,
            max_result_rows: 10_000,
            max_concurrent_queries: 5,
            max_query_size_bytes: 1_048_576,
        }
    }
}

/// Filesystem-specific limits
#[derive(Debug, Clone)]
pub struct FilesystemLimits {
    /// Maximum storage in bytes per extension (default: 100MB)
    pub max_storage_bytes: i64,
    /// Maximum single file size in bytes (default: 50MB)
    pub max_file_size_bytes: i64,
    /// Maximum concurrent file operations (default: 10)
    pub max_concurrent_operations: i64,
}

impl Default for FilesystemLimits {
    fn default() -> Self {
        Self {
            max_storage_bytes: 100 * 1024 * 1024,  // 100MB
            max_file_size_bytes: 50 * 1024 * 1024, // 50MB
            max_concurrent_operations: 10,
        }
    }
}

/// Web request-specific limits
#[derive(Debug, Clone)]
pub struct WebLimits {
    /// Maximum requests per minute (default: 60)
    pub max_requests_per_minute: i64,
    /// Maximum bandwidth in bytes per minute (default: 10MB)
    pub max_bandwidth_bytes_per_minute: i64,
    /// Maximum concurrent requests (default: 5)
    pub max_concurrent_requests: i64,
}

impl Default for WebLimits {
    fn default() -> Self {
        Self {
            max_requests_per_minute: 60,
            max_bandwidth_bytes_per_minute: 10 * 1024 * 1024, // 10MB
            max_concurrent_requests: 5,
        }
    }
}

/// Default limits for all resource types
#[derive(Debug, Clone, Default)]
pub struct DefaultLimits {
    pub database: DatabaseLimits,
    pub filesystem: FilesystemLimits,
    pub web: WebLimits,
}

/// Resolved limits for a specific extension (all resource types)
#[derive(Debug, Clone)]
pub struct ExtensionLimits {
    pub database: DatabaseLimits,
    pub filesystem: FilesystemLimits,
    pub web: WebLimits,
}

impl From<HaexExtensionLimits> for ExtensionLimits {
    fn from(db: HaexExtensionLimits) -> Self {
        Self {
            database: DatabaseLimits {
                query_timeout_ms: db.query_timeout_ms,
                max_result_rows: db.max_result_rows,
                max_concurrent_queries: db.max_concurrent_queries,
                max_query_size_bytes: db.max_query_size_bytes,
            },
            // Use defaults for other resource types until we add columns for them
            filesystem: FilesystemLimits::default(),
            web: WebLimits::default(),
        }
    }
}

impl From<&DefaultLimits> for ExtensionLimits {
    fn from(defaults: &DefaultLimits) -> Self {
        Self {
            database: defaults.database.clone(),
            filesystem: defaults.filesystem.clone(),
            web: defaults.web.clone(),
        }
    }
}

/// Errors specific to limit enforcement, grouped by resource type
#[derive(Debug, Clone)]
pub enum LimitError {
    // === Database limit errors ===
    /// Query SQL exceeds maximum allowed size
    QueryTooLarge { size: usize, max_size: i64 },
    /// Query exceeded configured timeout
    QueryTimeout { timeout_ms: i64 },
    /// Result set exceeds maximum allowed rows
    ResultTooLarge { rows: usize, max_rows: i64 },
    /// Too many concurrent queries for this extension
    TooManyConcurrentQueries { current: usize, max: i64 },

    // === Filesystem limit errors ===
    /// Storage quota exceeded
    StorageQuotaExceeded { used: i64, max: i64 },
    /// Single file too large
    FileTooLarge { size: i64, max: i64 },
    /// Too many concurrent file operations
    TooManyConcurrentFileOps { current: usize, max: i64 },

    // === Web request limit errors ===
    /// Rate limit exceeded
    RateLimitExceeded { requests: usize, max: i64 },
    /// Bandwidth limit exceeded
    BandwidthExceeded { bytes: i64, max: i64 },
    /// Too many concurrent web requests
    TooManyConcurrentWebRequests { current: usize, max: i64 },
}

impl std::fmt::Display for LimitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            // Database errors
            LimitError::QueryTooLarge { size, max_size } => {
                write!(
                    f,
                    "Query SQL exceeds maximum size: {} bytes (limit: {} bytes)",
                    size, max_size
                )
            }
            LimitError::QueryTimeout { timeout_ms } => {
                write!(f, "Query exceeded timeout of {} ms", timeout_ms)
            }
            LimitError::ResultTooLarge { rows, max_rows } => {
                write!(
                    f,
                    "Query result exceeds maximum rows: {} (limit: {})",
                    rows, max_rows
                )
            }
            LimitError::TooManyConcurrentQueries { current, max } => {
                write!(
                    f,
                    "Too many concurrent queries: {} (limit: {})",
                    current, max
                )
            }
            // Filesystem errors
            LimitError::StorageQuotaExceeded { used, max } => {
                write!(
                    f,
                    "Storage quota exceeded: {} bytes used (limit: {} bytes)",
                    used, max
                )
            }
            LimitError::FileTooLarge { size, max } => {
                write!(f, "File too large: {} bytes (limit: {} bytes)", size, max)
            }
            LimitError::TooManyConcurrentFileOps { current, max } => {
                write!(
                    f,
                    "Too many concurrent file operations: {} (limit: {})",
                    current, max
                )
            }
            // Web errors
            LimitError::RateLimitExceeded { requests, max } => {
                write!(
                    f,
                    "Rate limit exceeded: {} requests (limit: {} per minute)",
                    requests, max
                )
            }
            LimitError::BandwidthExceeded { bytes, max } => {
                write!(
                    f,
                    "Bandwidth limit exceeded: {} bytes (limit: {} bytes per minute)",
                    bytes, max
                )
            }
            LimitError::TooManyConcurrentWebRequests { current, max } => {
                write!(
                    f,
                    "Too many concurrent web requests: {} (limit: {})",
                    current, max
                )
            }
        }
    }
}

impl std::error::Error for LimitError {}

impl From<LimitError> for DatabaseError {
    fn from(e: LimitError) -> Self {
        DatabaseError::LimitExceeded {
            reason: e.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_database_limits() {
        let limits = DatabaseLimits::default();
        assert_eq!(limits.query_timeout_ms, 30_000);
        assert_eq!(limits.max_result_rows, 10_000);
        assert_eq!(limits.max_concurrent_queries, 5);
        assert_eq!(limits.max_query_size_bytes, 1_048_576);
    }

    #[test]
    fn test_default_filesystem_limits() {
        let limits = FilesystemLimits::default();
        assert_eq!(limits.max_storage_bytes, 100 * 1024 * 1024);
        assert_eq!(limits.max_file_size_bytes, 50 * 1024 * 1024);
        assert_eq!(limits.max_concurrent_operations, 10);
    }

    #[test]
    fn test_default_web_limits() {
        let limits = WebLimits::default();
        assert_eq!(limits.max_requests_per_minute, 60);
        assert_eq!(limits.max_bandwidth_bytes_per_minute, 10 * 1024 * 1024);
        assert_eq!(limits.max_concurrent_requests, 5);
    }

    #[test]
    fn test_extension_limits_from_defaults() {
        let defaults = DefaultLimits::default();
        let limits: ExtensionLimits = (&defaults).into();

        assert_eq!(limits.database.query_timeout_ms, 30_000);
        assert_eq!(limits.filesystem.max_storage_bytes, 100 * 1024 * 1024);
        assert_eq!(limits.web.max_requests_per_minute, 60);
    }

    #[test]
    fn test_limit_error_display_database() {
        let error = LimitError::QueryTooLarge {
            size: 2_000_000,
            max_size: 1_000_000,
        };
        assert!(error.to_string().contains("2000000"));
        assert!(error.to_string().contains("1000000"));

        let error = LimitError::QueryTimeout { timeout_ms: 30_000 };
        assert!(error.to_string().contains("30000"));

        let error = LimitError::ResultTooLarge {
            rows: 15_000,
            max_rows: 10_000,
        };
        assert!(error.to_string().contains("15000"));
        assert!(error.to_string().contains("10000"));

        let error = LimitError::TooManyConcurrentQueries { current: 5, max: 5 };
        assert!(error.to_string().contains("5"));
    }

    #[test]
    fn test_limit_error_display_filesystem() {
        let error = LimitError::StorageQuotaExceeded {
            used: 200_000_000,
            max: 100_000_000,
        };
        assert!(error.to_string().contains("200000000"));

        let error = LimitError::FileTooLarge {
            size: 100_000_000,
            max: 50_000_000,
        };
        assert!(error.to_string().contains("100000000"));
    }

    #[test]
    fn test_limit_error_display_web() {
        let error = LimitError::RateLimitExceeded {
            requests: 100,
            max: 60,
        };
        assert!(error.to_string().contains("100"));
        assert!(error.to_string().contains("60"));

        let error = LimitError::BandwidthExceeded {
            bytes: 20_000_000,
            max: 10_000_000,
        };
        assert!(error.to_string().contains("20000000"));
    }
}
