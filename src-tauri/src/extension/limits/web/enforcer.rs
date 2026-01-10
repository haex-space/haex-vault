// src-tauri/src/extension/limits/web/enforcer.rs
//!
//! Web request limit enforcement implementation (placeholder)

use crate::extension::limits::types::{LimitError, WebLimits};
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

/// Rate limit window entry
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
        let mut window_start = self.window_start.write().unwrap();
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

/// Tracks concurrent web requests per extension
#[derive(Debug, Default)]
pub struct WebRequestTracker {
    counts: RwLock<HashMap<String, Arc<AtomicUsize>>>,
}

impl WebRequestTracker {
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

/// RAII guard for concurrent web requests
pub struct WebRequestGuard<'a> {
    tracker: &'a WebRequestTracker,
    extension_id: String,
}

impl<'a> WebRequestGuard<'a> {
    pub fn new(tracker: &'a WebRequestTracker, extension_id: String) -> Self {
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
    concurrency: WebRequestTracker,
    rate_limits: RwLock<HashMap<String, Arc<RateLimitWindow>>>,
}

impl WebLimitEnforcer {
    pub fn new() -> Self {
        Self {
            concurrency: WebRequestTracker::new(),
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
    pub fn concurrency(&self) -> &WebRequestTracker {
        &self.concurrency
    }
}
