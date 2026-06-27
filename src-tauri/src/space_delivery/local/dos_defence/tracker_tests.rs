use std::sync::Arc;
use std::time::{Duration, Instant};

use super::tracker::{L1AcceptOutcome, RejectRateTracker};

#[test]
fn records_single_reject_and_reports_count_within_window() {
    let tracker = RejectRateTracker::new(Duration::from_secs(60));
    let now = Instant::now();
    tracker.record("did:key:alice", now);
    assert_eq!(tracker.count_within_window("did:key:alice", now), 1);
}

#[test]
fn aggregates_multiple_records_for_same_key_within_window() {
    let tracker = RejectRateTracker::new(Duration::from_secs(60));
    let start = Instant::now();
    tracker.record("did:key:alice", start);
    tracker.record("did:key:alice", start + Duration::from_millis(500));
    tracker.record("did:key:alice", start + Duration::from_secs(10));
    assert_eq!(
        tracker.count_within_window("did:key:alice", start + Duration::from_secs(15)),
        3
    );
}

#[test]
fn drops_records_outside_sliding_window() {
    let tracker = RejectRateTracker::new(Duration::from_secs(60));
    let start = Instant::now();
    tracker.record("did:key:alice", start);
    let later = start + Duration::from_secs(61);
    assert_eq!(tracker.count_within_window("did:key:alice", later), 0);
}

#[test]
fn keeps_record_at_exact_window_boundary() {
    // Boundary is inclusive: an event at exactly `now - window` is still counted.
    let tracker = RejectRateTracker::new(Duration::from_secs(60));
    let start = Instant::now();
    tracker.record("did:key:alice", start);
    let exactly_at_window = start + Duration::from_secs(60);
    assert_eq!(
        tracker.count_within_window("did:key:alice", exactly_at_window),
        1
    );
}

#[test]
fn counts_are_isolated_per_key() {
    let tracker = RejectRateTracker::new(Duration::from_secs(60));
    let now = Instant::now();
    tracker.record("did:key:alice", now);
    tracker.record("did:key:alice", now);
    tracker.record("did:key:bob", now);
    assert_eq!(tracker.count_within_window("did:key:alice", now), 2);
    assert_eq!(tracker.count_within_window("did:key:bob", now), 1);
    assert_eq!(tracker.count_within_window("did:key:charlie", now), 0);
}

#[test]
fn returns_zero_for_unknown_key() {
    let tracker = RejectRateTracker::new(Duration::from_secs(60));
    let now = Instant::now();
    assert_eq!(tracker.count_within_window("did:key:never-seen", now), 0);
}

#[test]
fn distinct_keys_count_returns_active_keys_in_window() {
    let tracker = RejectRateTracker::new(Duration::from_secs(60));
    let now = Instant::now();
    tracker.record("did:key:alice", now);
    tracker.record("did:key:bob", now);
    tracker.record("did:key:alice", now);
    assert_eq!(tracker.distinct_keys_count(now), 2);
}

#[test]
fn distinct_keys_count_excludes_keys_whose_records_all_expired() {
    let tracker = RejectRateTracker::new(Duration::from_secs(60));
    let start = Instant::now();
    tracker.record("did:key:old", start);
    tracker.record("did:key:current", start + Duration::from_secs(30));
    // Viewed from 70s after start: `old`'s only record is at -70s (outside
    // window), `current`'s record is at -40s (inside).
    let now = start + Duration::from_secs(70);
    assert_eq!(tracker.distinct_keys_count(now), 1);
}

#[test]
fn evicts_keys_with_empty_buckets_to_prevent_unbounded_growth() {
    // Without eviction, a long-lived leader that sees a million one-off DIDs
    // (e.g. a flood with rotating endpoint_ids that all get rejected once)
    // accumulates a million HashMap entries forever — the very DoS the
    // tracker is meant to defend against.
    let tracker = RejectRateTracker::new(Duration::from_secs(60));
    let start = Instant::now();
    tracker.record("did:key:transient", start);
    // After window expiry, querying the key must evict it.
    let later = start + Duration::from_secs(61);
    assert_eq!(tracker.count_within_window("did:key:transient", later), 0);
    assert_eq!(
        tracker.bucket_count(),
        0,
        "empty bucket must be evicted from the map"
    );
}

// =============================================================================
// try_record_l1_accept — atomic check-and-record
// =============================================================================

