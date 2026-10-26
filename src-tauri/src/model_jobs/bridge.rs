//! The bridge methods of model jobs (spec 055, `contracts/bridge.md`): start, cancel and list. They
//! belong to the calling frame alone. A start answers at once with the job's id after the
//! preflight (`preflight`); the run goes on in the background (`runner`) and ends with the
//! completion event, after which holzi forgets the job.

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{json, Value};
use tokio::time::Instant;
use uuid::Uuid;

use super::candidates::{takes_images, Candidate};
use super::errors::{bridge_error, refusal, too_many, JobErrorCode, JobFailure};
use super::job::{self, JobError};
use super::model_source::ModelSource;
use super::preflight::{preflight, read_facts, reread_facts, still_allowed, Ready};
use super::runner::{run_job, JobEnd, JobRun, Recheck};
use super::state::{JobId, JobOwner, JobRecord, JobRegistry};
use super::trace;
use crate::extensions::bridge::dispatch::{CallContext, Emit};
use crate::extensions::bridge::events::emit_to_frames;
use crate::extensions::error::{BridgeError, ExtensionErrorCode};
use crate::extensions::host::ExtensionHost;
use crate::model_capabilities::ReasoningControl;
use crate::vault_gate::VaultDb;

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
        // `JobFailure` contains only infallibly serializable primitive fields. Keep the invariant
        // explicit instead of silently dropping the required error object on a serialization bug.
        payload["error"] = serde_json::to_value(failure).expect("JobFailure is serializable");
    }
    payload
}

fn emit_completion(emitter: &dyn Emit, host: &ExtensionHost, extension_id: Uuid, payload: &Value) {
    emit_to_frames(emitter, host, extension_id, COMPLETED_EVENT, payload);
}

/// `{job}`: parses and checks the job before any model is asked (a wrong form answers 3001, a
/// limit 7000, a missing grant or consent 1004), registers it and starts the run.
pub(crate) fn start(ctx: &CallContext, params: &Value) -> Result<Value, BridgeError> {
    let started = begin(ctx, params);
    if let Err(error) = &started {
        trace::refused(error);
    }
    started
}

/// Performs the synchronous validation, preflight, and registration before spawning the run.
fn begin(ctx: &CallContext, params: &Value) -> Result<Value, BridgeError> {
    let job = params
        .get("job")
        .ok_or_else(|| refusal(&JobError::Invalid("the call needs a job".into())))?;
    let job = job::parse(job).map_err(|error| refusal(&error))?;
    let ready = preflight(ctx, job)?;
    let registry = ctx.host.model_jobs();
    let record = registry
        .start(owner(ctx), ready.timeout)
        .map_err(too_many)?;
    let id = record.id;
    let run = Run {
        source: ctx.host.model_source(),
        registry,
        host: Arc::clone(&ctx.host),
        emitter: Arc::clone(&ctx.emitter),
        grants: Grants {
            db: ctx.db.clone(),
            host: Arc::clone(&ctx.host),
            extension_id: ctx.session.extension_id,
            device: ctx.device,
        },
    };
    tauri::async_runtime::spawn(async move { run.job(record, ready).await });
    Ok(json!({ "jobId": id.to_string(), "status": "running" }))
}

/// What the background run of a job holds. Its database handle is for the grants between steps;
/// a close of the vault cancels every job first (`instances::close::begin_close`), so a run never
/// keeps the vault open.
struct Run {
    source: Arc<dyn ModelSource>,
    registry: JobRegistry,
    host: Arc<ExtensionHost>,
    emitter: Arc<dyn Emit>,
    grants: Grants,
}

/// The grants and consents of the job's extension as they are now.
struct Grants {
    db: VaultDb,
    host: Arc<ExtensionHost>,
    extension_id: Uuid,
    device: Uuid,
}

