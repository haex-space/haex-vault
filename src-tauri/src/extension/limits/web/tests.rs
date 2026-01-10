// src-tauri/src/extension/limits/web/tests.rs
//!
//! Tests for web request limit enforcement

use super::*;
use crate::extension::limits::types::{LimitError, WebLimits};

#[test]
fn test_web_request_tracker_initial_count() {
    let tracker = WebRequestTracker::new();
    assert_eq!(tracker.get_count("ext1"), 0);
}

#[test]
fn test_web_request_tracker_acquire_release() {
    let tracker = WebRequestTracker::new();

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
fn test_web_request_guard_raii() {
    let tracker = WebRequestTracker::new();

    {
        let _guard = WebRequestGuard::new(&tracker, "ext1".to_string());
        assert_eq!(tracker.get_count("ext1"), 1);
    }

    assert_eq!(tracker.get_count("ext1"), 0);
}

#[test]
fn test_check_rate_limit_valid() {
    let enforcer = WebLimitEnforcer::new();
    let limits = WebLimits {
        max_requests_per_minute: 60,
        max_bandwidth_bytes_per_minute: 10 * 1024 * 1024,
        max_concurrent_requests: 5,
    };

    // First request should succeed
    assert!(enforcer.check_rate_limit("ext1", &limits).is_ok());

    // More requests should succeed up to limit
    for _ in 0..58 {
        assert!(enforcer.check_rate_limit("ext1", &limits).is_ok());
    }
}

#[test]
fn test_check_rate_limit_exceeded() {
    let enforcer = WebLimitEnforcer::new();
    let limits = WebLimits {
        max_requests_per_minute: 5,
        max_bandwidth_bytes_per_minute: 10 * 1024 * 1024,
        max_concurrent_requests: 5,
    };

    // Use up all requests
    for _ in 0..5 {
        assert!(enforcer.check_rate_limit("ext1", &limits).is_ok());
    }

    // Next request should fail
    let result = enforcer.check_rate_limit("ext1", &limits);
    assert!(matches!(result, Err(LimitError::RateLimitExceeded { .. })));
}

#[test]
fn test_check_bandwidth_valid() {
    let enforcer = WebLimitEnforcer::new();
    let limits = WebLimits {
        max_requests_per_minute: 60,
        max_bandwidth_bytes_per_minute: 10 * 1024 * 1024, // 10MB
        max_concurrent_requests: 5,
    };

    assert!(enforcer.check_bandwidth("ext1", 1024 * 1024, &limits).is_ok());
    assert!(enforcer.check_bandwidth("ext1", 1024 * 1024, &limits).is_ok());
}

#[test]
fn test_check_bandwidth_exceeded() {
    let enforcer = WebLimitEnforcer::new();
    let limits = WebLimits {
        max_requests_per_minute: 60,
        max_bandwidth_bytes_per_minute: 2 * 1024 * 1024, // 2MB
        max_concurrent_requests: 5,
    };

    // First 1MB should succeed
    assert!(enforcer.check_bandwidth("ext1", 1024 * 1024, &limits).is_ok());

    // Second 1MB should succeed (now at 2MB)
    assert!(enforcer.check_bandwidth("ext1", 1024 * 1024, &limits).is_ok());

    // Third 1MB should fail (would exceed 2MB limit)
    let result = enforcer.check_bandwidth("ext1", 1024 * 1024, &limits);
    assert!(matches!(result, Err(LimitError::BandwidthExceeded { .. })));
}

#[test]
fn test_acquire_request_slot_success() {
    let enforcer = WebLimitEnforcer::new();
    let limits = WebLimits {
        max_requests_per_minute: 60,
        max_bandwidth_bytes_per_minute: 10 * 1024 * 1024,
        max_concurrent_requests: 2,
    };

    let guard1 = enforcer.acquire_request_slot("ext1", &limits);
    assert!(guard1.is_ok());

    let guard2 = enforcer.acquire_request_slot("ext1", &limits);
    assert!(guard2.is_ok());
}

#[test]
fn test_acquire_request_slot_at_limit() {
    let enforcer = WebLimitEnforcer::new();
    let limits = WebLimits {
        max_requests_per_minute: 60,
        max_bandwidth_bytes_per_minute: 10 * 1024 * 1024,
        max_concurrent_requests: 2,
    };

    let _guard1 = enforcer.acquire_request_slot("ext1", &limits).unwrap();
    let _guard2 = enforcer.acquire_request_slot("ext1", &limits).unwrap();

    let guard3 = enforcer.acquire_request_slot("ext1", &limits);
    assert!(matches!(
        guard3,
        Err(LimitError::TooManyConcurrentWebRequests { current: 2, max: 2 })
    ));
}

#[test]
fn test_acquire_request_slot_after_release() {
    let enforcer = WebLimitEnforcer::new();
    let limits = WebLimits {
        max_requests_per_minute: 60,
        max_bandwidth_bytes_per_minute: 10 * 1024 * 1024,
        max_concurrent_requests: 1,
    };

    {
        let _guard1 = enforcer.acquire_request_slot("ext1", &limits).unwrap();
        assert!(enforcer.acquire_request_slot("ext1", &limits).is_err());
    }

    // After guard dropped, should succeed
    let guard2 = enforcer.acquire_request_slot("ext1", &limits);
    assert!(guard2.is_ok());
}

#[test]
fn test_multiple_extensions_independent() {
    let enforcer = WebLimitEnforcer::new();
    let limits = WebLimits {
        max_requests_per_minute: 2,
        max_bandwidth_bytes_per_minute: 10 * 1024 * 1024,
        max_concurrent_requests: 5,
    };

    // Use up all requests for ext1
    enforcer.check_rate_limit("ext1", &limits).unwrap();
    enforcer.check_rate_limit("ext1", &limits).unwrap();
    assert!(enforcer.check_rate_limit("ext1", &limits).is_err());

    // ext2 should still be able to make requests
    assert!(enforcer.check_rate_limit("ext2", &limits).is_ok());
    assert!(enforcer.check_rate_limit("ext2", &limits).is_ok());
}
