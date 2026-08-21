//! L5 per-handler rate gate: the last defense layer against
//! "valid-but-evil" request floods from an authenticated peer.
//!
//! Runs after the AuthGate has produced a `verified_did`. Uses the
//! Leader's existing [`RejectRateTracker`] as the sliding-window counter
//! (key namespace `l5:{did}:{op_name}` so it cannot collide with the L4
//! per-DID reject counters that share that instance; L1's global/source
//! buckets live on the separate `PeerEndpoint::accept_tracker`).
//! Every handler is rate-limited except the session-establishment set in
//! [`HandlerRateLimits::EXEMPT_OPS`], which passes through as
//! [`HandlerRateOutcome::NoLimit`] so the dispatcher continues without
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
    // `l5:` prefix keeps this namespace disjoint from the L4 per-DID
    // reject counters that share this tracker instance (L1 uses its own
    // tracker on the endpoint, see `PeerEndpoint::accept_tracker`).
    let key = format!("l5:{verified_did}:{op_name}");
    match tracker.try_record_single(&key, limit as usize, now) {
        SingleAcceptOutcome::Accepted => HandlerRateOutcome::Accepted,
        SingleAcceptOutcome::Rejected(observed) => HandlerRateOutcome::Rejected { limit, observed },
    }
}

/// At most one `haex_logs_no_sync` row per (DID, handler) per window.
///
/// Without this the L5 reject path would write one audit row **per
/// rejected request**, so an authenticated peer flooding a rate-limited
/// handler turns the gate into a log-write amplifier against the Owner's
/// own vault — exactly the failure mode L4's `should_log_this_reject`
/// sampling exists to prevent (see `dos_defence::decision`). Uses a
/// dedicated `l5log:` bucket with a limit of 1 so the accounting never
/// perturbs the enforcement bucket above.
///
/// Returns `true` for the first reject in the window and `false` for every
/// further reject of the same (DID, handler) until the window slides.
pub fn should_log_reject(
    tracker: &RejectRateTracker,
    op_name: &str,
    verified_did: &str,
    now: Instant,
) -> bool {
    let key = format!("l5log:{verified_did}:{op_name}");
    matches!(
        tracker.try_record_single(&key, 1, now),
        SingleAcceptOutcome::Accepted
    )
}
