//! The native executor of the `syncRules.*` actions for agents (spec 061 US5, FR-035, FR-036,
//! research R11). Every call needs the agent's permission "Sync-Regeln verwalten". Reading the rules
//! and their status runs without a question; creating, changing and deleting a rule waits for the
//! user's confirmation in the window, every time and whatever the approval mode. The rule is checked
//! before the question, so holzi's own places and contacts' shares are refused without asking
//! (FR-002). A confirmed rule runs on its own, with what the user confirmed (FR-036).

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, Manager};
use tokio_util::sync::CancellationToken;

use super::agent_confirm::{
    SyncRuleConfirm, SyncRuleConfirmAction, SyncRuleConfirmRequest, EVENT_SYNC_RULE_CONFIRM,
};
use super::commands::{self, here, others, rule_of, view};
use super::model::{SyncRule, SyncRuleDraft};
use super::rules::{validate, SyncRuleError};
use super::schedule::{Command, Shared};
use super::service::FileSyncState;
use crate::chat::tools::native_action::{
    builtin_only, NativeCall, NativeError, NativeExecutor, NativeReply,
};
use crate::files::local::OwnPlaces;
use crate::files::permissions::{self, BUILTIN_AGENT};
use crate::files::state::FilesState;
use crate::vault_gate::VaultDb;

/// The actions this executor runs.
pub const SYNC_RULE_ACTION_IDS: &[&str] = &[
    "syncRules.list",
    "syncRules.status",
    "syncRules.create",
    "syncRules.update",
    "syncRules.delete",
];

/// The actions that ask the user themselves, so the chat does not ask a second time.
const CONFIRMING_ACTION_IDS: &[&str] =
    &["syncRules.create", "syncRules.update", "syncRules.delete"];

/// What the executor needs from the running app.
#[async_trait]
pub trait SyncRulesEnv: Send + Sync {
    /// The open vault and this device's `vault_device_uuid`.
    async fn vault(&self) -> Result<(VaultDb, String), SyncRuleError>;

    /// holzi's own places, never a place of a rule (FR-002).
    fn own(&self) -> OwnPlaces;

    fn confirm(&self) -> &SyncRuleConfirm;

    /// The running session's view of the rules, for their status; none: every rule idle.
    fn shared(&self) -> Option<Arc<Mutex<Shared>>> {
        None
    }

    /// Stops a run of the rule `id` before it changes or goes.
    async fn stop(&self, _id: &str) {}

    /// Lets the session take up a change and frees the rule [`Self::stop`] held.
    fn reload(&self, _stopped: Option<String>) {}
}

/// The sync rule actions of one agent.
pub struct SyncRulesAgent<E> {
    env: E,
    agent_id: String,
}

