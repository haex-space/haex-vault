//! Unit tests for the L5 per-handler rate gate.

use std::time::{Duration, Instant};

use super::config::HandlerRateLimits;
use super::handler_rate_gate::{check_and_record, should_log_reject, HandlerRateOutcome};
use super::tracker::RejectRateTracker;

fn tracker() -> RejectRateTracker {
    RejectRateTracker::new(Duration::from_secs(1))
}

const DID: &str = "did:key:test-peer";

#[test]
fn unlisted_handlers_pass_through_as_no_limit() {
    let tracker = tracker();
    let limits = HandlerRateLimits::defaults();
    let now = Instant::now();

    // `Announce` and other cheap or bypass variants are intentionally not
    // in `limit_for_op` — the gate must return NoLimit without touching
    // the tracker (a live tracker would leak keys otherwise).
    let outcome = check_and_record(&tracker, &limits, "Announce", DID, now);
    assert_eq!(outcome, HandlerRateOutcome::NoLimit);

    // No bucket for this key means bucket_count stays 0.
    assert_eq!(tracker.bucket_count(), 0);
}

#[test]
fn accepts_while_under_limit_and_rejects_at_boundary() {
    let tracker = tracker();
    let limits = HandlerRateLimits::defaults();
    let now = Instant::now();

    // Default sync_pull limit is 5 requests/sec/DID.
    for i in 0..limits.sync_pull {
        let outcome = check_and_record(&tracker, &limits, "SyncPull", DID, now);
        assert_eq!(
            outcome,
            HandlerRateOutcome::Accepted,
            "request #{i} should be accepted (under limit)",
        );
    }

    // The request after the `limit`-th must be rejected.
    let rejected = check_and_record(&tracker, &limits, "SyncPull", DID, now);
    match rejected {
        HandlerRateOutcome::Rejected { limit, observed } => {
            assert_eq!(limit, limits.sync_pull);
            assert_eq!(observed, limits.sync_pull as usize);
        }
        other => panic!("expected Rejected at limit boundary, got {other:?}"),
    }
}

#[test]
fn reject_does_not_record_against_peer_bucket() {
    let tracker = tracker();
    let limits = HandlerRateLimits::defaults();
    let now = Instant::now();

    // Saturate the bucket to the limit.
    for _ in 0..limits.sync_pull {
        assert_eq!(
            check_and_record(&tracker, &limits, "SyncPull", DID, now),
            HandlerRateOutcome::Accepted,
        );
    }

    // A rejection at the boundary must not increment the bucket beyond
    // the observed count — otherwise the peer's own bucket would keep
    // growing under sustained flood and the reject observation would
    // report inflated numbers.
    let observed_before_reject = {
        let key = format!("l5:{DID}:SyncPull");
        tracker.count_within_window(&key, now)
    };
    let _ = check_and_record(&tracker, &limits, "SyncPull", DID, now);
    let observed_after_reject = {
        let key = format!("l5:{DID}:SyncPull");
        tracker.count_within_window(&key, now)
    };
    assert_eq!(observed_before_reject, observed_after_reject);
}

#[test]
fn per_did_buckets_are_independent() {
    let tracker = tracker();
    let limits = HandlerRateLimits::defaults();
    let now = Instant::now();

    // Saturate peer A up to the limit.
    for _ in 0..limits.sync_pull {
        assert_eq!(
            check_and_record(&tracker, &limits, "SyncPull", "did:key:peer-a", now),
            HandlerRateOutcome::Accepted,
        );
    }
    assert!(matches!(
        check_and_record(&tracker, &limits, "SyncPull", "did:key:peer-a", now),
        HandlerRateOutcome::Rejected { .. }
    ));

    // Peer B's bucket must be independent and still accept its first
    // request. A flood by A cannot deny service to B.
    assert_eq!(
        check_and_record(&tracker, &limits, "SyncPull", "did:key:peer-b", now),
        HandlerRateOutcome::Accepted,
    );
}

#[test]
fn per_handler_buckets_are_independent() {
    let tracker = tracker();
    let limits = HandlerRateLimits::defaults();
    let now = Instant::now();

    // Same DID: saturating SyncPull must not affect SyncPush budget.
    for _ in 0..limits.sync_pull {
        let _ = check_and_record(&tracker, &limits, "SyncPull", DID, now);
    }
    assert!(matches!(
        check_and_record(&tracker, &limits, "SyncPull", DID, now),
        HandlerRateOutcome::Rejected { .. }
    ));

    assert_eq!(
        check_and_record(&tracker, &limits, "SyncPush", DID, now),
        HandlerRateOutcome::Accepted,
        "SyncPush bucket should not be affected by SyncPull saturation",
    );
}

#[test]
fn bucket_prunes_after_window_and_recovers() {
    let tracker = tracker();
    let limits = HandlerRateLimits::defaults();
    let t0 = Instant::now();

    // Saturate at t0.
    for _ in 0..limits.sync_pull {
        assert_eq!(
            check_and_record(&tracker, &limits, "SyncPull", DID, t0),
            HandlerRateOutcome::Accepted,
        );
    }
    // Reject at t0.
    assert!(matches!(
        check_and_record(&tracker, &limits, "SyncPull", DID, t0),
        HandlerRateOutcome::Rejected { .. }
    ));

    // At t0 + window + 1s, the sliding window has slid past every
    // recorded event → the bucket is effectively empty → the next
    // request must be accepted.
    let t_after = t0 + Duration::from_secs(2);
    assert_eq!(
        check_and_record(&tracker, &limits, "SyncPull", DID, t_after),
        HandlerRateOutcome::Accepted,
    );
}

#[test]
fn reject_logging_is_sampled_to_once_per_window() {
    let tracker = tracker();
    let t0 = Instant::now();

    // First reject in the window writes an audit row; every further reject
    // of the same (DID, handler) is suppressed. Without this the gate is a
    // log-write amplifier against the Owner's own vault.
    assert!(should_log_reject(&tracker, "SyncPull", DID, t0));
    for _ in 0..50 {
        assert!(!should_log_reject(&tracker, "SyncPull", DID, t0));
    }

    // A different handler and a different DID each get their own budget —
    // one flooder must not silence diagnostics for anyone else.
    assert!(should_log_reject(&tracker, "SyncPush", DID, t0));
    assert!(should_log_reject(&tracker, "SyncPull", "did:key:other", t0));

    // Once the window has slid the next reject is logged again.
    assert!(should_log_reject(
        &tracker,
        "SyncPull",
        DID,
        t0 + Duration::from_secs(2)
    ));
}

#[test]
fn log_sampling_does_not_consume_the_enforcement_budget() {
    let tracker = tracker();
    let limits = HandlerRateLimits::defaults();
    let now = Instant::now();

    // `l5log:` is a separate namespace from `l5:`, so asking whether to log
    // must not eat into the handler's request allowance.
    for _ in 0..10 {
        let _ = should_log_reject(&tracker, "SyncPull", DID, now);
    }
    for _ in 0..limits.sync_pull {
        assert_eq!(
            check_and_record(&tracker, &limits, "SyncPull", DID, now),
            HandlerRateOutcome::Accepted,
        );
    }
}
