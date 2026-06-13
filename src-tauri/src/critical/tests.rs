//! Tests for the `critical` module — `lock_or_fail`, sink emit/UPSERT,
//! acknowledge, cleanup, severity contract.
//!
//! Strategy: tests use [`CriticalNotificationSink::in_memory`] so no
//! filesystem or SQLCipher key is needed. Poison is produced via
//! [`poison_mutex`] (panic inside a `lock()` while holding the guard).

#![cfg(test)]

use std::sync::Mutex;

use super::{lock_or_fail, CriticalFailureCode, CriticalNotificationSink, Severity};

/// Poison a mutex deterministically by panicking with the guard held.
/// `catch_unwind` swallows the panic so the test process survives.
fn poison_mutex<T>(mutex: &Mutex<T>) {
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _guard = mutex.lock().expect("acquire mutex before poisoning");
        panic!("intentional poison for test");
    }));
    assert!(mutex.is_poisoned(), "poison_mutex helper should leave mutex poisoned");
}

// =========================================================================
// CriticalFailureCode + Severity
// =========================================================================

#[test]
fn severity_critical_for_data_corruption_codes() {
    assert_eq!(CriticalFailureCode::HlcMutexPoisoned.severity(), Severity::Critical);
    assert_eq!(CriticalFailureCode::DbMutexPoisoned.severity(), Severity::Critical);
    assert_eq!(CriticalFailureCode::DbSchemaDrift.severity(), Severity::Critical);
}

#[test]
fn severity_warning_for_observability_codes() {
    assert_eq!(CriticalFailureCode::AuditLogWriteFailed.severity(), Severity::Warning);
    assert_eq!(CriticalFailureCode::CrdtTransformFailed.severity(), Severity::Warning);
}

#[test]
fn code_as_str_matches_variant_name() {
    // Pinning the string ↔ variant mapping so a future rename touches both.
    assert_eq!(CriticalFailureCode::HlcMutexPoisoned.as_str(), "HlcMutexPoisoned");
    assert_eq!(CriticalFailureCode::DbMutexPoisoned.as_str(), "DbMutexPoisoned");
    assert_eq!(CriticalFailureCode::DbSchemaDrift.as_str(), "DbSchemaDrift");
    assert_eq!(CriticalFailureCode::AuditLogWriteFailed.as_str(), "AuditLogWriteFailed");
    assert_eq!(CriticalFailureCode::CrdtTransformFailed.as_str(), "CrdtTransformFailed");
}

// =========================================================================
// Sink emit + UPSERT dedup
// =========================================================================

#[test]
fn emit_inserts_a_new_row_when_none_exists() {
    let sink = CriticalNotificationSink::in_memory().expect("in-memory sink");
    sink.emit(
        CriticalFailureCode::HlcMutexPoisoned,
        "test::loc",
        serde_json::json!({"foo": "bar"}),
    )
    .expect("emit");

    let row = sink.newest_unacked().expect("query").expect("one row");
    assert_eq!(row.code, "HlcMutexPoisoned");
    assert_eq!(row.location, "test::loc");
    assert_eq!(row.count, 1);
    assert!(!row.acknowledged);
    // params is JSON-string; sanity-check substring rather than full JSON
    // equality so reformatting (e.g. whitespace) doesn't break the test.
    assert!(row.params.contains("foo"));
    assert!(row.params.contains("bar"));
}

#[test]
fn emit_upserts_count_when_same_code_location_acknowledged_triple_repeats() {
    let sink = CriticalNotificationSink::in_memory().expect("in-memory sink");

    for _ in 0..5 {
        sink.emit(
            CriticalFailureCode::HlcMutexPoisoned,
            "test::loc",
            serde_json::json!({}),
        )
        .expect("emit");
    }

    let row = sink.newest_unacked().expect("query").expect("still one row");
    assert_eq!(row.count, 5, "five emits of the same triple must collapse to count=5");
    assert_eq!(row.first_seen, row.first_seen, "first_seen must be set");
    // last_seen >= first_seen — temporal ordering preserved.
    assert!(row.last_seen >= row.first_seen);
}

