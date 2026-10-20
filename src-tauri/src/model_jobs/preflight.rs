//! The preflight of a start (spec 055 R4, FR-025): before any model is asked, every step must have
//! a model the extension may use, and the first one of each step outside this device must have the
//! user's consent. Every question to the user is asked here, in the synchronous part, where the
//! 1004 path works; a job never fails half way for a grant that was missing from the start.
//!
//! - No granted model fits the step: holzi asks for a grant (1004 `model`) when some model of the
//!   vault fits and the user has not refused it, else `consent_denied` (1002) or
//!   `no_capable_model` (8001).
//! - The first candidate outside this device without consent: holzi asks (1004
//!   `modelDestination`, with a summary of what would be sent). One the user refused is dropped.
//!   A later candidate without consent is dropped and never tried (FR-027).

use std::time::Duration;

use serde_json::{json, Value};
use uuid::Uuid;

use super::candidates::Candidate;
use super::candidates::{fits, for_step, granted, job_models, model_permissions, model_request};
use super::effort::Level;
use super::errors::{bridge_error, JobErrorCode};
use super::job::{Job, Step};
use super::model_source::JobModel;
use crate::chat::default_model::PREF_DEFAULT_MODEL;
use crate::error::Result as VaultResult;
use crate::extensions::bridge::dispatch::CallContext;
use crate::extensions::error::{BridgeError, ExtensionErrorCode};
use crate::extensions::host::ExtensionHost;
use crate::extensions::permissions::store;
use crate::extensions::permissions::{
    evaluate, Action, Decision, Permission, PermissionKind, PermissionRequest, RequestTarget,
};
use crate::extensions::sql::exec::limits_of;
use crate::storage::preferences::{self, PrefScope};
use crate::storage::query::Query;
use crate::vault_gate::VaultDb;

/// A job that passed the preflight: each step with its candidates.
pub(crate) struct Ready {
    pub steps: Vec<(Step, Vec<Candidate>)>,
    pub timeout: Duration,
}

/// What the preflight reads from the vault.
pub(crate) struct Facts {
    /// Every model the extension may use, first choice first.
    pub granted: Vec<Candidate>,
    /// Every model of the vault that can take a job.
    pub models: Vec<JobModel>,
    pub model_permissions: Vec<Permission>,
    pub destinations: Vec<Permission>,
    /// The user's default model: this device's, else the vault's.
    pub default_model: Option<String>,
    pub timeout: Duration,
}

/// The model and destination decisions held for the session.
fn held(host: &ExtensionHost, extension_id: Uuid) -> (Vec<Permission>, Vec<Permission>) {
    let permissions = &host.permissions;
    (
        permissions.temporary(extension_id, PermissionKind::Model),
        permissions.temporary(extension_id, PermissionKind::ModelDestination),
    )
}

/// Reads the [`Facts`] of the calling extension, with the decisions held for the session.
pub(crate) fn read_facts(ctx: &CallContext) -> Result<Facts, BridgeError> {
    let (extension_id, device) = (ctx.session.extension_id, ctx.device);
    let (held, destinations) = held(&ctx.host, extension_id);
    ctx.db
        .read_blocking(move |q| {
            facts_of(q, extension_id, device, held, destinations).map_err(Into::into)
        })
        .map_err(|_| bridge_error(JobErrorCode::Unavailable, "the models could not be read"))
}

/// [`read_facts`] for a running job, between two of its steps.
pub(crate) async fn reread_facts(
    db: &VaultDb,
    host: &ExtensionHost,
    extension_id: Uuid,
    device: Uuid,
) -> VaultResult<Facts> {
    let (held, destinations) = held(host, extension_id);
    db.read(move |q| facts_of(q, extension_id, device, held, destinations).map_err(Into::into))
        .await
}

/// The candidates of a later step that may still be tried: the ones planned at the start that
/// the extension may still use (with the ceiling as it is now) and, outside this device, whose
/// destination still has the user's consent. A model granted since the start is not added: the
/// user was asked about the job as it started (FR-025).
pub(crate) fn still_allowed(planned: &[Candidate], facts: &Facts, device: Uuid) -> Vec<Candidate> {
    planned
        .iter()
        .filter_map(|candidate| {
            let now = facts
                .granted
                .iter()
                .find(|granted| granted.model.id == candidate.model.id)?;
            let consented = !candidate.model.place.needs_consent()
                || evaluate(&facts.destinations, &destination(candidate), device)
                    == Decision::Allow;
            consented.then(|| Candidate {
                model: candidate.model.clone(),
                ceiling: now.ceiling,
            })
        })
        .collect()
}

/// The question whether the job may be sent to the provider of `candidate`.
fn destination(candidate: &Candidate) -> PermissionRequest {
    PermissionRequest {
        kind: PermissionKind::ModelDestination,
        action: Action::Send,
        target: RequestTarget::Provider(candidate.model.provider.id.to_string()),
    }
}

