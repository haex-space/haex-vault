//! `send_message`, generation abort, and tool-permission resolution.
//!
//! Starting a turn (admission, persistence, the request, the first step and
//! spawning the rest) lives in [`super::send::start_turn`], shared with the
//! voice assistant (spec 066 T038); `send_message` maps its wire arguments
//! onto it and forwards every event to the window.

use std::sync::Arc;

use serde::Deserialize;
use tauri::{AppHandle, Emitter, State};
use uuid::Uuid;

use crate::adapters::cli_delegate::autonomy::AutonomyMode;
use crate::chat::tools::ApprovalDecision;
use crate::error::{HolziError, Result};
use crate::state::AppState;
use crate::state_utils::active_database;
use crate::storage::models as models_store;

pub use super::send::SendMessageResult;
use super::send::{start_turn, StartTurn};
use super::session::ChatState;

pub(crate) const PREF_LAST_ACTIVE_MODEL: &str = "chat.last_active_model_id";

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SendMessageArgs {
    /// If `None`, a new thread is created and its id is returned.
    pub thread_id: Option<Uuid>,
    pub content: String,
    /// Optional system prompt applied for this generation. Not
    /// persisted; a persistent system prompt is a later slice.
    pub system_prompt: Option<String>,
    /// Cap on generated tokens. `None` uses the adapter default.
    pub max_new_tokens: Option<usize>,
    /// Stable across every retry of the same user send (contract
    /// §send_message). Non-empty; deduplicates a frontend retry of its
    /// own `invoke()` call without resuming a failed generation — see
    /// [`resolve_idempotent_send`].
    pub idempotency_key: String,
    /// Per-request autonomy posture for a `cli_delegate` backend (spec
    /// 009-autonomous-delegate-mode). `None` behaves identically to
    /// `Some(AutonomyMode::Standard)` and is ignored entirely by
    /// non-delegate adapters. As of the spec's 2026-09-19 amendment the
    /// frontend now sources this from a persisted device preference
    /// (`chat.autonomy_mode`, defaulting to `Ungated`) and always sends a
    /// concrete value for a delegate model — this `None`/`Standard`
    /// fallback is a defensive default for an omitted field, not the
    /// product-level default (see `AutonomyMode`'s own doc comment).
    pub autonomy_mode: Option<AutonomyMode>,
    /// The selected reasoning option as a provider-native id (spec 012).
    /// `None` means Auto. Validated against the model's cached options in
    /// [`send_message`]; an option the model no longer offers is dropped,
    /// not an error. Never persisted with the message.
    pub reasoning_option: Option<String>,
    /// Files attached to this message (spec 011-composer-toolbar-parity).
    /// Scoped to this one send — never persisted or carried over to a
    /// later message (FR-017). Each path was already validated once via
    /// `inspect_attachment`; `send_message` re-validates at send time
    /// (FR-018) rather than trusting that earlier check.
    #[serde(default)]
    pub attachments: Vec<AttachmentInput>,
}

/// One attachment the frontend picked, identified by the open dialog's choice
/// (contracts/tauri-commands.md `send_message`; spec 043: a path or a provider
/// address). Content is read fresh by `send_message` itself, not carried over
/// the wire from the frontend.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AttachmentInput {
    pub path: crate::files::PickedFile,
}

/// Classifies a file the user is about to attach and reports whether the
/// currently selected model can actually use it (spec
/// 011-composer-toolbar-parity, contracts/tauri-commands.md
/// `inspect_attachment`) — called right after the file picker resolves, so
/// the composer can show the reason before the message is sent (FR-015/
/// FR-016). `model_id` is the same composite/local id `load_model`/
/// `active_model_info` use; usability comes from that model's cached
/// capabilities (spec 012), read once. Errors only when `path` itself cannot
/// be read/stat'd at all; an oversized or unsupported-type file still
/// resolves normally with `usable: false`.
#[tauri::command]
pub async fn inspect_attachment(
    app: AppHandle,
    state: State<'_, AppState>,
    path: crate::files::PickedFile,
    model_id: String,
) -> Result<crate::chat::attachments::AttachmentInfo> {
    use crate::chat::attachments::{usability_for, AttachmentUsability};

    let mut info = tauri::async_runtime::spawn_blocking(move || {
        crate::chat::attachments::classify_attachment(&app, &path)
    })
    .await
    .map_err(|e| HolziError::CrdtInit {
        reason: format!("classify_attachment join: {e}"),
    })??;

    let Some(kind) = info.kind else {
        return Ok(info);
    };
    let db = active_database(&state)?;
    let capabilities = db
        .read(move |r| models_store::get_model(r, &model_id))
        .await?
        .and_then(|row| row.capabilities);

    match usability_for(&kind.into(), capabilities.as_ref()) {
        AttachmentUsability::Usable => {}
        AttachmentUsability::NotAccepted => {
            info.usable = false;
            info.reason = Some("not usable by the selected model".to_string());
        }
        AttachmentUsability::Undetermined => {
            info.usable = false;
            info.reason = Some("attachment support for this model is not yet known".to_string());
        }
    }
    Ok(info)
}

