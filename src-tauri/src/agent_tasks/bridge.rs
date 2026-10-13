use std::sync::Arc;

use serde_json::{json, Value};
use uuid::Uuid;

use super::{
    definitions::definition_for,
    errors::{TaskError, TaskErrorCode},
    model::{ProcessingMode, TaskInput, TaskProvenance},
    run_fixture, validate_paper_note_result, TaskId, TaskOwner, TaskRunStatus,
};
use crate::extensions::bridge::dispatch::{CallContext, Emit};
use crate::extensions::bridge::events::emit_to_frames;
use crate::extensions::error::{BridgeError, ExtensionErrorCode};

pub(crate) const COMPLETED_EVENT: &str = "haextension:ai-task:completed";

const FIXTURE_PROFILE: &str = "fixture-paper-note";
const FIXTURE_MODEL: &str = "fixture-paper-note-recognition";
const FIXTURE_HARNESS: &str = "fixture";
const MAX_IMAGE_BYTES: usize = 25 * 1024 * 1024;

fn validation(message: impl Into<String>) -> BridgeError {
    BridgeError::new(ExtensionErrorCode::Validation, message)
}

fn task_error(error: TaskError) -> BridgeError {
    let code = match error.code {
        TaskErrorCode::LimitExceeded => ExtensionErrorCode::LimitExceeded,
        TaskErrorCode::InvalidInput | TaskErrorCode::InvalidResult => {
            ExtensionErrorCode::Validation
        }
        TaskErrorCode::Cancelled
        | TaskErrorCode::ConsentRequired
        | TaskErrorCode::ConsentDenied
        | TaskErrorCode::UnsupportedCapability
        | TaskErrorCode::ProfileUnavailable
        | TaskErrorCode::Timeout
        | TaskErrorCode::InferenceFailed => ExtensionErrorCode::NotAvailable,
    };
    BridgeError::new(code, error.message)
}

fn parse_input(params: &Value) -> Result<TaskInput, BridgeError> {
    let input: TaskInput = serde_json::from_value(params.clone())
        .map_err(|_| validation("invalid structured task input"))?;
    let definition =
        definition_for(&input).ok_or_else(|| validation("unsupported task or schema version"))?;
    if !definition.accepts_image {
        return Err(validation("task does not accept image input"));
    }
    if definition.allows_general_tools {
        return Err(validation("task definition may not enable general tools"));
    }
    if input.input.image.bytes.is_empty() {
        return Err(validation("image bytes must not be empty"));
    }
    if input.input.image.bytes.len() > MAX_IMAGE_BYTES {
        return Err(validation("image is too large"));
    }
    if input.input.image.width == 0 || input.input.image.height == 0 {
        return Err(validation("image dimensions must be positive"));
    }
    if !input.input.image.mime_type.starts_with("image/") {
        return Err(validation("mimeType must describe an image"));
    }
    if input.input.locale.trim().is_empty() {
        return Err(validation("locale must not be empty"));
    }
    Ok(input)
}

fn owner(ctx: &CallContext) -> TaskOwner {
    TaskOwner {
        extension_id: ctx.session.extension_id,
        frame: ctx.session.frame.clone(),
    }
}

fn provenance() -> TaskProvenance {
    TaskProvenance {
        task_version: super::TASK_SCHEMA_VERSION,
        profile_id: FIXTURE_PROFILE.to_owned(),
        model_id: FIXTURE_MODEL.to_owned(),
        harness_id: FIXTURE_HARNESS.to_owned(),
        mode: ProcessingMode::Local,
        destination: None,
    }
}

fn completion_payload(
    task_id: TaskId,
    status: &str,
    processing: &TaskProvenance,
    result: Option<Value>,
    error: Option<TaskError>,
) -> Value {
    let mut payload = json!({
        "taskId": task_id.to_string(),
        "status": status,
        "processing": processing,
    });
    if let Some(result) = result {
        payload["result"] = result;
    }
    if let Some(error) = error {
        payload["error"] = serde_json::to_value(error).unwrap_or_else(|_| {
            json!({
                "code": "inference_failed",
                "message": "structured task failed"
            })
        });
    }
    payload
}

