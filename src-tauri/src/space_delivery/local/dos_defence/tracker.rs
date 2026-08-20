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

/// Bucket count above which an insert also sweeps the whole map.
///
/// Buckets self-clean only when their own key is touched again
/// (`prune_and_count` / `count_within_window` evict on empty), so a key that
/// sees one event and never returns — an L5 `l5:{did}:{op}` bucket from a peer
/// that issued one request and disconnected, and there are two such keys per
/// handler once `l5log:` is counted — would linger for the whole leader
/// session. `distinct_keys_count` does sweep, but production only calls it on
/// the endpoint's L1 accept-tracker, never on the Leader instance the L4/L5
/// keys live on.
///
/// This is a memory bound, not a security parameter: the sweep is O(keys) and
/// amortized, since it can only run again after enough *fresh* keys have been
/// inserted to cross the threshold once more.
const SWEEP_THRESHOLD: usize = 1024;

pub struct RejectRateTracker {
    window: Duration,
    buckets: Mutex<HashMap<String, VecDeque<Instant>>>,
}

/// Outcome of `try_record_l1_accept`. The `usize` is the count observed
/// in the rejecting bucket at decision time — useful for logging.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum L1AcceptOutcome {
    Accepted,
    RejectedGlobal(usize),
    RejectedPerSource(usize),
}

/// Outcome of `try_record_single` (L5 per-handler rate limit). The
/// `usize` is the count observed in the bucket at decision time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SingleAcceptOutcome {
    Accepted,
    Rejected(usize),
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
        sweep_if_large(&mut buckets, when.checked_sub(self.window));
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

    /// Atomic check-and-record for the L1 accept-rate decision. Reads
    /// both buckets, checks both limits, and records the accept event in
    /// both buckets — all inside a single mutex acquisition. Without this
    /// atomicity, two concurrent accept tasks can each observe counts
    /// below the cap and then each record, producing bursts beyond
    /// `*_limit`. See PR #560 review for the race scenario.
    ///
    /// Returns:
    /// - `L1AcceptOutcome::Accepted` — both buckets were under their cap;
    ///   both have had `when` recorded.
    /// - `L1AcceptOutcome::RejectedGlobal(count)` — global bucket already
    ///   at or above `global_limit`; nothing was recorded.
    /// - `L1AcceptOutcome::RejectedPerSource(count)` — global was under
    ///   cap but the per-source bucket is at or above `source_limit`;
    ///   nothing was recorded.
    pub fn try_record_l1_accept(
        &self,
        global_key: &str,
        global_limit: usize,
        source_key: &str,
        source_limit: usize,
        when: Instant,
    ) -> L1AcceptOutcome {
        let cutoff = when.checked_sub(self.window);
        let mut buckets = self
            .buckets
            .lock()
            .expect("RejectRateTracker mutex poisoned");

        let global_count = prune_and_count(&mut buckets, global_key, cutoff);
        if global_count >= global_limit {
            return L1AcceptOutcome::RejectedGlobal(global_count);
        }

        let source_count = prune_and_count(&mut buckets, source_key, cutoff);
        if source_count >= source_limit {
            return L1AcceptOutcome::RejectedPerSource(source_count);
        }

        buckets
            .entry(global_key.to_string())
            .or_default()
            .push_back(when);
        buckets
            .entry(source_key.to_string())
            .or_default()
            .push_back(when);
        sweep_if_large(&mut buckets, cutoff);
        L1AcceptOutcome::Accepted
    }

    /// Atomic check-and-record for a single-key rate limit (L5 per-handler).
    /// Prunes + counts under the same mutex acquisition that records the
    /// event, so two concurrent requests cannot both observe count < limit
    /// and then both record — same rationale as `try_record_l1_accept`.
    pub fn try_record_single(&self, key: &str, limit: usize, when: Instant) -> SingleAcceptOutcome {
        let cutoff = when.checked_sub(self.window);
        let mut buckets = self
            .buckets
            .lock()
            .expect("RejectRateTracker mutex poisoned");

        let count = prune_and_count(&mut buckets, key, cutoff);
        if count >= limit {
            return SingleAcceptOutcome::Rejected(count);
        }

        buckets.entry(key.to_string()).or_default().push_back(when);
        sweep_if_large(&mut buckets, cutoff);
        SingleAcceptOutcome::Accepted
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

/// Drop every bucket whose entries have all expired, once the map has grown
/// past [`SWEEP_THRESHOLD`]. Called from the record paths because a key that
/// is never touched again is exactly the one no per-key prune can reach.
fn sweep_if_large(buckets: &mut HashMap<String, VecDeque<Instant>>, cutoff: Option<Instant>) {
    if buckets.len() <= SWEEP_THRESHOLD {
        return;
    }
    buckets.retain(|_, entries| {
        prune_expired(entries, cutoff);
        !entries.is_empty()
    });
}

fn prune_expired(entries: &mut VecDeque<Instant>, cutoff: Option<Instant>) {
    let Some(cutoff) = cutoff else { return };
    while entries.front().is_some_and(|t| *t < cutoff) {
        entries.pop_front();
    }
}

/// Prune expired entries from one bucket and return the remaining count.
/// Removes the bucket entirely if pruning leaves it empty — matches
/// `count_within_window`'s unbounded-growth guard.
fn prune_and_count(
    buckets: &mut HashMap<String, VecDeque<Instant>>,
    key: &str,
    cutoff: Option<Instant>,
) -> usize {
    let Some(entries) = buckets.get_mut(key) else {
        return 0;
    };
    prune_expired(entries, cutoff);
    let count = entries.len();
    if count == 0 {
        buckets.remove(key);
    }
    count
}
