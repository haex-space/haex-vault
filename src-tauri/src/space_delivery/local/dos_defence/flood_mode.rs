//! L1-aggregated flood-mode state machine.
//!
//! Pure logic — no DB, no time-of-day clock. Callers feed in tracker
//! observations (`global_count`, `distinct_sources`) plus the current
//! `Instant`; the machine decides whether to stay in the current state or
//! transition. The DB write is a side effect the caller applies after a
//! `FloodTransition::*` is returned.
//!
//! Two transitions matter for Phase 3:
//!
//! - `Quiet | SingleSource → Ddos` when the global L1 accept-rate AND the
//!   distinct-source-count are simultaneously above their thresholds. This
//!   is the trigger for the contacts-only escalation.
//! - `Ddos → Quiet` when the auto-expiry deadline passes, or when the user
//!   acknowledges via the early-end command (handled by the caller).
//!
//! `Quiet → SingleSource` lives in `decision.rs::classify` (L4 reject-rate)
//! and is referenced here only as a passthrough state the machine recognises
//! so it can return the right `FloodTransition` when DDoS supersedes it.

use std::time::{Duration, Instant};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FloodMode {
    Quiet,
    SingleSource {
        did: String,
    },
    Ddos {
        /// Distinct source count observed at the moment of transition.
        source_count: usize,
        /// Wall-clock-ish deadline expressed as a monotonic `Instant`. The
        /// state machine never reads the system clock — callers convert to
        /// RFC3339 for DB persistence outside this module.
        expires_at: Instant,
    },
}

impl FloodMode {
    pub fn is_ddos(&self) -> bool {
        matches!(self, FloodMode::Ddos { .. })
    }
}

/// Outcome of `evaluate`. Callers persist on every `*Transition` variant and
/// emit a `FLOOD_DDOS` critical notification on `EnteredDdos`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FloodTransition {
    /// No change.
    NoChange,
    /// Transitioned into DDoS from Quiet or SingleSource. Carries the new
    /// state for convenience — callers persist this verbatim.
    EnteredDdos {
        source_count: usize,
        expires_at: Instant,
    },
    /// Auto-expiry elapsed; back to Quiet without user action.
    Expired,
}

/// Observation snapshot the caller feeds in. Decoupled from the tracker so
/// tests stay deterministic.
#[derive(Debug, Clone, Copy)]
pub struct FloodObservation {
    /// Number of accepts (or attempted accepts incl. rejected) within the
    /// global L1 sliding window. The `pre_auth_accept_check` path reads this
    /// from `tracker.count_within_window(ACCEPT_TRACKER_GLOBAL_KEY, now)`.
    pub global_count: usize,
    /// Distinct per-source keys with non-empty buckets in the window. Reads
    /// from `tracker.distinct_keys_count(now)` minus 1 if the global bucket
    /// is also stored under the same map — see caller note.
    pub distinct_sources: usize,
}

/// Threshold inputs the caller feeds in (typically derived from
/// `DosDefenceConfig`). Kept on the call signature instead of inside
/// `FloodMode` so config changes apply on the very next `evaluate` without
/// embedding stale state in the machine.
#[derive(Debug, Clone, Copy)]
pub struct FloodThresholds {
    /// `dosDefence.l1.globalRatePerSec` — same knob L1 already uses for
    /// silent-drop. Re-using it keeps the policy story coherent: by the
    /// time we'd hit DDoS-mode, L1 is already dropping under-cap connects.
    pub global_rate_per_sec: usize,
    /// `dosDefence.ddos.distinctSourcesThreshold`.
    pub distinct_sources_threshold: usize,
    /// `dosDefence.ddos.autoExpirySecs` — TTL for the contacts-only mode.
    pub auto_expiry: Duration,
}

/// Pure transition function. `current` is the persisted state, `obs` is the
/// fresh tracker observation, `now` is the call-time monotonic clock.
///
/// Returns `NoChange` when nothing needs to be written.
pub fn evaluate(
    current: &FloodMode,
    obs: FloodObservation,
    thresholds: FloodThresholds,
    now: Instant,
) -> FloodTransition {
    if let FloodMode::Ddos { expires_at, .. } = current {
        if now >= *expires_at {
            return FloodTransition::Expired;
        }
    }

    // `>=` rather than `>`: `RejectRateTracker::try_record_l1_accept` caps
    // the global bucket exactly AT `global_rate_per_sec` (it rejects when
    // `count >= limit`), so under sustained pressure the observation
    // saturates at the threshold value and never exceeds it. A strict `>`
    // would mean the DDoS branch never fires during a real flood. See
    // CodeRabbit review on PR #562.
    let over_global = obs.global_count >= thresholds.global_rate_per_sec;
    let over_distinct = obs.distinct_sources >= thresholds.distinct_sources_threshold;

    if over_global && over_distinct {
        match current {
            FloodMode::Ddos { .. } => FloodTransition::NoChange,
            _ => FloodTransition::EnteredDdos {
                source_count: obs.distinct_sources,
                expires_at: now + thresholds.auto_expiry,
            },
        }
    } else {
        FloodTransition::NoChange
    }
}

/// Apply a `FloodTransition` to a state value. Convenience so callers don't
/// branch on `EnteredDdos` vs `Expired` themselves — the state value they
/// hand to the persistence layer is what they pass in here.
pub fn apply(current: FloodMode, transition: &FloodTransition) -> FloodMode {
    match transition {
        FloodTransition::NoChange => current,
        FloodTransition::EnteredDdos {
            source_count,
            expires_at,
        } => FloodMode::Ddos {
            source_count: *source_count,
            expires_at: *expires_at,
        },
        FloodTransition::Expired => FloodMode::Quiet,
    }
}
