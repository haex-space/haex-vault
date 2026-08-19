//! L5 per-handler rate gate: the last defense layer against
//! "valid-but-evil" request floods from an authenticated peer.
//!
//! Runs after the AuthGate has produced a `verified_did`. Uses the
//! existing [`RejectRateTracker`] as the sliding-window counter (key
//! namespace `l5:{did}:{op_name}` so L1/L4 buckets never collide).
//! Only handlers listed in [`HandlerRateLimits::limit_for_op`] are
//! rate-limited — cheap or session-lifecycle handlers pass through as
//! [`HandlerRateOutcome::NoLimit`] and the dispatcher continues without
//! recording anything.
//!
//! Enforcement point: `space_delivery::local::leader::dispatch::handle_delivery_request`,
//! immediately after `authorize_request` returns Ok and before the
//! per-variant match. See ADR-context in
//! `docs/plans/2026-06-13-leader-reject-rate-limit.md` §Phase 4.

use std::time::Instant;

use super::config::HandlerRateLimits;
use super::tracker::{RejectRateTracker, SingleAcceptOutcome};

/// Result of an L5 handler rate check.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HandlerRateOutcome {
    /// The request is under the per-DID per-window cap for this handler
    /// AND the event has been recorded. The dispatcher may proceed.
    Accepted,
    /// The request is at or above the per-DID per-window cap for this
    /// handler. Nothing was recorded — the reject is not counted against
    /// the peer's own bucket. The dispatcher must return
    /// `Response::Error { message: rate-limit diagnostic }`.
    Rejected { limit: u32, observed: usize },
    /// This handler is not L5-rate-limited (either not in the expensive
    /// set, or the client sent a bypass variant like `Announce`). No
    /// action taken.
    NoLimit,
}

/// Atomic check-and-record for one (DID, handler) rate bucket. See
/// [`HandlerRateOutcome`] for the tri-state result.
///
/// `now` is threaded in as [`Instant`] so tests can pin time
/// deterministically — same convention as [`RejectRateTracker`].
pub fn check_and_record(
    tracker: &RejectRateTracker,
    limits: &HandlerRateLimits,
    op_name: &str,
    verified_did: &str,
    now: Instant,
) -> HandlerRateOutcome {
    let Some(limit) = limits.limit_for_op(op_name) else {
        return HandlerRateOutcome::NoLimit;
    };
    // `l5:` prefix keeps this namespace disjoint from L1's global/source
    // keys and L4's per-DID reject counters. All three live on the same
    // tracker instance to avoid a second mutex per request path.
    let key = format!("l5:{verified_did}:{op_name}");
    match tracker.try_record_single(&key, limit as usize, now) {
        SingleAcceptOutcome::Accepted => HandlerRateOutcome::Accepted,
        SingleAcceptOutcome::Rejected(observed) => HandlerRateOutcome::Rejected { limit, observed },
    }
}
