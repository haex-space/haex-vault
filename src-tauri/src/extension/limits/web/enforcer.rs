// src-tauri/src/extension/limits/web/enforcer.rs
//!
//! Web request limit enforcement implementation (placeholder)

use crate::extension::limits::shared::ConcurrencyTracker;
use crate::extension::limits::types::{LimitError, WebLimits};
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

/// Rate limit window entry with byte tracking for web requests
#[derive(Debug)]
struct RateLimitWindow {
    count: AtomicUsize,
    bytes: std::sync::atomic::AtomicI64,
    window_start: RwLock<Instant>,
}

impl RateLimitWindow {
    fn new() -> Self {
        Self {
            count: AtomicUsize::new(0),
            bytes: std::sync::atomic::AtomicI64::new(0),
            window_start: RwLock::new(Instant::now()),
        }
    }

    fn reset_if_expired(&self, window_duration: Duration) {
        let mut window_start = self.window_start.write().unwrap_or_else(|e| e.into_inner());
        if window_start.elapsed() >= window_duration {
            self.count.store(0, Ordering::SeqCst);
            self.bytes.store(0, Ordering::SeqCst);
            *window_start = Instant::now();
        }
    }

    fn increment_count(&self) -> usize {
        self.count.fetch_add(1, Ordering::SeqCst) + 1
    }

    fn add_bytes(&self, bytes: i64) -> i64 {
        self.bytes.fetch_add(bytes, Ordering::SeqCst) + bytes
    }

    fn get_count(&self) -> usize {
        self.count.load(Ordering::SeqCst)
    }

    fn get_bytes(&self) -> i64 {
        self.bytes.load(Ordering::SeqCst)
    }
}


/// RAII guard for concurrent web requests
pub struct WebRequestGuard<'a> {
    tracker: &'a ConcurrencyTracker,
    extension_id: String,
}

impl<'a> WebRequestGuard<'a> {
    pub fn new(tracker: &'a ConcurrencyTracker, extension_id: String) -> Self {
        tracker.acquire(&extension_id);
        Self {
            tracker,
            extension_id,
        }
    }
}

impl Drop for WebRequestGuard<'_> {
    fn drop(&mut self) {
        self.tracker.release(&self.extension_id);
    }
}

/// Web request limit enforcer
#[derive(Debug, Default)]
pub struct WebLimitEnforcer {
    concurrency: ConcurrencyTracker,
    rate_limits: RwLock<HashMap<String, Arc<RateLimitWindow>>>,
}

impl WebLimitEnforcer {
    pub fn new() -> Self {
        Self {
            concurrency: ConcurrencyTracker::new(),
            rate_limits: RwLock::new(HashMap::new()),
        }
    }

    fn get_or_create_rate_limit(&self, extension_id: &str) -> Arc<RateLimitWindow> {
        {
            let rate_limits = self.rate_limits.read().unwrap_or_else(|e| e.into_inner());
            if let Some(window) = rate_limits.get(extension_id) {
                return Arc::clone(window);
            }
        }

        let mut rate_limits = self.rate_limits.write().unwrap_or_else(|e| e.into_inner());
        let window = rate_limits
            .entry(extension_id.to_string())
            .or_insert_with(|| Arc::new(RateLimitWindow::new()));
        Arc::clone(window)
    }

    /// Check and record a request for rate limiting
    pub fn check_rate_limit(
        &self,
        extension_id: &str,
        limits: &WebLimits,
    ) -> Result<(), LimitError> {
        let window = self.get_or_create_rate_limit(extension_id);
        window.reset_if_expired(Duration::from_secs(60));

        let current = window.get_count();
        if current as i64 >= limits.max_requests_per_minute {
            return Err(LimitError::RateLimitExceeded {
                requests: current,
                max: limits.max_requests_per_minute,
            });
        }

        window.increment_count();
        Ok(())
    }

    /// Check and record bandwidth usage
    pub fn check_bandwidth(
        &self,
        extension_id: &str,
        bytes: i64,
        limits: &WebLimits,
    ) -> Result<(), LimitError> {
        let window = self.get_or_create_rate_limit(extension_id);
        window.reset_if_expired(Duration::from_secs(60));

        let current = window.get_bytes();
        if current + bytes > limits.max_bandwidth_bytes_per_minute {
            return Err(LimitError::BandwidthExceeded {
                bytes: current + bytes,
                max: limits.max_bandwidth_bytes_per_minute,
            });
        }

        window.add_bytes(bytes);
        Ok(())
    }

    /// Acquire a concurrent request slot
    pub fn acquire_request_slot<'a>(
        &'a self,
        extension_id: &str,
        limits: &WebLimits,
    ) -> Result<WebRequestGuard<'a>, LimitError> {
        let current = self.concurrency.get_count(extension_id);
        if current as i64 >= limits.max_concurrent_requests {
            return Err(LimitError::TooManyConcurrentWebRequests {
                current,
                max: limits.max_concurrent_requests,
            });
        }

        Ok(WebRequestGuard::new(
            &self.concurrency,
            extension_id.to_string(),
        ))
    }

    /// Get the concurrency tracker reference
    pub fn concurrency(&self) -> &ConcurrencyTracker {
        &self.concurrency
    }
}