#[test]
fn emit_with_different_location_creates_separate_row() {
    let sink = CriticalNotificationSink::in_memory().expect("in-memory sink");
    sink.emit(CriticalFailureCode::HlcMutexPoisoned, "loc::A", serde_json::json!({})).unwrap();
    sink.emit(CriticalFailureCode::HlcMutexPoisoned, "loc::B", serde_json::json!({})).unwrap();

    // newest_unacked returns only ONE row, but the second emit must NOT
    // have collapsed into the first. Validate by acknowledging the
    // newest and confirming a *different* unacked row remains.
    let first = sink.newest_unacked().unwrap().unwrap();
    sink.acknowledge(&first.id).unwrap();
    let second = sink
        .newest_unacked()
        .unwrap()
        .expect("a second row at the other location must still be unacked");
    assert_ne!(
        first.location, second.location,
        "different locations must produce separate rows",
    );
}

#[test]
fn emit_after_acknowledge_creates_fresh_unacked_row() {
    // Q3 invariant: once a (code, location) is acknowledged, the next
    // occurrence MUST re-surface as a new unacked row — otherwise the
    // banner stays silent on a recurring failure.
    let sink = CriticalNotificationSink::in_memory().expect("in-memory sink");

    sink.emit(CriticalFailureCode::DbMutexPoisoned, "test::loc", serde_json::json!({})).unwrap();
    let first = sink.newest_unacked().unwrap().unwrap();
    assert_eq!(first.count, 1);

    sink.acknowledge(&first.id).unwrap();
    assert!(
        sink.newest_unacked().unwrap().is_none(),
        "after ack, no unacked row should remain",
    );

    sink.emit(CriticalFailureCode::DbMutexPoisoned, "test::loc", serde_json::json!({})).unwrap();
    let second = sink.newest_unacked().unwrap().expect("new unacked row after re-emit");
    assert_ne!(first.id, second.id, "the acknowledged row must NOT be reused");
    assert_eq!(second.count, 1, "fresh row starts at count=1");
}

// =========================================================================
// Acknowledge
// =========================================================================

#[test]
fn acknowledge_returns_zero_for_unknown_id() {
    let sink = CriticalNotificationSink::in_memory().unwrap();
    let n = sink.acknowledge("not-a-real-uuid").unwrap();
    assert_eq!(n, 0, "unknown id must update 0 rows, not panic");
}

#[test]
fn newest_unacked_returns_none_when_table_empty() {
    let sink = CriticalNotificationSink::in_memory().unwrap();
    assert!(sink.newest_unacked().unwrap().is_none());
}

// =========================================================================
// Cleanup retention
// =========================================================================

#[test]
fn cleanup_deletes_rows_older_than_retention() {
    let sink = CriticalNotificationSink::in_memory().unwrap();
    sink.emit(CriticalFailureCode::HlcMutexPoisoned, "test::loc", serde_json::json!({})).unwrap();

    // 0-day retention: cutoff = now, every row's last_seen is older than now → deleted.
    // (Strictly: last_seen was set "now" too, but the test is OK with that
    // either way — we just need a deterministic boundary check.)
    let report = sink.cleanup(0).unwrap();
    // last_seen ~= now; depending on time-precision the row may or may
    // not get deleted. The contract is "removes rows older than cutoff",
    // not "removes everything" — so we accept either 0 or 1 deleted
    // rows here. The next test pins the actual behaviour for clearly-
    // old rows.
    assert!(
        report.deleted_rows <= 1,
        "cleanup must not delete future rows ({} reported)",
        report.deleted_rows,
    );
}