#[test]
fn l1_accept_records_both_buckets_when_under_limits() {
    let tr = RejectRateTracker::new(Duration::from_secs(1));
    let now = Instant::now();
    assert_eq!(
        tr.try_record_l1_accept("global", 10, "src-A", 5, now),
        L1AcceptOutcome::Accepted
    );
    assert_eq!(tr.count_within_window("global", now), 1);
    assert_eq!(tr.count_within_window("src-A", now), 1);
}

#[test]
fn l1_accept_rejects_global_first_and_records_nothing() {
    let tr = RejectRateTracker::new(Duration::from_secs(1));
    let now = Instant::now();
    // Pre-fill the global bucket to its cap.
    for _ in 0..5 {
        tr.record("global", now);
    }
    assert_eq!(
        tr.try_record_l1_accept("global", 5, "src-A", 5, now),
        L1AcceptOutcome::RejectedGlobal(5)
    );
    // Neither bucket should have been bumped by the rejected accept.
    assert_eq!(tr.count_within_window("global", now), 5);
    assert_eq!(tr.count_within_window("src-A", now), 0);
}

#[test]
fn l1_accept_rejects_per_source_when_global_under_cap() {
    let tr = RejectRateTracker::new(Duration::from_secs(1));
    let now = Instant::now();
    // Global is empty, but src-A is already at its cap.
    for _ in 0..3 {
        tr.record("src-A", now);
    }
    assert_eq!(
        tr.try_record_l1_accept("global", 10, "src-A", 3, now),
        L1AcceptOutcome::RejectedPerSource(3)
    );
    assert_eq!(tr.count_within_window("global", now), 0);
    assert_eq!(tr.count_within_window("src-A", now), 3);
}

#[test]
fn l1_accept_under_concurrency_never_exceeds_global_cap() {
    // The core race CodeRabbit flagged: N threads each see counts below
    // the cap and each record, producing bursts beyond the limit. The
    // atomic check-and-record must let at most `cap` accepts through
    // even under heavy parallel contention.
    const THREADS: usize = 32;
    const GLOBAL_CAP: usize = 10;

    let tr = Arc::new(RejectRateTracker::new(Duration::from_secs(1)));
    let now = Instant::now();

    let handles: Vec<_> = (0..THREADS)
        .map(|i| {
            let tr = tr.clone();
            // Distinct source keys so the per-source cap can't be the
            // gating factor — we're isolating the global-cap race.
            let source = format!("src-{i}");
            std::thread::spawn(move || {
                tr.try_record_l1_accept("global", GLOBAL_CAP, &source, 100, now)
            })
        })
        .collect();

    let outcomes: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();
    let accepted = outcomes
        .iter()
        .filter(|o| matches!(o, L1AcceptOutcome::Accepted))
        .count();

    assert_eq!(
        accepted, GLOBAL_CAP,
        "exactly {GLOBAL_CAP} threads must be accepted; got {accepted}"
    );
    assert_eq!(
        tr.count_within_window("global", now),
        GLOBAL_CAP,
        "global bucket count must equal cap; the missing slots indicate \
         a check-without-record race"
    );
}

#[test]
fn l1_accept_under_concurrency_never_exceeds_per_source_cap() {
    // Same race, scoped to a single per-source bucket.
    const THREADS: usize = 32;
    const SOURCE_CAP: usize = 4;

    let tr = Arc::new(RejectRateTracker::new(Duration::from_secs(1)));
    let now = Instant::now();

    let handles: Vec<_> = (0..THREADS)
        .map(|_| {
            let tr = tr.clone();
            std::thread::spawn(move || {
                tr.try_record_l1_accept("global", 10_000, "src-A", SOURCE_CAP, now)
            })
        })
        .collect();

    let outcomes: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();
    let accepted = outcomes
        .iter()
        .filter(|o| matches!(o, L1AcceptOutcome::Accepted))
        .count();

    assert_eq!(
        accepted, SOURCE_CAP,
        "exactly {SOURCE_CAP} threads must be accepted; got {accepted}"
    );
    assert_eq!(tr.count_within_window("src-A", now), SOURCE_CAP);
}

#[test]
fn distinct_keys_count_evicts_expired_keys_from_storage() {
    // distinct_keys_count walks all buckets — same eviction guarantee.
    let tracker = RejectRateTracker::new(Duration::from_secs(60));
    let start = Instant::now();
    tracker.record("did:key:old", start);
    tracker.record("did:key:current", start + Duration::from_secs(30));
    let later = start + Duration::from_secs(70);
    tracker.distinct_keys_count(later);
    assert_eq!(
        tracker.bucket_count(),
        1,
        "the fully-expired key must be evicted"
    );
}