fn emit_completion(
    emitter: &dyn Emit,
    host: &crate::extensions::host::ExtensionHost,
    extension_id: Uuid,
    payload: &Value,
) {
    emit_to_frames(emitter, host, extension_id, COMPLETED_EVENT, payload);
}

/// Starts the deterministic first-release task path. Real profile resolution
/// is deliberately added behind the same lifecycle in the provider phase.
pub(crate) fn start(ctx: &CallContext, params: &Value) -> Result<Value, BridgeError> {
    let input = parse_input(params)?;
    let owner = owner(ctx);
    let registry = ctx.host.agent_tasks();
    let record = registry.start(owner.clone()).map_err(task_error)?;
    let task_id = record.id;
    let provenance = provenance();
    let host = Arc::clone(&ctx.host);
    let emitter = Arc::clone(&ctx.emitter);
    let extension_id = owner.extension_id;
    let cancellation = record.cancel.clone();
    let registry_for_task = registry.clone();
    let task_input = input.input;
    let task_provenance = provenance.clone();

    tauri::async_runtime::spawn(async move {
        if cancellation.is_cancelled() {
            return;
        }
        let result = run_fixture(&task_input);
        if cancellation.is_cancelled() {
            return;
        }
        if let Err(error) = validate_paper_note_result(&result) {
            if !registry_for_task.transition(
                task_id,
                TaskRunStatus::Failed,
                Some(task_provenance.clone()),
            ) {
                return;
            }
            emit_completion(
                emitter.as_ref(),
                &host,
                extension_id,
                &completion_payload(task_id, "failed", &task_provenance, None, Some(error)),
            );
            return;
        }
        if !registry_for_task.transition(
            task_id,
            TaskRunStatus::Completed,
            Some(task_provenance.clone()),
        ) {
            return;
        }
        emit_completion(
            emitter.as_ref(),
            &host,
            extension_id,
            &completion_payload(
                task_id,
                "completed",
                &task_provenance,
                Some(serde_json::to_value(result).unwrap_or(Value::Null)),
                None,
            ),
        );
    });

    Ok(json!({
        "taskId": task_id.to_string(),
        "status": "running",
        "processing": provenance,
    }))
}

/// Cancels a task only when the request comes from its originating frame.
pub(crate) fn cancel(ctx: &CallContext, params: &Value) -> Result<Value, BridgeError> {
    let task_id = params
        .get("taskId")
        .and_then(Value::as_str)
        .ok_or_else(|| validation("taskId must be a string"))?
        .parse::<Uuid>()
        .map_err(|_| validation("taskId must be a UUID"))?;
    let owner = owner(ctx);
    let registry = ctx.host.agent_tasks();
    let Some(record) = registry.get(task_id) else {
        return Ok(json!({ "cancelled": false }));
    };
    if record.owner != owner {
        return Err(BridgeError::new(
            ExtensionErrorCode::SecurityViolation,
            "task does not belong to this frame",
        ));
    }
    let cancelled = registry.cancel(task_id, &owner);
    if cancelled {
        let provenance = record.provenance.unwrap_or_else(provenance);
        emit_completion(
            ctx.emitter.as_ref(),
            &ctx.host,
            owner.extension_id,
            &completion_payload(
                task_id,
                "cancelled",
                &provenance,
                None,
                Some(TaskError::new(TaskErrorCode::Cancelled, "task cancelled")),
            ),
        );
    }
    Ok(json!({ "cancelled": cancelled }))
}

#[cfg(test)]
mod tests {
    use super::parse_input;
    use crate::extensions::error::ExtensionErrorCode;
    use serde_json::json;

    #[test]
    fn rejects_non_image_input_before_task_creation() {
        let error = parse_input(&json!({
            "task": "paper-note-recognition",
            "schemaVersion": 1,
            "input": {
                "image": {"bytes": [1], "mimeType": "text/plain", "width": 1, "height": 1},
                "locale": "de-DE"
            }
        }))
        .expect_err("non-image MIME type must be rejected");
        assert_eq!(error.code, ExtensionErrorCode::Validation);
    }
}
