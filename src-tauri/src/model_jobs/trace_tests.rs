//! Tests for what a job writes to the log (spec 055 FR-028): the cause of a failure is kept, a
//! provider's key and address and the body it answered with are not.

use super::model_source::JobModel;
use super::test_support::{model_row, provider};
use super::trace::{adapter_error, failure_line, model_label, refused_line, stream_error};
use crate::adapters::{AdapterError, StreamError};
use crate::extensions::error::{BridgeError, ExtensionErrorCode};
use crate::storage::providers::ProviderKind;
use serde_json::json;

#[test]
fn a_status_names_the_number_and_never_the_body_that_may_echo_the_request() {
    let error = AdapterError::Status {
        status: 400,
        body: "echoed: Das ist meine Mitschrift".into(),
    };
    let line = adapter_error(&error);
    assert_eq!(line, "provider status 400");
}

#[test]
fn a_failure_keeps_its_cause() {
    let line = adapter_error(&AdapterError::Unavailable {
        reason: "claude is not installed".into(),
    });
    assert!(line.contains("claude is not installed"), "{line}");
    let line = stream_error(&StreamError::Model("unknown model".into()));
    assert!(line.contains("unknown model"), "{line}");
}

#[test]
fn the_log_line_of_a_failure_loses_the_key_and_the_address_of_the_provider() {
    let server = provider(
        ProviderKind::ApiKey,
        Some("https://ai.school.example/v1"),
        Some("sk-school-123456"),
    );
    let model = JobModel::new(
        model_row(
            &server,
            "https://ai.school.example sk-school-123456\nmodel",
            None,
        ),
        server,
    );

    let line = failure_line(
        &model,
        "start",
        "connect to https://ai.school.example/v1 with sk-school-123456 failed",
    );

    assert!(line.contains("failed at start"), "{line}");
    assert!(!line.contains("sk-school-123456"), "{line}");
    assert!(!line.contains("ai.school.example"), "{line}");
    assert_eq!(model_label(&model), "[address] [redacted] model");
}

#[test]
fn a_refused_start_log_does_not_repeat_untrusted_validation_text() {
    let error = BridgeError::new(
        ExtensionErrorCode::Validation,
        "step secret-id: instruction contains the student's note",
    )
    .with_details(json!({ "reason": "invalid_input" }));

    assert_eq!(
        refused_line(&error),
        "model job: start refused with Validation: invalid_input"
    );
}
