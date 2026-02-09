// src-tauri/src/extension/limits/database/tests.rs
//!
//! Tests for database limit enforcement

use super::*;
use crate::extension::limits::database::enforcer::{ConcurrencyTracker, QueryGuard};
use crate::extension::limits::types::{DatabaseLimits, LimitError};

#[test]
fn test_concurrency_tracker_initial_count() {
    let tracker = ConcurrencyTracker::new();
    assert_eq!(tracker.get_count("ext1"), 0);
}

#[test]
fn test_concurrency_tracker_acquire() {
    let tracker = ConcurrencyTracker::new();

    let count1 = tracker.acquire("ext1");
    assert_eq!(count1, 1);
    assert_eq!(tracker.get_count("ext1"), 1);

    let count2 = tracker.acquire("ext1");
    assert_eq!(count2, 2);
    assert_eq!(tracker.get_count("ext1"), 2);
}

#[test]
fn test_concurrency_tracker_multiple_extensions() {
    let tracker = ConcurrencyTracker::new();

    tracker.acquire("ext1");
    tracker.acquire("ext1");
    let count_ext2 = tracker.acquire("ext2");

    assert_eq!(tracker.get_count("ext1"), 2);
    assert_eq!(count_ext2, 1);
    assert_eq!(tracker.get_count("ext2"), 1);
}

#[test]
fn test_concurrency_tracker_release() {
    let tracker = ConcurrencyTracker::new();

    tracker.acquire("ext1");
    tracker.acquire("ext1");
    assert_eq!(tracker.get_count("ext1"), 2);

    tracker.release("ext1");
    assert_eq!(tracker.get_count("ext1"), 1);

    tracker.release("ext1");
    assert_eq!(tracker.get_count("ext1"), 0);
}

#[test]
fn test_concurrency_tracker_release_nonexistent() {
    let tracker = ConcurrencyTracker::new();
    // Should not panic
    tracker.release("nonexistent");
}

#[test]
fn test_query_guard_raii() {
    let tracker = ConcurrencyTracker::new();

    {
        let _guard = QueryGuard::new(&tracker, "ext1".to_string());
        assert_eq!(tracker.get_count("ext1"), 1);

        {
            let _guard2 = QueryGuard::new(&tracker, "ext1".to_string());
            assert_eq!(tracker.get_count("ext1"), 2);
        }

        // guard2 dropped
        assert_eq!(tracker.get_count("ext1"), 1);
    }

    // guard dropped
    assert_eq!(tracker.get_count("ext1"), 0);
}

#[test]
fn test_query_guard_multiple_extensions() {
    let tracker = ConcurrencyTracker::new();

    let _guard1 = QueryGuard::new(&tracker, "ext1".to_string());
    let _guard2 = QueryGuard::new(&tracker, "ext2".to_string());

    assert_eq!(tracker.get_count("ext1"), 1);
    assert_eq!(tracker.get_count("ext2"), 1);

    drop(_guard1);
    assert_eq!(tracker.get_count("ext1"), 0);
    assert_eq!(tracker.get_count("ext2"), 1);
}

#[test]
fn test_validate_query_size_valid() {
    let enforcer = DatabaseLimitEnforcer::new();
    let limits = DatabaseLimits {
        query_timeout_ms: 30_000,
        max_result_rows: 10_000,
        max_concurrent_queries: 5,
        max_query_size_bytes: 100,
    };

    let small_sql = "SELECT * FROM users";
    assert!(enforcer.validate_query_size(small_sql, &limits).is_ok());
}

#[test]
fn test_validate_query_size_at_limit() {
    let enforcer = DatabaseLimitEnforcer::new();
    let limits = DatabaseLimits {
        query_timeout_ms: 30_000,
        max_result_rows: 10_000,
        max_concurrent_queries: 5,
        max_query_size_bytes: 20,
    };

    let sql = "SELECT * FROM users"; // 19 chars
    assert!(enforcer.validate_query_size(sql, &limits).is_ok());
}

#[test]
fn test_validate_query_size_too_large() {
    let enforcer = DatabaseLimitEnforcer::new();
    let limits = DatabaseLimits {
        query_timeout_ms: 30_000,
        max_result_rows: 10_000,
        max_concurrent_queries: 5,
        max_query_size_bytes: 100,
    };

    let large_sql = "x".repeat(150);
    let result = enforcer.validate_query_size(&large_sql, &limits);
    assert!(matches!(
        result,
        Err(LimitError::QueryTooLarge {
            size: 150,
            max_size: 100
        })
    ));
}

