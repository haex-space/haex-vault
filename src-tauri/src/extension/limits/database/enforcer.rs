// src-tauri/src/extension/limits/database/enforcer.rs
//!
//! Database limit enforcement implementation

use crate::extension::limits::shared::ConcurrencyTracker;
use crate::extension::limits::types::{DatabaseLimits, LimitError};

/// RAII guard for concurrent query tracking
pub struct QueryGuard<'a> {
    tracker: &'a ConcurrencyTracker,
    extension_id: String,
}

impl<'a> QueryGuard<'a> {
    pub fn new(tracker: &'a ConcurrencyTracker, extension_id: String) -> Self {
        tracker.acquire(&extension_id);
        Self {
            tracker,
            extension_id,
        }
    }
}

impl Drop for QueryGuard<'_> {
    fn drop(&mut self) {
        self.tracker.release(&self.extension_id);
    }
}

/// Database limit enforcer
#[derive(Debug, Default)]
pub struct DatabaseLimitEnforcer {
    concurrency: ConcurrencyTracker,
}

impl DatabaseLimitEnforcer {
    pub fn new() -> Self {
        Self {
            concurrency: ConcurrencyTracker::new(),
        }
    }

    /// Validate query size against limits
    pub fn validate_query_size(&self, sql: &str, limits: &DatabaseLimits) -> Result<(), LimitError> {
        let size = sql.len();
        if size as i64 > limits.max_query_size_bytes {
            return Err(LimitError::QueryTooLarge {
                size,
                max_size: limits.max_query_size_bytes,
            });
        }
        Ok(())
    }

    /// Check and acquire a concurrent query slot
    /// Returns a guard that releases the slot on drop
    pub fn acquire_query_slot<'a>(
        &'a self,
        extension_id: &str,
        limits: &DatabaseLimits,
    ) -> Result<QueryGuard<'a>, LimitError> {
        let current = self.concurrency.get_count(extension_id);
        if current as i64 >= limits.max_concurrent_queries {
            return Err(LimitError::TooManyConcurrentQueries {
                current,
                max: limits.max_concurrent_queries,
            });
        }

        Ok(QueryGuard::new(&self.concurrency, extension_id.to_string()))
    }

    /// Validate result row count against limits
    pub fn validate_result_rows(
        &self,
        rows: usize,
        limits: &DatabaseLimits,
    ) -> Result<(), LimitError> {
        if rows as i64 > limits.max_result_rows {
            return Err(LimitError::ResultTooLarge {
                rows,
                max_rows: limits.max_result_rows,
            });
        }
        Ok(())
    }

    /// Get the concurrency tracker reference
    pub fn concurrency(&self) -> &ConcurrencyTracker {
        &self.concurrency
    }
}
