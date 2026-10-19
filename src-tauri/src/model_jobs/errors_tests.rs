//! Tests for the errors of a model job and the message sanitiser (spec 055 FR-021, FR-011,
//! `contracts/bridge.md` §Fehlercodes).

use serde_json::json;

use super::errors::{bridge_error, refusal, sanitize, too_many, JobErrorCode, JobFailure};
use super::job::JobError;
use super::state::TooManyJobs;
use crate::extensions::error::ExtensionErrorCode;

#[test]
fn every_code_is_written_in_snake_case() {
    for (code, name) in [
        (JobErrorCode::InvalidInput, "invalid_input"),
        (JobErrorCode::LimitExceeded, "limit_exceeded"),
        (JobErrorCode::NoCapableModel, "no_capable_model"),
        (JobErrorCode::ConsentDenied, "consent_denied"),
        (JobErrorCode::Unusable, "unusable"),
        (JobErrorCode::Unavailable, "unavailable"),
        (JobErrorCode::Timeout, "timeout"),
        (JobErrorCode::Cancelled, "cancelled"),
    ] {
        assert_eq!(serde_json::to_value(code).expect("serialize"), name);
        assert_eq!(code.as_str(), name);
    }
}

#[test]
fn a_refused_start_answers_with_the_fixed_codes_and_a_reason() {
    let invalid = refusal(&JobError::Invalid(
        "step read: the instruction is empty".into(),
    ));
    assert_eq!(invalid.code, ExtensionErrorCode::Validation);
    assert_eq!(invalid.code.as_u16(), 3001);
    assert_eq!(invalid.details, Some(json!({"reason": "invalid_input"})));

    for name in [
        "steps",
        "image_count",
        "image_bytes",
        "instruction_bytes",
        "input_bytes",
    ] {
        let limit = refusal(&JobError::Limit(name));
        assert_eq!(limit.code.as_u16(), 7000, "{name}");
        assert_eq!(limit.details, Some(json!({"reason": name, "limit": name})));
    }

    let busy = too_many(TooManyJobs);
    assert_eq!(busy.code.as_u16(), 7000);
    assert_eq!(
        busy.details,
        Some(json!({"reason": "active_jobs", "limit": "active_jobs"}))
    );
}

#[test]
fn a_failure_of_a_started_job_maps_to_the_answer_of_a_call() {
    for (code, number, reason) in [
        (JobErrorCode::NoCapableModel, 8001, "no_capable_model"),
        (JobErrorCode::ConsentDenied, 1002, "consent_denied"),
        (JobErrorCode::InvalidInput, 3001, "invalid_input"),
        (JobErrorCode::LimitExceeded, 7000, "limit_exceeded"),
        (JobErrorCode::Unusable, 8001, "unusable"),
        (JobErrorCode::Unavailable, 8001, "unavailable"),
        (JobErrorCode::Timeout, 8001, "timeout"),
        (JobErrorCode::Cancelled, 8001, "cancelled"),
    ] {
        let answer = bridge_error(code, "no model fits");
        assert_eq!(answer.code.as_u16(), number, "{code:?}");
        assert_eq!(answer.details, Some(json!({"reason": reason})));
    }
}

#[test]
fn the_event_error_has_a_code_a_message_and_perhaps_a_step() {
    let failure = JobFailure::new(JobErrorCode::Unusable, "loop", &[]).at_step("read");
    assert_eq!(
        serde_json::to_value(&failure).expect("serialize"),
        json!({"code": "unusable", "message": "loop", "stepId": "read"})
    );
    let without = JobFailure::new(JobErrorCode::Cancelled, "cancelled", &[]);
    assert_eq!(
        serde_json::to_value(&without).expect("serialize"),
        json!({"code": "cancelled", "message": "cancelled"})
    );
}

#[test]
fn no_failure_carries_an_address_a_key_or_a_value_the_caller_knows_to_be_secret() {
    let key = "school2026";
    let message =
        "POST http://10.1.2.3:8000/v1/chat failed: 401 key school2026 sk-abc123def456ghi789jkl012";
    let failure = JobFailure::new(JobErrorCode::Unavailable, message, &[key, "10.1.2.3"]);
    let shown = serde_json::to_string(&failure).expect("serialize");
    for leak in ["10.1.2.3", "8000", "school2026", "sk-abc123", "http://"] {
        assert!(!shown.contains(leak), "{leak} leaked: {shown}");
    }
}

#[test]
fn a_transport_error_loses_its_url_and_address() {
    let cases = [
        "error sending request for url (http://192.168.1.5:8000/v1/chat/completions): connection refused",
        "POST https://api.example.com/v1/chat failed: timeout",
        "connect to 10.0.0.7 failed",
        "connect to fe80::1 failed",
        "connect to fd12:3456::7 failed",
        "tcp connect error to school-server:11434: refused",
        "GET http://localhost:11434/v1/models: error",
        "connect to gpu-box.lan failed",
        "lookup localhost failed",
    ];
    for message in cases {
        let cleaned = sanitize(message, &[]);
        for leak in [
            "192.168.1.5",
            "8000",
            "api.example.com",
            "10.0.0.7",
            "fe80::1",
            "fd12:3456::7",
            "school-server",
            "11434",
            "localhost",
            "http",
        ] {
            assert!(!cleaned.contains(leak), "{leak} leaked: {cleaned}");
        }
        assert!(cleaned.contains("[address]"), "{cleaned}");
    }
}

#[test]
fn a_key_loses_its_value_and_ordinary_words_stay() {
    let cleaned = sanitize(
        "invalid api key sk-abc123def456ghi789jkl012 for the request",
        &[],
    );
    assert!(!cleaned.contains("sk-abc123"), "{cleaned}");
    assert!(cleaned.contains("[redacted]"));
    assert!(cleaned.contains("invalid api key"));
    assert!(cleaned.ends_with("for the request"));
    // A normal sentence is untouched.
    assert_eq!(
        sanitize("the model gave no usable answer", &[]),
        "the model gave no usable answer"
    );
    // A short word with digits is not a secret.
    assert_eq!(sanitize("step read4 failed", &[]), "step read4 failed");
}

#[test]
fn a_long_message_is_cut_short() {
    let long = "word ".repeat(100);
    let cleaned = sanitize(&long, &[]);
    assert!(
        cleaned.chars().count() <= 201,
        "{}",
        cleaned.chars().count()
    );
    assert!(cleaned.ends_with('…'));
}

#[test]
fn keys_glued_to_a_name_or_quoted_and_known_values_are_redacted() {
    for (message, leak) in [
        ("invalid api_key=sk-proj-abc123def456ghi789", "abc123def456"),
        ("bad \"key\":\"sk-abc123def456ghi789jkl\"", "abc123def456"),
        (
            "secret wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY refused",
            "wJalrXUtnFEMI",
        ),
    ] {
        let cleaned = sanitize(message, &[]);
        assert!(!cleaned.contains(leak), "{leak} leaked: {cleaned}");
    }
    // A value the caller knows is secret goes whatever its shape.
    let cleaned = sanitize("rejected key school2026 at gpu", &["school2026"]);
    assert_eq!(cleaned, "rejected key [redacted] at gpu");
}

#[test]
fn model_names_times_and_counts_stay() {
    for message in [
        "model qwen2.5-vl-7b-instruct not found",
        "timeout after 00:05:00",
        "retry at 12:30",
        "see /v1/chat/completions docs",
    ] {
        assert_eq!(sanitize(message, &[]), message);
    }
}