/// Persists a typed user message, spawns a streaming generation, returns
/// both message ids so the frontend can subscribe. The assistant
/// message is inserted on completion. The work is [`start_turn`], shared
/// with the voice assistant (spec 066 T038).
#[tauri::command]
pub async fn send_message(app: AppHandle, args: SendMessageArgs) -> Result<SendMessageResult> {
    let emitter = app.clone();
    start_turn(
        &app,
        StartTurn {
            thread_id: args.thread_id,
            content: args.content,
            origin: None,
            idempotency_key: args.idempotency_key,
            system_prompt: args.system_prompt,
            max_new_tokens: args.max_new_tokens,
            autonomy_mode: args.autonomy_mode,
            reasoning_option: args.reasoning_option,
            attachments: args.attachments.into_iter().map(|a| a.path).collect(),
            emit: Arc::new(move |name, payload| {
                let _ = emitter.emit(name, payload);
            }),
        },
    )
    .await
}

/// Cancels the in-flight generation, if any. Idempotent — safe to
/// call when nothing is running. Split from the `#[tauri::command]`
/// wrapper so integration tests (`tests/chat_tool_loop.rs`) can trigger the
/// same cancellation without a live `AppHandle`/`State` — same pattern as
/// `resolve_idempotent_send` etc. above.
pub fn abort_turn(chat_state: &ChatState) -> Result<()> {
    {
        let mut guard = chat_state
            .current_generation
            .lock()
            .map_err(|e| HolziError::CrdtInit {
                reason: format!("chat.current_generation mutex poisoned: {e}"),
            })?;
        if let Some(abort) = guard.take() {
            abort.abort();
        }
    }
    // The turn's own cooperative signal: `chat/turn/step.rs`'s streaming loop races every step
    // against this token directly, so this is what actually stops the LLM stream promptly, not
    // just an in-flight `Tool::execute` (T032, CLI/MCP implementations race their own work
    // against it too and tear it down on the spot).
    {
        let guard = chat_state
            .turn_cancellation
            .lock()
            .map_err(|e| HolziError::CrdtInit {
                reason: format!("chat.turn_cancellation mutex poisoned: {e}"),
            })?;
        if let Some(token) = guard.as_ref() {
            token.cancel();
        }
    }
    // Dropping every pending sender resolves its `oneshot::Receiver` with
    // an `Err`, which the turn loop already treats as cancelled rather
    // than denied (distinct from a `respond_tool_permission` deny).
    {
        let mut pending =
            chat_state
                .pending_tool_approvals
                .lock()
                .map_err(|e| HolziError::CrdtInit {
                    reason: format!("chat.pending_tool_approvals mutex poisoned: {e}"),
                })?;
        let mut cancelled =
            chat_state
                .cancelled_tool_approvals
                .lock()
                .map_err(|e| HolziError::CrdtInit {
                    reason: format!("chat.cancelled_tool_approvals mutex poisoned: {e}"),
                })?;
        cancelled.extend(pending.keys().copied());
        pending.clear();
    }
    chat_state.pending_choices.cancel_all();
    Ok(())
}

#[tauri::command]
pub async fn abort_current_generation(chat: State<'_, ChatState>) -> Result<()> {
    abort_turn(&chat)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RespondToolPermissionArgs {
    pub request_id: Uuid,
    pub decision: ApprovalDecisionWire,
}

/// Wire shape for `decision` — a bare string, no wrapper object, matching
/// contracts/tauri-commands.md's `'allow' | 'deny'`.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalDecisionWire {
    Allow,
    Deny,
}

/// Resolves one open permission request; a late reply to a cancelled
/// request is a no-op, while an unknown id remains an input error.
#[tauri::command]
pub async fn respond_tool_permission(
    chat: State<'_, ChatState>,
    args: RespondToolPermissionArgs,
) -> Result<()> {
    resolve_tool_permission(&chat, args)
}

fn resolve_tool_permission(chat: &ChatState, args: RespondToolPermissionArgs) -> Result<()> {
    let sender = {
        let mut pending = chat
            .pending_tool_approvals
            .lock()
            .map_err(|e| HolziError::CrdtInit {
                reason: format!("chat.pending_tool_approvals mutex poisoned: {e}"),
            })?;
        pending.remove(&args.request_id)
    };
    let Some(sender) = sender else {
        if chat
            .cancelled_tool_approvals
            .lock()
            .map_err(|e| HolziError::CrdtInit {
                reason: format!("chat.cancelled_tool_approvals mutex poisoned: {e}"),
            })?
            .contains(&args.request_id)
        {
            return Ok(());
        }
        return Err(HolziError::InvalidInput {
            reason: format!("no pending tool permission request: {}", args.request_id),
        });
    };
    let decision = match args.decision {
        ApprovalDecisionWire::Allow => ApprovalDecision::Allow,
        ApprovalDecisionWire::Deny => ApprovalDecision::Deny,
    };
    // The receiver may already be gone (e.g. the turn ended some other
    // way) — that is not an error for the caller.
    let _ = sender.send(decision);
    Ok(())
}

#[cfg(test)]
#[path = "commands_tests.rs"]
mod tests;
