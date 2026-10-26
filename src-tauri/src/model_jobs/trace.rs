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
use crate::extensions::error::BridgeError;
use serde_json::Value;

/// The model name as one redacted log-safe label.
pub(super) fn model_label(model: &JobModel) -> String {
    let secrets = model.secrets();
    let known: Vec<&str> = secrets.iter().map(String::as_str).collect();
    sanitize(&model.name, &known)
}

/// The line for a model that could not give an answer: where it failed and why.
pub(super) fn failure_line(model: &JobModel, stage: &str, detail: &str) -> String {
    let secrets = model.secrets();
    let known: Vec<&str> = secrets.iter().map(String::as_str).collect();
    format!(
        "model job: {} ({:?}) failed at {stage}: {}",
        model_label(model),
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
        model_label(model),
        model.place
    );
}

/// A refusal line that keeps only the stable reason, never untrusted validation text.
pub(super) fn refused_line(error: &BridgeError) -> String {
    let reason = error
        .details
        .as_ref()
        .and_then(|details| details.get("reason"))
        .and_then(Value::as_str)
        .unwrap_or("refused");
    format!("model job: start refused with {:?}: {reason}", error.code)
}

/// A job that was refused before any model was asked; only its stable reason is logged.
pub(super) fn refused(error: &BridgeError) {
    log::warn!("{}", refused_line(error));
}