#[async_trait]
impl Recheck for Grants {
    async fn still_allowed(&self, planned: &[Candidate]) -> Option<Vec<Candidate>> {
        let facts = reread_facts(&self.db, &self.host, self.extension_id, self.device)
            .await
            .ok()?;
        Some(still_allowed(planned, &facts, self.device))
    }
}

impl Run {
    async fn job(self, record: JobRecord, ready: Ready) {
        let run = run_job(
            self.source.as_ref(),
            &ready.steps,
            &self.grants,
            &record.cancel,
            Instant::from_std(record.deadline),
        )
        .await;
        let secrets: Vec<String> = ready
            .steps
            .iter()
            .flat_map(|(_, candidates)| candidates)
            .flat_map(|c| c.model.secrets())
            .collect();
        let Some(payload) = report(record.id, run, &secrets) else {
            return;
        };
        // Cancelled or over time first: whoever took the record reports the end.
        if self.registry.finish(record.id) {
            emit_completion(
                self.emitter.as_ref(),
                &self.host,
                record.owner.extension_id,
                &payload,
            );
        }
    }
}

/// The completion event of a run; `None` for a cancelled one (the cancel reports it, a closed
/// frame hears nothing).
fn report(id: JobId, run: JobRun, secrets: &[String]) -> Option<Value> {
    let provenance: Vec<Value> = run
        .steps
        .iter()
        .map(|step| {
            json!({
                "stepId": step.step_id,
                "formatName": step.format_name,
                "formatVersion": step.format_version,
                "attempts": step.attempts,
            })
        })
        .collect();
    Some(match run.end {
        JobEnd::Cancelled => return None,
        JobEnd::Completed(results) => {
            let last = results
                .last()
                .map_or(Value::Null, |(_, value)| value.clone());
            let steps: Vec<Value> = results
                .into_iter()
                .map(|(id, result)| json!({ "id": id, "result": result }))
                .collect();
            completion_payload(
                id,
                "completed",
                Value::Array(provenance),
                Some(json!({ "steps": steps, "final": last })),
                None,
            )
        }
        JobEnd::Failed {
            step_id,
            code,
            message,
        } => {
            let known: Vec<&str> = secrets.iter().map(String::as_str).collect();
            let failure = JobFailure::new(code, &message, &known).at_step(&step_id);
            completion_payload(id, "failed", Value::Array(provenance), None, Some(&failure))
        }
    })
}

/// `{jobId}`: cancels a job of the calling frame; `{cancelled: false}` for an unknown id and
/// 1000 for the job of another frame.
pub(crate) fn cancel(ctx: &CallContext, params: &Value) -> Result<Value, BridgeError> {
    let id = params
        .get("jobId")
        .and_then(Value::as_str)
        .and_then(|text| text.parse::<Uuid>().ok())
        .ok_or_else(|| bridge_error(JobErrorCode::InvalidInput, "jobId must be a UUID"))?;
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

/// The models the extension may use, first choice first, without address or key (FR-030). No
/// grant gives an empty list.
pub(crate) fn list(ctx: &CallContext, _params: &Value) -> Result<Value, BridgeError> {
    let models: Vec<Value> = read_facts(ctx)?.granted.iter().map(entry).collect();
    Ok(json!({ "models": models }))
}

/// One granted model as the extension sees it: name, provider, place, capabilities and ceiling.
pub(super) fn entry(c: &Candidate) -> Value {
    let capabilities = c.model.capabilities.as_ref();
    let effort = match capabilities.and_then(|c| c.reasoning.as_ref()) {
        Some(ReasoningControl::Presets { .. }) => "levels",
        Some(ReasoningControl::ModelManaged) => "managed",
        Some(ReasoningControl::Unavailable) | None => "none",
    };
    json!({
        "id": c.model.id,
        "name": c.model.name,
        "provider": c.model.provider.name,
        "place": c.model.place,
        "capabilities": { "image": takes_images(capabilities), "effort": effort },
        "maxEffort": c.ceiling.as_str(),
    })
}
