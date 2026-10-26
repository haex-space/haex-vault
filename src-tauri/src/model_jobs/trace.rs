//! What a job writes to holzi's log (spec 055 FR-028): which model was tried, with which effort,
//! how the try ended and, for a model that could not be reached, why. The provenance the extension
//! gets names only the category, so the log is where the cause can be read.
//!
//! A line never carries the input, the instruction or the answer of a job, no key or address of
//! the provider (`sanitize`), nothing of the body a provider answered a request with (it can echo
//! the request), and no id the extension chose. Only the message of a failure is kept, cut to
//! one short line.

use super::errors::sanitize;
use super::model_source::JobModel;
use super::runner::Outcome;
use crate::adapters::{AdapterError, StreamError};
use crate::extensions::error::BridgeError;

/// The cause of a request that failed before its stream, without the body of the provider's answer.
pub(super) fn adapter_error(error: &AdapterError) -> String {
    match error {
        AdapterError::Status { status, .. } => format!("provider status {status}"),
        other => other.to_string(),
    }
}

/// The cause of a stream that failed.
pub(super) fn stream_error(error: &StreamError) -> String {
    error.to_string()
}

/// The line for a model that could not give an answer: where it failed and why.
pub(super) fn failure_line(model: &JobModel, stage: &str, detail: &str) -> String {
    let secrets = model.secrets();
    let known: Vec<&str> = secrets.iter().map(String::as_str).collect();
    format!(
        "model job: {} ({:?}) failed at {stage}: {}",
        model.name,
        model.place,
        sanitize(detail, &known)
    )
}

/// A model that could not give an answer: where it failed and why.
pub(super) fn failed(model: &JobModel, stage: &str, detail: &str) {
    log::warn!("{}", failure_line(model, stage, detail));
}

/// How one try of a model ended.
pub(super) fn tried(model: &JobModel, effort: &str, outcome: Outcome) {
    log::info!(
        "model job: {} ({:?}, effort {effort}) ended {outcome:?}",
        model.name,
        model.place
    );
}

/// A job that was refused before any model was asked; the message is the one the extension got.
pub(super) fn refused(error: &BridgeError) {
    log::warn!(
        "model job: start refused with {:?}: {}",
        error.code,
        error.message
    );
}
