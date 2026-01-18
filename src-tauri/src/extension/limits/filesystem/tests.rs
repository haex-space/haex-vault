// src-tauri/src/extension/limits/filesystem/tests.rs
//!
//! Tests for filesystem limit enforcement

use super::*;
use crate::extension::limits::types::{FilesystemLimits, LimitError};

#[test]
fn test_file_op_tracker_initial_count() {
    let tracker = FileOpTracker::new();
    assert_eq!(tracker.get_count("ext1"), 0);
}

#[test]
fn test_file_op_tracker_acquire_release() {
    let tracker = FileOpTracker::new();

    let count1 = tracker.acquire("ext1");
    assert_eq!(count1, 1);

    let count2 = tracker.acquire("ext1");
    assert_eq!(count2, 2);

    tracker.release("ext1");
    assert_eq!(tracker.get_count("ext1"), 1);

    tracker.release("ext1");
    assert_eq!(tracker.get_count("ext1"), 0);
}

#[test]
fn test_file_op_guard_raii() {
    let tracker = FileOpTracker::new();

    {
        let _guard = FileOpGuard::new(&tracker, "ext1".to_string());
        assert_eq!(tracker.get_count("ext1"), 1);
    }

    assert_eq!(tracker.get_count("ext1"), 0);
}

#[test]
fn test_validate_file_size_valid() {
    let enforcer = FilesystemLimitEnforcer::new();
    let limits = FilesystemLimits {
        max_storage_bytes: 100 * 1024 * 1024,
        max_file_size_bytes: 10 * 1024 * 1024, // 10MB
        max_concurrent_operations: 10,
        max_operations_per_minute: 120,
    };

    assert!(enforcer.validate_file_size(5 * 1024 * 1024, &limits).is_ok());
}

#[test]
fn test_validate_file_size_at_limit() {
    let enforcer = FilesystemLimitEnforcer::new();
    let limits = FilesystemLimits {
        max_storage_bytes: 100 * 1024 * 1024,
        max_file_size_bytes: 10 * 1024 * 1024,
        max_concurrent_operations: 10,
        max_operations_per_minute: 120,
    };

    assert!(enforcer.validate_file_size(10 * 1024 * 1024, &limits).is_ok());
}

#[test]
fn test_validate_file_size_too_large() {
    let enforcer = FilesystemLimitEnforcer::new();
    let limits = FilesystemLimits {
        max_storage_bytes: 100 * 1024 * 1024,
        max_file_size_bytes: 10 * 1024 * 1024,
        max_concurrent_operations: 10,
        max_operations_per_minute: 120,
    };

    let result = enforcer.validate_file_size(15 * 1024 * 1024, &limits);
    assert!(matches!(result, Err(LimitError::FileTooLarge { .. })));
}

#[test]
fn test_validate_storage_quota_valid() {
    let enforcer = FilesystemLimitEnforcer::new();
    let limits = FilesystemLimits {
        max_storage_bytes: 100 * 1024 * 1024,
        max_file_size_bytes: 50 * 1024 * 1024,
        max_concurrent_operations: 10,
        max_operations_per_minute: 120,
    };

    assert!(enforcer
        .validate_storage_quota(50 * 1024 * 1024, 10 * 1024 * 1024, &limits)
        .is_ok());
}

#[test]
fn test_validate_storage_quota_exceeded() {
    let enforcer = FilesystemLimitEnforcer::new();
    let limits = FilesystemLimits {
        max_storage_bytes: 100 * 1024 * 1024,
        max_file_size_bytes: 50 * 1024 * 1024,
        max_concurrent_operations: 10,
        max_operations_per_minute: 120,
    };

    let result = enforcer.validate_storage_quota(90 * 1024 * 1024, 20 * 1024 * 1024, &limits);
    assert!(matches!(result, Err(LimitError::StorageQuotaExceeded { .. })));
}

#[test]
fn test_acquire_op_slot_success() {
    let enforcer = FilesystemLimitEnforcer::new();
    let limits = FilesystemLimits {
        max_storage_bytes: 100 * 1024 * 1024,
        max_file_size_bytes: 50 * 1024 * 1024,
        max_concurrent_operations: 2,
        max_operations_per_minute: 120,
    };

    let guard1 = enforcer.acquire_op_slot("ext1", &limits);
    assert!(guard1.is_ok());

    let guard2 = enforcer.acquire_op_slot("ext1", &limits);
    assert!(guard2.is_ok());
}

#[test]
fn test_acquire_op_slot_at_limit() {
    let enforcer = FilesystemLimitEnforcer::new();
    let limits = FilesystemLimits {
        max_storage_bytes: 100 * 1024 * 1024,
        max_file_size_bytes: 50 * 1024 * 1024,
        max_concurrent_operations: 2,
        max_operations_per_minute: 120,
    };

    let _guard1 = enforcer.acquire_op_slot("ext1", &limits).unwrap();
    let _guard2 = enforcer.acquire_op_slot("ext1", &limits).unwrap();

    let guard3 = enforcer.acquire_op_slot("ext1", &limits);
    assert!(matches!(
        guard3,
        Err(LimitError::TooManyConcurrentFileOps { current: 2, max: 2 })
    ));
}

#[test]
fn test_acquire_op_slot_after_release() {
    let enforcer = FilesystemLimitEnforcer::new();
    let limits = FilesystemLimits {
        max_storage_bytes: 100 * 1024 * 1024,
        max_file_size_bytes: 50 * 1024 * 1024,
        max_concurrent_operations: 1,
        max_operations_per_minute: 120,
    };

    {
        let _guard1 = enforcer.acquire_op_slot("ext1", &limits).unwrap();
        assert!(enforcer.acquire_op_slot("ext1", &limits).is_err());
    }

    // After guard dropped, should succeed
    let guard2 = enforcer.acquire_op_slot("ext1", &limits);
    assert!(guard2.is_ok());
}
