//! The confirmation before an agent creates, changes or deletes a sync rule (spec 061 FR-035,
//! research R11): the rule goes to the window as `sync-rule-confirm-request` with its places,
//! direction, delete mode and interval, and waits for `sync_rule_confirm_answer`. Every time,
//! without "remember" and whatever the approval mode; no window, no answer in [`CONFIRM_TIMEOUT`]
//! or a cancelled turn refuse it.

use std::sync::Arc;
use std::time::Duration;

use serde::Serialize;
use tauri::State;
use tokio_util::sync::CancellationToken;
use ts_rs::TS;

use super::model::SyncRuleDraft;
use crate::prompt_park::PromptPark;

/// Event the confirmation goes out on (contracts/tauri-commands.md).
pub const EVENT_SYNC_RULE_CONFIRM: &str = "sync-rule-confirm-request";

/// How long the confirmation waits for the user.
pub const CONFIRM_TIMEOUT: Duration = Duration::from_secs(120);

/// What the agent wants to do with the rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub enum SyncRuleConfirmAction {
    Create,
    Update,
    Delete,
}

/// The payload of [`EVENT_SYNC_RULE_CONFIRM`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct SyncRuleConfirmRequest {
    pub request_id: String,
    pub agent_id: String,
    pub action: SyncRuleConfirmAction,
    /// The rule as it would be stored; for `delete` the rule that goes.
    pub rule: SyncRuleDraft,
    /// For `update`: the rule as it is now.
    pub before: Option<SyncRuleDraft>,
}

/// Delivers a confirmation to the window; `false` when no window could hear it.
pub type ConfirmEmitter = Arc<dyn Fn(&SyncRuleConfirmRequest) -> bool + Send + Sync>;

/// Shared by the executor and the answer command; a Tauri managed state. Cheap to clone.
#[derive(Clone)]
pub struct SyncRuleConfirm {
    park: PromptPark<SyncRuleConfirmRequest, bool>,
}

impl Default for SyncRuleConfirm {
    fn default() -> Self {
        Self::with_timeout(CONFIRM_TIMEOUT)
    }
}

impl SyncRuleConfirm {
    pub fn with_timeout(timeout: Duration) -> Self {
        Self {
            park: PromptPark::new(timeout),
        }
    }

    /// Set from `setup()`; the request goes out with the id it is parked under.
    pub fn set_emitter(&self, emitter: ConfirmEmitter) {
        self.park.set_emitter(Arc::new(
            move |id: &str, request: &SyncRuleConfirmRequest| {
                emitter(&SyncRuleConfirmRequest {
                    request_id: id.to_owned(),
                    ..request.clone()
                })
            },
        ));
    }

    /// Asks and waits; only an explicit "Erlauben" confirms.
    pub async fn ask(&self, request: &SyncRuleConfirmRequest, cancel: &CancellationToken) -> bool {
        self.park
            .ask(request, cancel, |_| {})
            .await
            .unwrap_or(false)
    }

    /// The window's answer; `false` for an unknown or late id.
    pub fn answer(&self, request_id: &str, confirmed: bool) -> bool {
        self.park.answer(request_id, confirmed)
    }
}

/// The answer to `sync-rule-confirm-request`. A late or unknown id is not an error.
#[tauri::command]
pub fn sync_rule_confirm_answer(
    confirm: State<'_, SyncRuleConfirm>,
    request_id: String,
    confirmed: bool,
) {
    confirm.answer(&request_id, confirmed);
}
