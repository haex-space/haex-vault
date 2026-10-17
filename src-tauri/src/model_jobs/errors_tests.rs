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
        "tcp connect error to school-server:11434: refused",
        "GET http://localhost:11434/v1/models: error",
    ];
    for message in cases {
        let cleaned = sanitize(message);
        for leak in [
            "192.168.1.5",
            "8000",
            "api.example.com",
            "10.0.0.7",
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
    let cleaned = sanitize("invalid api key sk-abc123def456ghi789jkl012 for the request");
    assert!(!cleaned.contains("sk-abc123"), "{cleaned}");
    assert!(cleaned.contains("[redacted]"));
    assert!(cleaned.contains("invalid api key"));
    assert!(cleaned.ends_with("for the request"));
    // A normal sentence is untouched.
    assert_eq!(
        sanitize("the model gave no usable answer"),
        "the model gave no usable answer"
    );
    // A short word with digits is not a secret.
    assert_eq!(sanitize("step read4 failed"), "step read4 failed");
}

#[test]
fn a_long_message_is_cut_short() {
    let long = "word ".repeat(100);
    let cleaned = sanitize(&long);
    assert!(
        cleaned.chars().count() <= 201,
        "{}",
        cleaned.chars().count()
    );
    assert!(cleaned.ends_with('…'));
}