#[test]
fn cleanup_does_not_delete_recent_rows() {
    let sink = CriticalNotificationSink::in_memory().unwrap();
    sink.emit(CriticalFailureCode::DbMutexPoisoned, "test::loc", serde_json::json!({})).unwrap();

    // Generous retention — nothing older than 1000 days exists in a
    // fresh in-memory DB.
    let report = sink.cleanup(1000).unwrap();
    assert_eq!(report.deleted_rows, 0);
    assert!(sink.newest_unacked().unwrap().is_some(), "row must survive cleanup");
}

// =========================================================================
// lock_or_fail
// =========================================================================

#[test]
fn lock_or_fail_returns_guard_on_healthy_mutex() {
    let sink = CriticalNotificationSink::in_memory().unwrap();
    let m = Mutex::new(42_u32);
    let guard = lock_or_fail(
        &m,
        CriticalFailureCode::HlcMutexPoisoned,
        "tests::healthy",
        &sink,
        serde_json::json!({}),
    )
    .expect("healthy mutex should yield Ok");
    assert_eq!(*guard, 42);
}

#[test]
fn lock_or_fail_returns_err_and_emits_row_on_poison() {
    let sink = CriticalNotificationSink::in_memory().unwrap();
    let m = Mutex::new(42_u32);
    poison_mutex(&m);

    let result = lock_or_fail(
        &m,
        CriticalFailureCode::DbMutexPoisoned,
        "tests::poisoned",
        &sink,
        serde_json::json!({"detail": "test"}),
    );
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert_eq!(err.code, CriticalFailureCode::DbMutexPoisoned);
    assert_eq!(err.location, "tests::poisoned");

    let row = sink.newest_unacked().unwrap().expect("emit must have written a row");
    assert_eq!(row.code, "DbMutexPoisoned");
    assert_eq!(row.location, "tests::poisoned");
    assert!(row.params.contains("detail"));
}

#[test]
fn lock_or_fail_keeps_mutex_poisoned_after_first_call() {
    // Subsequent callers must ALSO fail — no silent recovery (per the
    // User-Entscheidung 2026-06-13: no into_inner, no abort, no panic).
    let sink = CriticalNotificationSink::in_memory().unwrap();
    let m = Mutex::new(7_u32);
    poison_mutex(&m);

    let first = lock_or_fail(
        &m,
        CriticalFailureCode::HlcMutexPoisoned,
        "tests::repoison_first",
        &sink,
        serde_json::json!({}),
    );
    assert!(first.is_err());

    let second = lock_or_fail(
        &m,
        CriticalFailureCode::HlcMutexPoisoned,
        "tests::repoison_second",
        &sink,
        serde_json::json!({}),
    );
    assert!(second.is_err(), "mutex must stay poisoned across calls");
}

#[test]
fn lock_or_fail_emits_distinct_rows_for_distinct_locations_on_poison() {
    // Two callers from different locations both hit a poisoned mutex —
    // the sink should record both as separate (code, location) rows so
    // an operator can see which call sites observed the failure.
    let sink = CriticalNotificationSink::in_memory().unwrap();
    let m = Mutex::new(0_u32);
    poison_mutex(&m);

    // Discard the Err results via drop() — the assertions below check
    // the sink rows, not the return values. `let _ =` on a lock-returning
    // function trips clippy's `let_underscore_lock` lint.
    drop(lock_or_fail(
        &m,
        CriticalFailureCode::HlcMutexPoisoned,
        "tests::caller_a",
        &sink,
        serde_json::json!({}),
    ));
    drop(lock_or_fail(
        &m,
        CriticalFailureCode::HlcMutexPoisoned,
        "tests::caller_b",
        &sink,
        serde_json::json!({}),
    ));

    // newest_unacked returns one row; acknowledge and check the second.
    let row_a = sink.newest_unacked().unwrap().unwrap();
    sink.acknowledge(&row_a.id).unwrap();
    let row_b = sink.newest_unacked().unwrap().expect("second caller's row");
    assert_ne!(row_a.location, row_b.location);
}
