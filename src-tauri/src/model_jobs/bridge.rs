//! The bridge methods of model jobs (spec 055, `contracts/bridge.md`): start, cancel and list. They
//! belong to the calling frame alone. The preflight is here; the run of a job and the list of
//! models follow with the stories (`runner`, `candidates`), so until then a job that passes the
//! preflight and the list answer "not available" rather than a job that never ends.

use serde_json::{json, Value};
use uuid::Uuid;

use super::errors::{bridge_error, refusal, JobErrorCode, JobFailure};
use super::job::{self, JobError};
use super::state::{JobId, JobOwner};
use crate::extensions::bridge::dispatch::{CallContext, Emit};
use crate::extensions::bridge::events::emit_to_frames;
use crate::extensions::error::{BridgeError, ExtensionErrorCode};
use crate::extensions::host::ExtensionHost;

pub(crate) const COMPLETED_EVENT: &str = "haextension:model-job:completed";

fn owner(ctx: &CallContext) -> JobOwner {
    JobOwner {
        extension_id: ctx.session.extension_id,
        frame: ctx.session.frame.clone(),
    }
}

/// The data of the completion event (`contracts/bridge.md` §Ereignis). A job that ends without
/// a result has no `result`; `provenance` lists the attempts of each step.
pub(crate) fn completion_payload(
    job_id: JobId,
    status: &str,
    provenance: Value,
    result: Option<Value>,
    failure: Option<&JobFailure>,
) -> Value {
    let mut payload = json!({
        "jobId": job_id.to_string(),
        "status": status,
        "provenance": provenance,
    });
    if let Some(result) = result {
        payload["result"] = result;
    }
    if let Some(failure) = failure {
        payload["error"] = serde_json::to_value(failure).unwrap_or(Value::Null);
    }
    payload
}

fn emit_completion(emitter: &dyn Emit, host: &ExtensionHost, extension_id: Uuid, payload: &Value) {
    emit_to_frames(emitter, host, extension_id, COMPLETED_EVENT, payload);
}

/// `{job}`: parses and checks the job before any model is asked. A form that is wrong answers
/// 3001, a limit 7000.
pub(crate) fn start(_ctx: &CallContext, params: &Value) -> Result<Value, BridgeError> {
    let job = params
        .get("job")
        .ok_or_else(|| refusal(&JobError::Invalid("the call needs a job".into())))?;
    job::parse(job).map_err(|error| refusal(&error))?;
    Err(BridgeError::not_available())
}

/// `{jobId}`: cancels a job of the calling frame; `{cancelled: false}` for an unknown id and
/// 1000 for the job of another frame.
pub(crate) fn cancel(ctx: &CallContext, params: &Value) -> Result<Value, BridgeError> {
    let id = params
        .get("jobId")
        .and_then(Value::as_str)
        .and_then(|text| text.parse::<Uuid>().ok())
        .ok_or_else(|| {
            bridge_error(
                JobErrorCode::InvalidInput,
                "jobId must be a UUID",
                "invalid_input",
            )
        })?;
    let owner = owner(ctx);
    let registry = ctx.host.model_jobs();
    let Some(record) = registry.get(id) else {
        return Ok(json!({ "cancelled": false }));
    };
    if record.owner != owner {
        return Err(BridgeError::new(
            ExtensionErrorCode::SecurityViolation,
            "job does not belong to this frame",
        ));
    }
    let cancelled = registry.cancel(id, &owner);
    if cancelled {
        let failure = JobFailure::new(JobErrorCode::Cancelled, "the job was cancelled", &[]);
        emit_completion(
            ctx.emitter.as_ref(),
            &ctx.host,
            owner.extension_id,
            &completion_payload(id, "cancelled", json!([]), None, Some(&failure)),
        );
    }
    Ok(json!({ "cancelled": cancelled }))
}

/// The models the extension may use. Not yet: they come with the candidates of the first story.
pub(crate) fn list(_ctx: &CallContext, _params: &Value) -> Result<Value, BridgeError> {
    Err(BridgeError::not_available())
}
