//! The question when the model of a step could not take it (spec 055, changes of 2026-11-13 and
//! 2026-11-14): a job goes to one model, the extension's default or, without one, the first it may
//! use. holzi never tries another model on its own. When the model fails while the job runs (an
//! unusable answer, no connection), or cannot be asked at all (the default cannot take the step or
//! is no longer available, which is found before any request is sent), the job waits and the
//! window asks the user which other model should take the step, and whether that model becomes the
//! extension's default. The wait does not count against the job's time (`runner::run_step`).
//!
//! The question goes out as [`REPLACEMENT_EVENT`] and is answered by `model_replacement_answer`.
//! Sending to a model outside this device is the user's consent when the question names it
//! (`needs_consent`): choosing it remembers the consent for its provider, as the question of the
//! destination does (`preflight`). No window, no answer in [`REPLACEMENT_TIMEOUT`], a refusal or a
//! cancelled job end the step with the failure of the first model.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio::sync::oneshot;
use tokio_util::sync::CancellationToken;
use ts_rs::TS;
use uuid::Uuid;

use super::candidates::model_request;
use super::candidates::{fits, Candidate};
use super::destinations::{host_of, ExtensionModelTransfer};
use super::effort::Level;
use super::grants::{self, ExtensionModelChoice};
use super::job::Step;
use super::model_source::JobModel;
use super::preflight::{destination, reread_facts};
use super::runner::{Failed, FailedKind, Replace};
use crate::extensions::bridge::dispatch::Emit;
use crate::extensions::host::ExtensionHost;
use crate::extensions::permissions::store::{self, NewPermission};
use crate::extensions::permissions::{
    default_model, evaluate, Decision, PermissionKind, PermissionStatus,
};
use crate::passwords::clock::unix_millis;
use crate::storage::query::Query;
use crate::vault_gate::VaultDb;

/// Event the question goes out on.
pub const REPLACEMENT_EVENT: &str = "model-replacement-request";

/// Event that says a question is moot: the job stopped waiting (cancelled, or no answer in time).
pub const REPLACEMENT_ENDED_EVENT: &str = "model-replacement-ended";

/// How long the question waits for the user.
pub const REPLACEMENT_TIMEOUT: Duration = Duration::from_secs(300);

/// One model the question offers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct ModelReplacementChoice {
    pub id: String,
    pub name: String,
    pub provider: String,
    /// `this_device`, `network` or `cloud`.
    pub place: String,
    pub takes_images: bool,
    /// A subscription: an agent with tools of its own, which the choice says to the user.
    pub agent: bool,
    /// The efforts the model offers, lowest first; empty when it has no choice of effort.
    pub efforts: Vec<String>,
    /// The host of the provider's address with the port, when the job would leave this device.
    #[ts(optional)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,
    /// The job would leave this device and the user has not agreed to that provider yet: choosing
    /// the model agrees to it.
    pub needs_consent: bool,
}

/// The question: which model takes the step that `failed_model` could not take.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct ModelReplacementRequest {
    pub request_id: String,
    pub extension_id: String,
    pub display_name: String,
    /// Empty when the vault does not know the model any more.
    pub failed_model: String,
    /// The model that failed is the extension's default.
    pub failed_was_default: bool,
    /// `unusable` or `unavailable` while the job ran; `cannot_take` or `not_available` when the
    /// model was not asked: it cannot take the step, or the extension may not use it (any more).
    pub outcome: String,
    /// Why a model that was asked failed, with the secrets of the provider taken out. Not set for
    /// `cannot_take` and `not_available`: `outcome` says it and the window words it in the user's
    /// language.
    #[ts(optional)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// The models that can do the step, the ones the extension may use first.
    pub models: Vec<ModelReplacementChoice>,
    /// What the step would send, for a model outside this device.
    pub transfer: ExtensionModelTransfer,
}

/// The user's answer: the model, the highest effort it may use and whether it becomes the default.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct ModelReplacementAnswer {
    pub model_id: String,
    pub effort: String,
    pub make_default: bool,
}

type Pending = HashMap<String, oneshot::Sender<Option<ModelReplacementAnswer>>>;

/// The open questions of the jobs of every extension; one of the host.
#[derive(Default)]
pub struct ModelReplacements {
    pending: Mutex<Pending>,
    /// How long a question waits, in milliseconds; 0 is [`REPLACEMENT_TIMEOUT`].
    timeout_ms: AtomicU64,
}

impl ModelReplacements {
    fn timeout(&self) -> Duration {
        match self.timeout_ms.load(Ordering::Relaxed) {
            0 => REPLACEMENT_TIMEOUT,
            ms => Duration::from_millis(ms),
        }
    }

