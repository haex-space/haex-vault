// src-tauri/src/extension/limits/filesystem/enforcer.rs
//!
//! Filesystem limit enforcement implementation

use crate::extension::limits::types::{FilesystemLimits, LimitError};
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

/// Tracks concurrent file operations per extension
#[derive(Debug, Default)]
pub struct FileOpTracker {
    counts: RwLock<HashMap<String, Arc<AtomicUsize>>>,
}

impl FileOpTracker {
    pub fn new() -> Self {
        Self {
            counts: RwLock::new(HashMap::new()),
        }
    }

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

    pub fn release(&self, extension_id: &str) {
        let counts = self.counts.read().unwrap();
        if let Some(counter) = counts.get(extension_id) {
            counter.fetch_sub(1, Ordering::SeqCst);
        }
    }

    pub fn get_count(&self, extension_id: &str) -> usize {
        let counts = self.counts.read().unwrap();
        counts
            .get(extension_id)
            .map(|c| c.load(Ordering::SeqCst))
            .unwrap_or(0)
    }
}

/// RAII guard for concurrent file operations
pub struct FileOpGuard<'a> {
    tracker: &'a FileOpTracker,
    extension_id: String,
}

impl<'a> FileOpGuard<'a> {
    pub fn new(tracker: &'a FileOpTracker, extension_id: String) -> Self {
        tracker.acquire(&extension_id);
        Self {
            tracker,
            extension_id,
        }
    }
}

impl Drop for FileOpGuard<'_> {
    fn drop(&mut self) {
        self.tracker.release(&self.extension_id);
    }
}

/// Rate limit window for tracking operations per minute
#[derive(Debug)]
struct RateLimitWindow {
    count: AtomicUsize,
    window_start: RwLock<Instant>,
}

impl RateLimitWindow {
    fn new() -> Self {
        Self {
            count: AtomicUsize::new(0),
            window_start: RwLock::new(Instant::now()),
        }
    }

    fn reset_if_expired(&self, window_duration: Duration) {
        let mut window_start = self.window_start.write().unwrap();
        if window_start.elapsed() >= window_duration {
            self.count.store(0, Ordering::SeqCst);
            *window_start = Instant::now();
        }
    }

    fn increment_count(&self) -> usize {
        self.count.fetch_add(1, Ordering::SeqCst) + 1
    }

    fn get_count(&self) -> usize {
        self.count.load(Ordering::SeqCst)
    }
}

/// Filesystem limit enforcer
#[derive(Debug, Default)]
pub struct FilesystemLimitEnforcer {
    concurrency: FileOpTracker,
    rate_limits: RwLock<HashMap<String, Arc<RateLimitWindow>>>,
}

impl FilesystemLimitEnforcer {
    pub fn new() -> Self {
        Self {
            concurrency: FileOpTracker::new(),
            rate_limits: RwLock::new(HashMap::new()),
        }
    }

    fn get_or_create_rate_limit(&self, extension_id: &str) -> Arc<RateLimitWindow> {
        {
            let rate_limits = self.rate_limits.read().unwrap();
            if let Some(window) = rate_limits.get(extension_id) {
                return Arc::clone(window);
            }
        }

        let mut rate_limits = self.rate_limits.write().unwrap();
        let window = rate_limits
            .entry(extension_id.to_string())
            .or_insert_with(|| Arc::new(RateLimitWindow::new()));
        Arc::clone(window)
    }

    /// Check and record a filesystem operation for rate limiting
    pub fn check_rate_limit(
        &self,
        extension_id: &str,
        limits: &FilesystemLimits,
    ) -> Result<(), LimitError> {
        let window = self.get_or_create_rate_limit(extension_id);
        window.reset_if_expired(Duration::from_secs(60));

        let current = window.get_count();
        if current as i64 >= limits.max_operations_per_minute {
            return Err(LimitError::FilesystemRateLimitExceeded {
                operations: current,
                max: limits.max_operations_per_minute,
            });
        }

        window.increment_count();
        Ok(())
    }

    /// Validate file size against limits
    pub fn validate_file_size(&self, size: i64, limits: &FilesystemLimits) -> Result<(), LimitError> {
        if size > limits.max_file_size_bytes {
            return Err(LimitError::FileTooLarge {
                size,
                max: limits.max_file_size_bytes,
            });
        }
        Ok(())
    }

    /// Validate storage quota
    pub fn validate_storage_quota(
        &self,
        current_usage: i64,
        additional_bytes: i64,
        limits: &FilesystemLimits,
    ) -> Result<(), LimitError> {
        let total = current_usage + additional_bytes;
        if total > limits.max_storage_bytes {
            return Err(LimitError::StorageQuotaExceeded {
                used: total,
                max: limits.max_storage_bytes,
            });
        }
        Ok(())
    }

    /// Acquire a file operation slot
    pub fn acquire_op_slot<'a>(
        &'a self,
        extension_id: &str,
        limits: &FilesystemLimits,
    ) -> Result<FileOpGuard<'a>, LimitError> {
        let current = self.concurrency.get_count(extension_id);
        if current as i64 >= limits.max_concurrent_operations {
            return Err(LimitError::TooManyConcurrentFileOps {
                current,
                max: limits.max_concurrent_operations,
            });
        }

        Ok(FileOpGuard::new(&self.concurrency, extension_id.to_string()))
    }

    /// Get the concurrency tracker reference
    pub fn concurrency(&self) -> &FileOpTracker {
        &self.concurrency
    }
}
