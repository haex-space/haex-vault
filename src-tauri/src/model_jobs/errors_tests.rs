//! Tests for the error codes of a model job and the message sanitiser (spec 055 FR-021).

use super::errors::{sanitize, TaskErrorCode};

#[test]
fn the_new_codes_are_named_in_snake_case() {
    for (code, name) in [
        (TaskErrorCode::NoCapableModel, "no_capable_model"),
        (TaskErrorCode::Unusable, "unusable"),
        (TaskErrorCode::Unavailable, "unavailable"),
        (TaskErrorCode::ConsentDenied, "consent_denied"),
    ] {
        assert_eq!(serde_json::to_value(code).expect("serialize"), name);
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