#[test]
fn test_validate_result_rows_valid() {
    let enforcer = DatabaseLimitEnforcer::new();
    let limits = DatabaseLimits {
        query_timeout_ms: 30_000,
        max_result_rows: 100,
        max_concurrent_queries: 5,
        max_query_size_bytes: 1_000_000,
    };

    assert!(enforcer.validate_result_rows(50, &limits).is_ok());
}

#[test]
fn test_validate_result_rows_at_limit() {
    let enforcer = DatabaseLimitEnforcer::new();
    let limits = DatabaseLimits {
        query_timeout_ms: 30_000,
        max_result_rows: 100,
        max_concurrent_queries: 5,
        max_query_size_bytes: 1_000_000,
    };

    assert!(enforcer.validate_result_rows(100, &limits).is_ok());
}

#[test]
fn test_validate_result_rows_over_limit() {
    let enforcer = DatabaseLimitEnforcer::new();
    let limits = DatabaseLimits {
        query_timeout_ms: 30_000,
        max_result_rows: 100,
        max_concurrent_queries: 5,
        max_query_size_bytes: 1_000_000,
    };

    let result = enforcer.validate_result_rows(150, &limits);
    assert!(matches!(
        result,
        Err(LimitError::ResultTooLarge {
            rows: 150,
            max_rows: 100
        })
    ));
}

#[test]
fn test_acquire_query_slot_success() {
    let enforcer = DatabaseLimitEnforcer::new();
    let limits = DatabaseLimits {
        query_timeout_ms: 30_000,
        max_result_rows: 10_000,
        max_concurrent_queries: 2,
        max_query_size_bytes: 1_000_000,
    };

    let guard1 = enforcer.acquire_query_slot("ext1", &limits);
    assert!(guard1.is_ok());

    let guard2 = enforcer.acquire_query_slot("ext1", &limits);
    assert!(guard2.is_ok());
}

#[test]
fn test_acquire_query_slot_at_limit() {
    let enforcer = DatabaseLimitEnforcer::new();
    let limits = DatabaseLimits {
        query_timeout_ms: 30_000,
        max_result_rows: 10_000,
        max_concurrent_queries: 2,
        max_query_size_bytes: 1_000_000,
    };

    let _guard1 = enforcer.acquire_query_slot("ext1", &limits).unwrap();
    let _guard2 = enforcer.acquire_query_slot("ext1", &limits).unwrap();

    // Third slot should fail (limit is 2)
    let guard3 = enforcer.acquire_query_slot("ext1", &limits);
    assert!(matches!(
        guard3,
        Err(LimitError::TooManyConcurrentQueries { current: 2, max: 2 })
    ));
}

#[test]
fn test_acquire_query_slot_different_extensions() {
    let enforcer = DatabaseLimitEnforcer::new();
    let limits = DatabaseLimits {
        query_timeout_ms: 30_000,
        max_result_rows: 10_000,
        max_concurrent_queries: 1,
        max_query_size_bytes: 1_000_000,
    };

    let _guard1 = enforcer.acquire_query_slot("ext1", &limits).unwrap();

    // Different extension should succeed even though ext1 is at limit
    let guard2 = enforcer.acquire_query_slot("ext2", &limits);
    assert!(guard2.is_ok());
}

#[test]
fn test_acquire_query_slot_after_release() {
    let enforcer = DatabaseLimitEnforcer::new();
    let limits = DatabaseLimits {
        query_timeout_ms: 30_000,
        max_result_rows: 10_000,
        max_concurrent_queries: 1,
        max_query_size_bytes: 1_000_000,
    };

    {
        let _guard1 = enforcer.acquire_query_slot("ext1", &limits).unwrap();
        // At this point, limit is reached
        assert!(enforcer.acquire_query_slot("ext1", &limits).is_err());
    }
    // guard1 dropped, slot released

    // Now should be able to acquire again
    let guard2 = enforcer.acquire_query_slot("ext1", &limits);
    assert!(guard2.is_ok());
}

#[test]
fn test_enforcer_concurrency_reference() {
    let enforcer = DatabaseLimitEnforcer::new();

    // Use the concurrency tracker directly through the reference
    enforcer.concurrency().acquire("ext1");
    assert_eq!(enforcer.concurrency().get_count("ext1"), 1);

    enforcer.concurrency().release("ext1");
    assert_eq!(enforcer.concurrency().get_count("ext1"), 0);
}