fn facts_of(
    q: &mut impl Query,
    extension_id: Uuid,
    device: Uuid,
    held: Vec<Permission>,
    mut destinations: Vec<Permission>,
) -> VaultResult<Facts> {
    let model_permissions = model_permissions(q, extension_id, device, held)?;
    destinations.extend(store::candidates(
        q,
        extension_id,
        PermissionKind::ModelDestination,
        device,
    )?);
    let default_model = match preferences::get(q, PrefScope::Device(device), PREF_DEFAULT_MODEL)? {
        Some(id) if !id.is_empty() => Some(id),
        _ => preferences::get(q, PrefScope::Vault, PREF_DEFAULT_MODEL)?.filter(|id| !id.is_empty()),
    };
    Ok(Facts {
        granted: granted(q, extension_id, device, &model_permissions)?,
        models: job_models(q)?,
        model_permissions,
        destinations,
        default_model,
        timeout: Duration::from_millis(limits_of(q, extension_id)?.model_job_timeout_ms),
    })
}

/// Checks a parsed job and finds the candidates of every step, in the order of the steps.
pub(crate) fn preflight(ctx: &CallContext, job: Job) -> Result<Ready, BridgeError> {
    let facts = read_facts(ctx)?;
    let steps = job
        .steps
        .into_iter()
        .map(|step| {
            let candidates = choose(&step, &facts, ctx.device)?;
            Ok((step, candidates))
        })
        .collect::<Result<_, BridgeError>>()?;
    Ok(Ready {
        steps,
        timeout: facts.timeout,
    })
}

/// The candidates of `step` that may be tried, or the answer that asks or refuses.
pub(crate) fn choose(
    step: &Step,
    facts: &Facts,
    device: Uuid,
) -> Result<Vec<Candidate>, BridgeError> {
    let listed = for_step(&facts.granted, step);
    if listed.is_empty() {
        return Err(ask_for_model(step, facts, device));
    }
    let mut kept = Vec::new();
    let mut refused = false;
    for candidate in listed {
        if !candidate.model.place.needs_consent() {
            kept.push(candidate);
            continue;
        }
        match evaluate(&facts.destinations, &destination(&candidate), device) {
            Decision::Allow => kept.push(candidate),
            Decision::Deny => refused = true,
            Decision::Prompt if kept.is_empty() => {
                return Err(ask_for_destination(step, &candidate))
            }
            // A later candidate without consent is never tried.
            Decision::Prompt => {}
        }
    }
    if !kept.is_empty() {
        Ok(kept)
    } else if refused {
        Err(consent_denied())
    } else {
        Err(no_capable_model())
    }
}

fn consent_denied() -> BridgeError {
    bridge_error(JobErrorCode::ConsentDenied, "the user refused")
}

fn no_capable_model() -> BridgeError {
    bridge_error(
        JobErrorCode::NoCapableModel,
        "no model the extension may use can do what the step needs",
    )
}

fn prompt(details: Value) -> BridgeError {
    BridgeError::new(
        ExtensionErrorCode::PermissionPromptRequired,
        "permission required",
    )
    .with_details(details)
}

/// No granted model fits: ask for one that does, the default model when it fits.
fn ask_for_model(step: &Step, facts: &Facts, device: Uuid) -> BridgeError {
    let wish = step.effort.unwrap_or(Level::None);
    let fitting: Vec<&JobModel> = facts
        .models
        .iter()
        .filter(|m| fits(&step.needs, m.capabilities.as_ref()))
        .collect();
    if fitting.is_empty() {
        return no_capable_model();
    }
    let open: Vec<&JobModel> = fitting
        .into_iter()
        .filter(|m| {
            evaluate(
                &facts.model_permissions,
                &model_request(wish, &m.id),
                device,
            ) != Decision::Deny
        })
        .collect();
    let Some(first) = open.first() else {
        return consent_denied();
    };
    let target = facts
        .default_model
        .as_deref()
        .and_then(|id| open.iter().find(|m| m.id == id))
        .unwrap_or(first);
    let needs: Vec<&str> = step.needs.iter().map(|need| need.as_str()).collect();
    prompt(json!({
        "resourceType": PermissionKind::Model.as_str(),
        "action": wish.as_str(),
        "target": target.id,
        "needs": needs,
    }))
}

/// The first candidate leaves the device: ask with what would be sent, never the content.
fn ask_for_destination(step: &Step, candidate: &Candidate) -> BridgeError {
    let images = &step.input.images;
    let mut kinds = vec!["text"];
    if !images.is_empty() {
        kinds.push("image");
    }
    let text_bytes = step.instruction.len() + step.input.text.as_ref().map_or(0, String::len);
    prompt(json!({
        "resourceType": PermissionKind::ModelDestination.as_str(),
        "action": "send",
        "target": candidate.model.provider.id.to_string(),
        "transfer": {
            "kinds": kinds,
            "textBytes": text_bytes,
            "imageCount": images.len(),
            "imageBytes": images.iter().map(|i| i.bytes.len()).sum::<usize>(),
        },
    }))
}
