//! Event names and payload structs for the chat streaming/tool-loop/model-
//! load surface, plus the small helpers tied to them (`risk_class_str`,
//! `strip_leaked_tool_call_markup`) and the three `emit_*` functions.
//!
//! Split out of `chat/commands.rs` (2026-09-15 review) — first of five
//! steps, since every other piece being split out of that file emits one
//! of these events. See `chat/commands.rs`'s own history for the rest of
//! the split plan.

use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, Emitter};
use uuid::Uuid;

use crate::storage::chat_messages::FinishReason;

use super::model_loading::LoadPhase;
use super::session::ChatState;
use super::tools::ToolOrigin;

pub(crate) const EVENT_CHAT_TOKEN: &str = "chat-token";
pub(crate) const EVENT_CHAT_MESSAGE_COMPLETE: &str = "chat-message-complete";
pub(crate) const EVENT_CHAT_MESSAGE_ERROR: &str = "chat-message-error";
const EVENT_MODEL_LOAD_PROGRESS: &str = "model-load-progress";
const EVENT_MODEL_LOAD_STATUS: &str = "model-load-status";
pub(crate) const EVENT_CHAT_TOOL_CALL: &str = "chat-tool-call";
pub(crate) const EVENT_CHAT_TOOL_RESULT: &str = "chat-tool-result";
pub(crate) const EVENT_CHAT_TURN_COMPLETE: &str = "chat-turn-complete";
pub(crate) const EVENT_TOOL_PERMISSION_REQUEST: &str = "tool-permission-request";
/// A question the tool round asks the user (spec 046), answered via `respond_choice`.
pub(crate) const EVENT_CHOICE_REQUEST: &str = "chat-choice-request";
pub(crate) const EVENT_CHAT_RETRY: &str = "chat-retry";
/// Once per turn: whether the model got tools (spec 032 US4, `ToolAvailabilityEvent`).
pub(crate) const EVENT_CHAT_TOOL_AVAILABILITY: &str = "chat-tool-availability";
/// After the probe or the self-test wrote what a model can do with tools (spec 032).
pub(crate) const EVENT_MODEL_TOOL_USE_UPDATED: &str = "model-tool-use-updated";
const EVENT_MODEL_LOAD_ERROR: &str = "model-load-error";
pub(crate) const EVENT_CHAT_AGENT_ACTIVITY: &str = "chat-agent-activity";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ModelLoadProgress {
    #[serde(rename = "vaultGeneration")]
    vault_generation: u64,
    #[serde(rename = "loadId")]
    load_id: u64,
    model_id: String,
    model_name: String,
    phase: LoadPhase,
    #[serde(skip_serializing_if = "Option::is_none")]
    provider_name: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ModelLoadErrorEvent {
    vault_generation: u64,
    load_id: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    model_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    model_name: Option<String>,
    code: String,
}

/// Emits a `model-load-progress` event for the given phase. Frontend
/// translates the label via `$t('chat.loading.<phase>', ...)` and
/// gates the chat input on `phase === 'ready'`.
pub(crate) fn emit_load_progress(
    app: &AppHandle,
    vault_generation: u64,
    load_id: u64,
    model_id: &str,
    model_name: &str,
    phase: LoadPhase,
    provider_name: Option<String>,
) {
    let _ = app.emit(
        EVENT_MODEL_LOAD_PROGRESS,
        ModelLoadProgress {
            vault_generation,
            load_id,
            model_id: model_id.to_string(),
            model_name: model_name.to_string(),
            phase,
            provider_name,
        },
    );
}

pub(crate) fn emit_load_error(
    app: &AppHandle,
    vault_generation: u64,
    load_id: u64,
    model_id: Option<String>,
    model_name: Option<String>,
    code: impl Into<String>,
) {
    let _ = app.emit(
        EVENT_MODEL_LOAD_ERROR,
        ModelLoadErrorEvent {
            vault_generation,
            load_id,
            model_id,
            model_name,
            code: code.into(),
        },
    );
}

/// Publishes the authoritative model-load snapshot after lifecycle changes
/// such as cancellation, unload, or a Vault transition.
pub(crate) fn emit_model_load_status(app: &AppHandle, chat: &ChatState) {
    let _ = app.emit(EVENT_MODEL_LOAD_STATUS, chat.model_load_status());
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TokenEvent {
    pub(crate) message_id: Uuid,
    pub(crate) delta: String,
    /// Reasoning-content delta from Harmony-format local models or
    /// Anthropic `thinking_delta` events. `None` when the chunk has
    /// no reasoning.
    pub(crate) reasoning: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MessageCompleteEvent {
    pub(crate) message_id: Uuid,
    pub(crate) thread_id: Uuid,
    pub(crate) prompt_tokens: Option<usize>,
    pub(crate) completion_tokens: Option<usize>,
    pub(crate) ttft_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MessageErrorEvent {
    pub(crate) message_id: Uuid,
    pub(crate) thread_id: Uuid,
    pub(crate) reason: String,
    /// What kind of failure it was, where the chat says more than `reason` (spec 043 FR-024).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) kind: Option<MessageErrorKind>,
}

/// A failure the chat names in the person's language instead of showing its technical reason.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub(crate) enum MessageErrorKind {
    /// The provider's certificate is not trusted.
    UntrustedCertificate,
}

/// Payload for `chat-tool-call` (contracts/tauri-commands.md). Emitted at
/// the persistence boundary — a `tool_call` row exists from this point,
/// even if the call is later blocked under `plan` mode (Phase 4).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ToolCallEvent {
    pub(crate) message_id: Uuid,
    pub(crate) thread_id: Uuid,
    pub(crate) tool_name: String,
    pub(crate) tool_input: Value,
    pub(crate) tool_source: String,
    /// The extension of an extension's tool (spec 018).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) tool_origin: Option<ToolOrigin>,
}

/// Payload for `chat-tool-result` (contracts/tauri-commands.md).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ToolResultEvent {
    pub(crate) message_id: Uuid,
    pub(crate) thread_id: Uuid,
    pub(crate) tool_call_id: String,
    pub(crate) content: String,
    pub(crate) is_error: bool,
}

/// Payload for `tool-permission-request` (contracts/tauri-commands.md).
/// Answered via `respond_tool_permission`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ToolPermissionRequestEvent {
    pub(crate) request_id: Uuid,
    pub(crate) thread_id: Uuid,
    pub(crate) tool_name: String,
    pub(crate) tool_input: Value,
    pub(crate) risk_class: &'static str,
    /// `mcp`, `cli`, `action` or `haextension`: the dialog words an action in plain language
    /// (spec 032 FR-008) and names an extension's tool with its extension (spec 018).
    pub(crate) tool_source: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) tool_origin: Option<ToolOrigin>,
}