    /// Lets a test end an unanswered question soon.
    #[cfg(test)]
    pub(crate) fn set_timeout(&self, timeout: Duration) {
        let ms = u64::try_from(timeout.as_millis())
            .unwrap_or(u64::MAX)
            .max(1);
        self.timeout_ms.store(ms, Ordering::Relaxed);
    }

    fn pending(&self) -> MutexGuard<'_, Pending> {
        self.pending.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Asks the window and waits; `None` for a refusal, no answer in time or a cancelled job.
    async fn ask(
        &self,
        emitter: &dyn Emit,
        mut request: ModelReplacementRequest,
        cancel: &CancellationToken,
    ) -> Option<ModelReplacementAnswer> {
        let id = Uuid::new_v4().to_string();
        request.request_id = id.clone();
        // Serialised first: a failure here must not leave an open question behind.
        let payload = serde_json::to_value(&request).ok()?;
        let (sender, receiver) = oneshot::channel();
        self.pending().insert(id.clone(), sender);
        emitter.emit(REPLACEMENT_EVENT, payload);
        let answer = tokio::select! {
            biased;
            () = cancel.cancelled() => None,
            answer = receiver => answer.ok().flatten(),
            () = tokio::time::sleep(self.timeout()) => None,
        };
        self.pending().remove(&id);
        if answer.is_none() {
            // The window may still show the question; a late answer would do nothing.
            emitter.emit(
                REPLACEMENT_ENDED_EVENT,
                serde_json::json!({ "requestId": id }),
            );
        }
        answer
    }

    /// The window's answer; `false` for an unknown or late id.
    pub fn answer(&self, request_id: &str, answer: Option<ModelReplacementAnswer>) -> bool {
        self.pending()
            .remove(request_id)
            .is_some_and(|sender| sender.send(answer).is_ok())
    }
}

/// Asks the user for the model that takes a step whose model failed, for one running job.
pub(crate) struct Replacer {
    pub db: VaultDb,
    pub host: Arc<ExtensionHost>,
    pub emitter: Arc<dyn Emit>,
    pub extension_id: Uuid,
    pub device: Uuid,
}

fn choice_of(model: &JobModel, needs_consent: bool) -> ModelReplacementChoice {
    let choice: ExtensionModelChoice = grants::choice_of(model);
    ModelReplacementChoice {
        id: choice.id,
        name: choice.name,
        provider: choice.provider,
        place: choice.place,
        takes_images: choice.takes_images,
        agent: choice.agent,
        efforts: choice.efforts,
        host: model
            .place
            .needs_consent()
            .then(|| model.provider.base_url.as_deref().and_then(host_of))
            .flatten(),
        needs_consent,
    }
}

/// What the question needs from the vault.
struct Offer {
    choices: Vec<ModelReplacementChoice>,
    display_name: String,
    failed_was_default: bool,
}

impl Replacer {
    /// The models the question offers: those that can do the step, were not tried and the user has
    /// not refused, the ones the extension may use first. `None` when there is none.
    async fn offer(&self, step: &Step, failed: &Failed, tried: &[String]) -> Option<Offer> {
        let facts = reread_facts(&self.db, &self.host, self.extension_id, self.device)
            .await
            .ok()?;
        let mut open: Vec<&JobModel> = facts
            .models
            .iter()
            .filter(|m| fits(&step.needs, m) && !tried.contains(&m.id))
            .filter(|m| {
                evaluate(
                    &facts.model_permissions,
                    &model_request(Level::None, &m.id),
                    self.device,
                ) != Decision::Deny
            })
            .collect();
        if open.is_empty() {
            return None;
        }
        let allowed = |id: &str| facts.granted.iter().any(|c| c.model.id == id);
        open.sort_by(|a, b| {
            (!allowed(&a.id), &a.name, &a.id).cmp(&(!allowed(&b.id), &b.name, &b.id))
        });
        // A provider the user refused to send jobs to is not offered.
        let choices: Vec<ModelReplacementChoice> = open
            .into_iter()
            .filter_map(|model| {
                let candidate = Candidate {
                    model: model.clone(),
                    ceiling: Level::None,
                };
                let decision = if model.place.needs_consent() {
                    evaluate(&facts.destinations, &destination(&candidate), self.device)
                } else {
                    Decision::Allow
                };
                (decision != Decision::Deny).then(|| choice_of(model, decision != Decision::Allow))
            })
            .collect();
        if choices.is_empty() {
            return None;
        }
        let extension_id = self.extension_id;
        let failed_id = failed.model_id.clone();
        let (display_name, failed_was_default) = self
            .db
            .read(move |q| {
                let name = q
                    .query_row(
                        "SELECT coalesce(display_name, name) FROM extensions WHERE id = ?1 \
                         UNION ALL SELECT coalesce(display_name, name) \
                         FROM dev_extensions_no_sync WHERE id = ?1",
                        &[&extension_id.to_string()],
                        |r| r.get::<_, String>(0),
                    )?
                    .unwrap_or_default();
                let default = default_model::get(q, extension_id)
                    .map_err(|e| haex_crdt::Error::consumer(e.to_string()))?;
                Ok((name, default.as_deref() == Some(failed_id.as_str())))
            })
            .await
            .ok()?;
        Some(Offer {
            choices,
            display_name,
            failed_was_default,
        })
    }

