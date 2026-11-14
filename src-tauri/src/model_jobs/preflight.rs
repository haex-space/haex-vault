//! The preflight of a start (spec 055 R4, FR-025): before any model is asked, every step must have
//! a model the extension may use, and the first one of each step outside this device must have the
//! user's consent. Every question to the user is asked here, in the synchronous part, where the
//! 1004 path works; a job never fails half way for a grant that was missing from the start.
//!
//! - A step goes to one model. With a default model it goes to the default and nowhere else: when
//!   the default cannot take the step (it does not fit, the extension may not use it any more, the
//!   vault does not have it) the step starts with `Start::Ask` and the runner asks the user which
//!   other model to use before any request is sent. Without a default nothing was chosen to fall
//!   back from: the first model the extension may use that can take the step goes first. There is
//!   no list of fallbacks either way; when the model fails while the job runs, the runner asks the
//!   user as well.
//! - No granted model fits the step and there is no default: holzi asks for a grant (1004 `model`)
//!   when some model of the vault fits and the user has not refused it, else `consent_denied`
//!   (1002) or `no_capable_model` (8001). The question can make the picked model the default. With
//!   a default the runner asks instead, so no model of the vault fitting is `no_capable_model`.
//! - The model is outside this device without consent: holzi asks (1004 `modelDestination`, with a
//!   summary of what would be sent). A refused destination ends the job with `consent_denied`.

use std::time::Duration;

use serde_json::{json, Value};
use uuid::Uuid;

use super::candidates::Candidate;
use super::candidates::{fits, for_step, granted, job_models, model_permissions, model_request};
use super::destinations::ExtensionModelTransfer;
use super::effort::Level;
use super::errors::{bridge_error, JobErrorCode};
use super::job::{Job, Need, Step};
use super::model_source::JobModel;
use super::runner::{Blocked, Cause, Start};
use crate::error::Result as VaultResult;
use crate::extensions::bridge::dispatch::CallContext;
use crate::extensions::error::{BridgeError, ExtensionErrorCode};
use crate::extensions::host::ExtensionHost;
use crate::extensions::permissions::store;
use crate::extensions::permissions::{
    default_model, evaluate, Action, Decision, Permission, PermissionKind, PermissionRequest,
    RequestTarget,
};
use crate::extensions::sql::exec::limits_of;
use crate::storage::query::Query;
use crate::vault_gate::VaultDb;

/// A job that passed the preflight: each step with its candidates.
pub(crate) struct Ready {
    pub steps: Vec<(Step, Start)>,
    pub timeout: Duration,
}

/// What the preflight reads from the vault.
pub(crate) struct Facts {
    /// Every model the extension may use, the default model first.
    pub granted: Vec<Candidate>,
    /// The id of the extension's default model, whether or not it may be used.
    pub default: Option<String>,
    /// Every model of the vault that can take a job.
    pub models: Vec<JobModel>,
    pub model_permissions: Vec<Permission>,
    pub destinations: Vec<Permission>,
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
        .map_err(|error| {
            log::warn!("model job: the models could not be read: {error}");
            bridge_error(JobErrorCode::Unavailable, "the models could not be read")
        })
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
pub(super) fn destination(candidate: &Candidate) -> PermissionRequest {
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
    Ok(Facts {
        granted: granted(q, extension_id, device, &model_permissions)?,
        default: default_model::get(q, extension_id)?,
        models: job_models(q)?,
        model_permissions,
        destinations,
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

/// Where `step` goes first, or the answer that asks or refuses. With a default model that is the
/// default whenever it can take the step; when it cannot, the user is asked before a request is
/// sent (`Start::Ask`), unless no other model of the vault could take the step either. Without a
/// default it is the first model the extension may use that can.
pub(crate) fn choose(step: &Step, facts: &Facts, device: Uuid) -> Result<Start, BridgeError> {
    let candidate = match &facts.default {
        None => match for_step(&facts.granted, step).into_iter().next() {
            Some(candidate) => candidate,
            None => return Err(ask_for_model(&step.needs, facts, device)),
        },
        Some(default) => {
            let may_use = facts.granted.iter().find(|c| c.model.id == *default);
            match may_use {
                Some(candidate) if fits(&step.needs, &candidate.model) => candidate.clone(),
                _ => return blocked(step, facts, device, default),
            }
        }
    };
    if !candidate.model.place.needs_consent() {
        return Ok(Start::Model(Box::new(candidate)));
    }
    match evaluate(&facts.destinations, &destination(&candidate), device) {
        Decision::Allow => Ok(Start::Model(Box::new(candidate))),
        Decision::Deny => Err(consent_denied()),
        Decision::Prompt => Err(ask_for_destination(step, &candidate)),
    }
}

/// The default model cannot take `step`: the user is asked which other model does, when there is
/// one the user has not refused; else the job cannot be done.
fn blocked(step: &Step, facts: &Facts, device: Uuid, default: &str) -> Result<Start, BridgeError> {
    let others: Vec<&JobModel> = facts
        .models
        .iter()
        .filter(|m| m.id != default && fits(&step.needs, m))
        .collect();
    if others.is_empty() {
        return Err(no_capable_model());
    }
    let open = others.into_iter().any(|m| {
        evaluate(
            &facts.model_permissions,
            &model_request(Level::None, &m.id),
            device,
        ) != Decision::Deny
    });
    if !open {
        return Err(consent_denied());
    }
    let model = facts.models.iter().find(|m| m.id == default);
    let may_use = facts.granted.iter().any(|c| c.model.id == default);
    let (cause, reason) = match model {
        Some(model) if may_use && !fits(&step.needs, model) => (
            Cause::CannotTake,
            format!("it cannot take {}", lacking(&step.needs)),
        ),
        _ => (
            Cause::NotAvailable,
            "the model is not available to the extension".to_owned(),
        ),
    };
    Ok(Start::Ask(Blocked {
        id: default.to_owned(),
        known: model.map(|m| (m.name.clone(), m.place)),
        cause,
        reason,
    }))
}

/// What a model must take for the step, in words for the reason: only images tell models apart.
fn lacking(needs: &[Need]) -> &'static str {
    if needs.contains(&Need::Image) {
        "images"
    } else {
        "what the step needs"
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

/// No granted model fits what a step or a request for access needs: ask for the first one that
/// does.
pub(crate) fn ask_for_model(needs: &[Need], facts: &Facts, device: Uuid) -> BridgeError {
    let fitting: Vec<&JobModel> = facts.models.iter().filter(|m| fits(needs, m)).collect();
    if fitting.is_empty() {
        return no_capable_model();
    }
    let open: Vec<&JobModel> = fitting
        .into_iter()
        .filter(|m| {
            evaluate(
                &facts.model_permissions,
                &model_request(Level::None, &m.id),
                device,
            ) != Decision::Deny
        })
        .collect();
    let Some(first) = open.first() else {
        return consent_denied();
    };
    let target = first;
    let needs: Vec<&str> = needs.iter().map(|need| need.as_str()).collect();
    prompt(json!({
        "resourceType": PermissionKind::Model.as_str(),
        // The user chooses the effort with the model; the question names none.
        "action": Level::None.as_str(),
        "target": target.id,
        "needs": needs,
    }))
}

/// The first candidate leaves the device: ask with what would be sent, never the content.
fn ask_for_destination(step: &Step, candidate: &Candidate) -> BridgeError {
    prompt(json!({
        "resourceType": PermissionKind::ModelDestination.as_str(),
        "action": "send",
        "target": candidate.model.provider.id.to_string(),
        "transfer": ExtensionModelTransfer::of(step),
    }))
}
