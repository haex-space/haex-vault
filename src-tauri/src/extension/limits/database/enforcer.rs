// src-tauri/src/extension/limits/database/enforcer.rs
//!
//! Database limit enforcement implementation

use crate::extension::limits::types::{DatabaseLimits, LimitError};
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, RwLock};

/// Tracks concurrent query counts per extension
#[derive(Debug, Default)]
pub struct ConcurrencyTracker {
    /// Map from extension_id to active query count
    counts: RwLock<HashMap<String, Arc<AtomicUsize>>>,
}

impl ConcurrencyTracker {
    pub fn new() -> Self {
        Self {
            counts: RwLock::new(HashMap::new()),
        }
    }

    /// Acquire a query slot for an extension
    /// Returns the current count AFTER incrementing
    pub fn acquire(&self, extension_id: &str) -> usize {
        let counter = {
            let counts = self.counts.read().unwrap();
            counts.get(extension_id).cloned()
        };

        match counter {
            Some(counter) => counter.fetch_add(1, Ordering::SeqCst) + 1,
            None => {
                let mut counts = self.counts.write().unwrap();
                let counter = counts
                    .entry(extension_id.to_string())
                    .or_insert_with(|| Arc::new(AtomicUsize::new(0)));
                counter.fetch_add(1, Ordering::SeqCst) + 1
            }
        }
    }

    /// Release a query slot for an extension
    pub fn release(&self, extension_id: &str) {
        let counts = self.counts.read().unwrap();
        if let Some(counter) = counts.get(extension_id) {
            counter.fetch_sub(1, Ordering::SeqCst);
        }
    }

    /// Get current count for an extension
    pub fn get_count(&self, extension_id: &str) -> usize {
        let counts = self.counts.read().unwrap();
        counts
            .get(extension_id)
            .map(|c| c.load(Ordering::SeqCst))
            .unwrap_or(0)
    }
}

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
