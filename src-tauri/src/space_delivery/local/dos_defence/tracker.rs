//! Sliding-window reject-rate tracker.
//!
//! Key-agnostic counter keyed by `String`. Production sites tap this with
//! a DID (`did:key:...`) or a transport `endpoint_id`. Time is injected as
//! [`Instant`] so tests stay deterministic.
//!
//! ## Mutex poisoning
//!
//! The internal `Mutex` `expect()`s on poison. A poisoned tracker mutex means
//! a thread panicked while counting — at that point the Leader is already
//! degraded. The unified `critical::lock_or_fail` helper would emit a banner
//! via `CriticalNotificationSink`, but adopting it here would couple the
//! tracker (a pure in-memory counter) to the sink. Deferred to the
//! notification-integration step in
//! `docs/plans/2026-06-13-leader-reject-rate-limit.md`.

#![allow(clippy::expect_used)]

use std::collections::{HashMap, VecDeque};
use std::sync::Mutex;
use std::time::{Duration, Instant};

pub struct RejectRateTracker {
    window: Duration,
    buckets: Mutex<HashMap<String, VecDeque<Instant>>>,
}

impl RejectRateTracker {
    pub fn new(window: Duration) -> Self {
        Self {
            window,
            buckets: Mutex::new(HashMap::new()),
        }
    }

    pub fn record(&self, key: &str, when: Instant) {
        let mut buckets = self
            .buckets
            .lock()
            .expect("RejectRateTracker mutex poisoned");
        buckets.entry(key.to_string()).or_default().push_back(when);
    }

    pub fn count_within_window(&self, key: &str, now: Instant) -> usize {
        let cutoff = now.checked_sub(self.window);
        let mut buckets = self
            .buckets
            .lock()
            .expect("RejectRateTracker mutex poisoned");
        let Some(entries) = buckets.get_mut(key) else {
            return 0;
        };
        prune_expired(entries, cutoff);
        if entries.is_empty() {
            // Evict empty buckets — without this, every transient DID
            // permanently retains its (empty) `VecDeque` and `buckets`
            // grows unbounded under endpoint-rotation floods. See
            // CodeRabbit review on PR #491.
            buckets.remove(key);
            return 0;
        }
        entries.len()
    }

    pub fn distinct_keys_count(&self, now: Instant) -> usize {
        let cutoff = now.checked_sub(self.window);
        let mut buckets = self
            .buckets
            .lock()
            .expect("RejectRateTracker mutex poisoned");
        buckets.retain(|_, entries| {
            prune_expired(entries, cutoff);
            !entries.is_empty()
        });
        buckets.len()
    }

    /// Number of keys currently stored. Exposed for memory-leak tests —
    /// production code only needs `distinct_keys_count` (which prunes).
    #[cfg(test)]
    pub fn bucket_count(&self) -> usize {
        self.buckets
            .lock()
            .expect("RejectRateTracker mutex poisoned")
            .len()
    }
}

fn prune_expired(entries: &mut VecDeque<Instant>, cutoff: Option<Instant>) {
    let Some(cutoff) = cutoff else { return };
    while entries.front().is_some_and(|t| *t < cutoff) {
        entries.pop_front();
    }
}