impl<E: SyncRulesEnv> SyncRulesAgent<E> {
    /// The executor of the agent of the holzi chat.
    pub fn builtin(env: E) -> Self {
        Self {
            env,
            agent_id: BUILTIN_AGENT.to_owned(),
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct IdInput {
    id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateInput {
    id: String,
    draft: SyncRuleDraft,
}

fn parse<T: DeserializeOwned>(input: Value) -> Result<T, NativeError> {
    serde_json::from_value(input)
        .map_err(|error| NativeError::invalid_input(None, error.to_string()))
}

/// A rule error as the model sees it: `sync_rules_<code>` (`sync_rules_overlap`,
/// `sync_rules_invalid_place`, …) and its reason.
fn rule_error(error: SyncRuleError) -> NativeError {
    let code = serde_json::to_value(error.code)
        .ok()
        .and_then(|value| value.as_str().map(str::to_owned))
        .unwrap_or_default();
    let mut snake = String::from("sync_rules_");
    for c in code.chars() {
        if c.is_ascii_uppercase() {
            snake.push('_');
            snake.push(c.to_ascii_lowercase());
        } else {
            snake.push(c);
        }
    }
    NativeError::new(snake, error.message)
}

fn value_of<T: serde::Serialize>(value: &T) -> Result<Value, NativeError> {
    serde_json::to_value(value).map_err(|_| NativeError::new("failed", "the action failed"))
}

/// What the user set on `rule`, as the question shows it.
fn draft_of(rule: SyncRule) -> SyncRuleDraft {
    SyncRuleDraft {
        name: rule.name,
        place_a: rule.place_a,
        place_b: rule.place_b,
        direction: rule.direction,
        delete_mode: rule.delete_mode,
        interval_secs: rule.interval_secs,
        enabled: rule.enabled,
        allow_decrypted: rule.allow_decrypted,
    }
}

#[async_trait]
impl<E: SyncRulesEnv + 'static> NativeExecutor for SyncRulesAgent<E> {
    fn action_ids(&self) -> &'static [&'static str] {
        SYNC_RULE_ACTION_IDS
    }

    fn confirming_action_ids(&self) -> &'static [&'static str] {
        CONFIRMING_ACTION_IDS
    }

    async fn run(
        &self,
        action_id: &str,
        input: Value,
        call: &NativeCall,
        cancel: &CancellationToken,
    ) -> Result<NativeReply, NativeError> {
        builtin_only(call)?;
        let (db, device) = self.env.vault().await.map_err(rule_error)?;
        let agent = self.agent_id.clone();
        let grants = db
            .read(move |q| Ok(permissions::grants_of(q, &agent)?))
            .await
            .map_err(|_| NativeError::new("failed", "the permissions could not be read"))?;
        if !grants.sync_rules {
            return Err(NativeError::new(
                "sync_rules_not_granted",
                "managing sync rules is not granted to this agent: the user turns on \"Sync-Regeln verwalten\" in its settings",
            ));
        }
        let shared = self.env.shared();
        let result = match action_id {
            "syncRules.list" => {
                let views = commands::list(&db, &device, shared.as_deref())
                    .await
                    .map_err(rule_error)?;
                value_of(&views)
            }
            "syncRules.status" => {
                let input: IdInput = parse(input)?;
                let views = commands::list(&db, &device, shared.as_deref())
                    .await
                    .map_err(rule_error)?;
                let view = views
                    .into_iter()
                    .find(|view| view.rule.id == input.id)
                    .ok_or_else(|| rule_error(commands::not_found()))?;
                value_of(&view)
            }
            "syncRules.create" => {
                let draft: SyncRuleDraft = parse(input)?;
                let own = self.env.own();
                let draft = validate(
                    draft,
                    &others(&db, &device, None).await.map_err(rule_error)?,
                    &own,
                )
                .map_err(rule_error)?;
                self.ask(SyncRuleConfirmAction::Create, draft.clone(), None, cancel)
                    .await?;
                let rule = commands::create(&db, &device, &own, draft)
                    .await
                    .map_err(rule_error)?;
                self.env.reload(None);
                value_of(&view(rule, 0, shared.as_deref()))
            }
            "syncRules.update" => {
                let input: UpdateInput = parse(input)?;
                let before = rule_of(&db, &device, &input.id).await.map_err(rule_error)?;
                let own = self.env.own();
                let draft = validate(
                    input.draft,
                    &others(&db, &device, Some(&input.id))
                        .await
                        .map_err(rule_error)?,
                    &own,
                )
                .map_err(rule_error)?;
                self.ask(
                    SyncRuleConfirmAction::Update,
                    draft.clone(),
                    Some(draft_of(before)),
                    cancel,
                )
                .await?;
                self.env.stop(&input.id).await;
                let result = commands::update(&db, &device, &own, &input.id, draft).await;
                self.env.reload(Some(input.id));
                let rule = result.map_err(rule_error)?;
                value_of(&view(rule, 0, shared.as_deref()))
            }
            "syncRules.delete" => {
                let input: IdInput = parse(input)?;
                let rule = rule_of(&db, &device, &input.id).await.map_err(rule_error)?;
                self.ask(SyncRuleConfirmAction::Delete, draft_of(rule), None, cancel)
                    .await?;
                self.env.stop(&input.id).await;
                let result = commands::delete(&db, &device, &input.id).await;
                self.env.reload(Some(input.id));
                result.map_err(rule_error)?;
                Ok(json!({}))
            }
            other => Err(NativeError::new(
                "unknown_action",
                format!("{other} is no sync rule action"),
            )),
        }?;
        Ok(NativeReply::from(result))
    }
}

impl<E: SyncRulesEnv> SyncRulesAgent<E> {
    /// Waits for the user; anything but "Erlauben" refuses the call.
    async fn ask(
        &self,
        action: SyncRuleConfirmAction,
        rule: SyncRuleDraft,
        before: Option<SyncRuleDraft>,
        cancel: &CancellationToken,
    ) -> Result<(), NativeError> {
        let request = SyncRuleConfirmRequest {
            request_id: String::new(),
            agent_id: self.agent_id.clone(),
            action,
            rule,
            before,
        };
        if self.env.confirm().ask(&request, cancel).await {
            Ok(())
        } else {
            Err(NativeError::new(
                "denied_by_user",
                "the user did not confirm the sync rule",
            ))
        }
    }
}

/// The app's side: the open vault, the session of sync rules and the window's confirmation.
pub struct TauriSyncRulesEnv {
    app: AppHandle,
    confirm: SyncRuleConfirm,
}

impl TauriSyncRulesEnv {
    /// Sends the confirmation to the windows, if there is one (no window: refused).
    pub fn new(app: AppHandle, confirm: SyncRuleConfirm) -> Self {
        let emitting = app.clone();
        confirm.set_emitter(Arc::new(move |request: &SyncRuleConfirmRequest| {
            !emitting.webview_windows().is_empty()
                && emitting.emit(EVENT_SYNC_RULE_CONFIRM, request).is_ok()
        }));
        Self { app, confirm }
    }
}

#[async_trait]
impl SyncRulesEnv for TauriSyncRulesEnv {
    async fn vault(&self) -> Result<(VaultDb, String), SyncRuleError> {
        here(&self.app)
    }

    fn own(&self) -> OwnPlaces {
        self.app.state::<FilesState>().own.clone()
    }

    fn confirm(&self) -> &SyncRuleConfirm {
        &self.confirm
    }

    fn shared(&self) -> Option<Arc<Mutex<Shared>>> {
        self.app
            .state::<FileSyncState>()
            .session()
            .map(|session| Arc::clone(&session.shared))
    }

    async fn stop(&self, id: &str) {
        self.app
            .state::<FileSyncState>()
            .cancel_and_wait(id.to_owned())
            .await;
    }

    fn reload(&self, stopped: Option<String>) {
        let sync = self.app.state::<FileSyncState>();
        sync.send(Command::Reload);
        if let Some(id) = stopped {
            sync.release_cancel(id);
        }
    }
}

#[cfg(test)]
#[path = "agent_tests.rs"]
mod tests;
