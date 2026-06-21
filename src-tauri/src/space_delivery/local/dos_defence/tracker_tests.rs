use std::time::{Duration, Instant};

use super::tracker::RejectRateTracker;

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