    /// Remembers the user's choice: the model may be used up to the effort, the provider may get
    /// the job when it needed the user's consent, and the model is the default when asked.
    async fn remember(&self, answer: &ModelReplacementAnswer, consent_for: Option<Uuid>) -> bool {
        let Some(effort) = Level::parse(&answer.effort) else {
            return false;
        };
        let (extension_id, device) = (self.extension_id, self.device);
        let (model_id, make_default) = (answer.model_id.clone(), answer.make_default);
        let now = unix_millis(std::time::SystemTime::now());
        let written = self
            .db
            .write({
                let model_id = model_id.clone();
                move |tx| {
                    grants::put(
                        tx,
                        extension_id,
                        &model_id,
                        PermissionStatus::Granted,
                        effort,
                        now,
                    )
                    .map_err(|e| haex_crdt::Error::consumer(e.to_string()))?;
                    if let Some(provider) = consent_for {
                        let target = provider.to_string();
                        store::put(
                            tx,
                            extension_id,
                            &NewPermission {
                                kind: PermissionKind::ModelDestination.as_str(),
                                action: "send",
                                target: &target,
                                status: PermissionStatus::Granted.as_str(),
                                declared: false,
                                vault_device_uuid: PermissionKind::ModelDestination
                                    .scope_on(device),
                            },
                            now,
                        )
                        .map_err(|e| haex_crdt::Error::consumer(e.to_string()))?;
                    }
                    if make_default {
                        grants::make_default(tx, extension_id, &model_id)
                            .map_err(|e| haex_crdt::Error::consumer(e.to_string()))?;
                    }
                    Ok(())
                }
            })
            .await
            .is_ok();
        if written {
            grants::forget_held(&self.host, extension_id, &model_id);
        }
        written
    }
}

#[async_trait]
impl Replace for Replacer {
    async fn replacement(
        &self,
        step: &Step,
        failed: &Failed,
        tried: &[String],
        cancel: &CancellationToken,
    ) -> Option<Candidate> {
        let offer = self.offer(step, failed, tried).await?;
        let request = ModelReplacementRequest {
            request_id: String::new(),
            extension_id: self.extension_id.to_string(),
            display_name: offer.display_name,
            failed_model: failed.model_name.clone(),
            failed_was_default: offer.failed_was_default,
            outcome: failed.kind.as_str().to_owned(),
            reason: match failed.kind {
                FailedKind::Unusable | FailedKind::Unavailable => failed.reason.clone(),
                FailedKind::CannotTake | FailedKind::NotAvailable => None,
            },
            models: offer.choices.clone(),
            transfer: ExtensionModelTransfer::of(step),
        };
        let answer = self
            .host
            .replacements
            .ask(self.emitter.as_ref(), request, cancel)
            .await?;
        let chosen = offer.choices.iter().find(|c| c.id == answer.model_id)?;
        let effort = Level::parse(&answer.effort)?;
        let facts = reread_facts(&self.db, &self.host, self.extension_id, self.device)
            .await
            .ok()?;
        let model = facts.models.into_iter().find(|m| m.id == chosen.id)?;
        // The consent is judged again on the vault as it is now: a provider refused, or one that
        // needs consent the question did not name, while the question was open, is not used.
        let decision = if model.place.needs_consent() {
            let candidate = Candidate {
                model: model.clone(),
                ceiling: effort,
            };
            evaluate(&facts.destinations, &destination(&candidate), self.device)
        } else {
            Decision::Allow
        };
        match decision {
            Decision::Deny => return None,
            Decision::Prompt if !chosen.needs_consent => return None,
            Decision::Allow | Decision::Prompt => {}
        }
        let consent_for = (decision == Decision::Prompt).then_some(model.provider.id);
        if !self.remember(&answer, consent_for).await {
            return None;
        }
        Some(Candidate {
            model,
            ceiling: effort,
        })
    }
}

/// The answer to [`REPLACEMENT_EVENT`]: the model to use, or `None` to decline. A late or unknown
/// id is not an error.
#[tauri::command]
pub fn model_replacement_answer(
    state: tauri::State<'_, crate::state::AppState>,
    request_id: String,
    answer: Option<ModelReplacementAnswer>,
) {
    state.extensions().replacements.answer(&request_id, answer);
}

#[cfg(test)]
#[path = "replacement_tests.rs"]
mod tests;