/// Payload for `chat-choice-request` (spec 046, contracts/choice-contract.md).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ChoiceRequestEvent {
    pub(crate) request_id: Uuid,
    pub(crate) thread_id: Uuid,
    pub(crate) tool_name: String,
    #[serde(flatten)]
    pub(crate) choice: crate::chat::tools::ChoiceRequest,
}

pub(crate) fn risk_class_str(risk: crate::chat::tools::RiskClass) -> &'static str {
    match risk {
        crate::chat::tools::RiskClass::Safe => "safe",
        crate::chat::tools::RiskClass::Change => "change",
        crate::chat::tools::RiskClass::Risky => "risky",
    }
}

/// Strips a leaked `<tool_call>…</tool_call>` block (the Qwen text
/// convention) out of assistant text.
///
/// mistralrs 0.8.1's reasoning-mode content path (active whenever the
/// current model has thinking enabled, see `reasoning_requested_for`)
/// never runs generated text through its own tool-call-tag stripping the
/// way its non-reasoning path does, so on a reasoning-capable Qwen-family
/// model the raw tag the model emits to signal a tool call leaks into
/// `content` right alongside the correctly-parsed `tool_calls`. Only call
/// this once a step is already known to have produced `tool_calls` — at
/// that point any `<tool_call>` markup still in the text is certainly
/// leaked syntax, never legitimate prose.
///
/// Upstream bug: <https://github.com/EricLBuehler/mistral.rs/issues/2427>.
/// Remove this workaround once a fixed `mistralrs` release lands.
pub(crate) fn strip_leaked_tool_call_markup(text: &str) -> String {
    const OPEN: &str = "<tool_call>";
    const CLOSE: &str = "</tool_call>";

    let mut result = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(open_pos) = rest.find(OPEN) {
        result.push_str(&rest[..open_pos]);
        rest = &rest[open_pos + OPEN.len()..];
        rest = match rest.find(CLOSE) {
            Some(close_pos) => &rest[close_pos + CLOSE.len()..],
            None => "", // Unterminated tag: nothing legitimate follows it.
        };
    }
    result.push_str(rest);
    result
}

/// Payload for `chat-turn-complete` (contracts/tauri-commands.md). Fires
/// exactly once per `send_message` call, after the last step's own
/// per-step event — this is the frontend's sole signal to clear
/// `streamingMessageId`/`busy`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TurnCompleteEvent {
    pub(crate) thread_id: Uuid,
    pub(crate) assistant_message_id: Option<Uuid>,
    pub(crate) finish_reason: FinishReason,
}

/// Payload for `chat-agent-activity` (spec 011-composer-toolbar-parity,
/// contracts/tauri-commands.md). Only ever emitted while a Claude Code
/// delegate response is streaming — `batch_size` is set only on the update
/// where a new batch of that many sub-agents was just confirmed dispatched.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AgentActivityEvent {
    pub(crate) message_id: Uuid,
    pub(crate) active_count: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) batch_size: Option<usize>,
}

/// Payload for `chat-retry` (contracts/tauri-commands.md). Transient, not
/// persisted — informs the UI of an automatic retry attempt (spec.md
/// FR-012/FR-013) without ever adding a `chat_messages` row for the
/// discarded attempt.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RetryEvent {
    pub(crate) thread_id: Uuid,
    pub(crate) assistant_message_id: Uuid,
    /// 1-based.
    pub(crate) attempt: usize,
}

#[cfg(test)]
#[path = "events_tests.rs"]
mod tests;
