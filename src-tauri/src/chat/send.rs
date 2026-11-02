//! Starting a chat turn: admission, persistence, the request and the first step (spec 066 T038,
//! research R1).
//!
//! `send_message` (typed, `chat/commands.rs`) and the voice assistant (`assistant::turn`) both start
//! turns here, so a spoken turn is stored, built and locked exactly like a typed one. The caller
//! passes an `emit` closure that receives every event of the turn; the command forwards them to the
//! window, the assistant forwards them to the window and to the voice.
//!
//! The phases share `db`, `chat` and the clean-up of a send that failed to start, which is why they
//! stay in one function: idempotency admission and the transactional persist (delegating to
//! `send_admission`), building the `ChatRequest` from history, starting the first step
//! (`turn::start_step_stream`), and spawning `turn::run_turn` for the rest of the turn.

use std::sync::Arc;

use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, Manager};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::adapters::cli_delegate::autonomy::AutonomyMode;
use crate::adapters::types::{ChatMessage as LlmMessage, ChatRequest, ChatRole, ToolSpec};
use crate::chat::events::EVENT_CHAT_TOOL_AVAILABILITY;
use crate::chat::tools::availability::{ToolAvailability, ToolAvailabilityEvent};
use crate::chat::tools::offer::first_offer;
use crate::chat::tools::prompt::system_prompt;
use crate::error::{HolziError, Result};
use crate::model_capabilities::ModelCapabilities;
use crate::state::AppState;
use crate::storage::chat_threads::ThreadOrigin;
use crate::storage::providers::ProviderKind;
use crate::storage::{
    chat_messages::{self as msg_store, ChatMessage, MessageRole},
    chat_threads as thread_store, models as models_store, preferences,
    preferences::PrefScope,
};

use super::commands::PREF_LAST_ACTIVE_MODEL;
use super::send_admission::{
    persist_send_transaction, resolve_idempotent_send, IdempotentSend, PersistedSend,
};
use super::session::ChatState;
use super::turn::{now_ms, run_turn, start_step_stream, StreamStartError};

/// Receives every event of a turn by name, before and after the turn is spawned.
pub type Emit = Arc<dyn Fn(&'static str, Value) + Send + Sync>;

/// One turn to start.
pub struct StartTurn {
    /// `None` creates a new thread.
    pub thread_id: Option<Uuid>,
    pub content: String,
    /// Stored on a thread this turn creates; `None` is a typed conversation (spec 066).
    pub origin: Option<ThreadOrigin>,
    /// Stable across every retry of the same send; non-empty (see [`resolve_idempotent_send`]).
    pub idempotency_key: String,
    /// System prompt for this generation only; not persisted.
    pub system_prompt: Option<String>,
    /// Cap on generated tokens; `None` uses the adapter default.
    pub max_new_tokens: Option<usize>,
    /// Autonomy posture for a `cli_delegate` backend (spec 009); ignored by other adapters.
    pub autonomy_mode: Option<AutonomyMode>,
    /// The selected reasoning option (spec 012); dropped when the model no longer offers it.
    pub reasoning_option: Option<String>,
    /// Files attached to this message only (spec 011); read fresh here (FR-018).
    pub attachments: Vec<crate::files::PickedFile>,
    pub emit: Emit,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SendMessageResult {
    pub thread_id: Uuid,
    pub user_message_id: Uuid,
    pub assistant_message_id: Uuid,
    /// Names of attachments that were staged but could not be read at send
    /// time (FR-018 — e.g. deleted from disk after being attached) and
    /// were therefore excluded; the rest of the message still sent. Empty
    /// on the common path, and always empty for a duplicate/idempotent
    /// retry (nothing new was read for those).
    #[serde(default)]
    pub excluded_attachments: Vec<String>,
}

/// Removes the user turn and, when this send created it, its empty thread after startup failed
/// before `run_turn` could persist the assistant row.
async fn cleanup_staged_send(
    db: crate::vault_gate::VaultDb,
    user_message_id: Uuid,
    thread_id: Uuid,
    is_new_thread: bool,
) -> Result<()> {
    let cleanup = db
        .write(move |tx| {
            msg_store::delete_message(tx, user_message_id)?;
            if is_new_thread {
                thread_store::delete_thread(tx, thread_id)?;
            }
            Ok(())
        })
        .await;

    cleanup.map_err(|error| HolziError::CrdtInit {
        reason: format!("failed to clean up staged message or thread: {error}"),
    })
}

/// Converts persisted history rows into the adapter-neutral message shape
/// an adapter groups into its own wire format (data-model.md). `System`
/// rows are dropped — `system_prompt` carries system content out of band.
fn history_to_messages(history: &[ChatMessage]) -> Vec<LlmMessage> {
    history
        .iter()
        .filter_map(|m| match m.role {
            MessageRole::User => Some(LlmMessage {
                role: ChatRole::User,
                attachments: Vec::new(),
                content: m.content.clone(),
            }),
            MessageRole::Assistant => Some(LlmMessage {
                role: ChatRole::Assistant,
                attachments: Vec::new(),
                content: m.content.clone(),
            }),
            MessageRole::System => None,
            MessageRole::ToolCall => {
                let input = m
                    .tool_input
                    .as_deref()
                    .and_then(|s| serde_json::from_str(s).ok())
                    .unwrap_or(Value::Object(Default::default()));
                Some(LlmMessage {
                    role: ChatRole::ToolCall {
                        id: m.tool_call_id.clone().unwrap_or_default(),
                        name: m.tool_name.clone().unwrap_or_default(),
                        input,
                    },
                    attachments: Vec::new(),
                    content: String::new(),
                })
            }
            MessageRole::ToolResult => Some(LlmMessage {
                role: ChatRole::ToolResult {
                    call_id: m.tool_call_id.clone().unwrap_or_default(),
                    content: m.content.clone(),
                    is_error: m.tool_is_error.unwrap_or(false),
                },
                attachments: Vec::new(),
                content: String::new(),
            }),
        })
        .collect()
}

/// Whether reasoning is requested and shown for this request: the model's
/// cached record says it reasons (spec 012 FR-023), except for Qwen3's native
/// tool-call path, which must run with thinking disabled. The mistralrs
/// tool-calling example deliberately leaves thinking unspecified; enabling it
/// makes Qwen3 prone to spending the whole turn narrating a prospective tool
/// call instead of emitting the structured call. Reasoning stays enabled for
/// ordinary Qwen3 replies and for every other model. A model whose reasoning
/// control is not determined requests none.
pub(crate) fn reasoning_requested_for(
    capabilities: Option<&ModelCapabilities>,
    model_id: &str,
    tools: &[ToolSpec],
) -> bool {
    let is_qwen3_tool_request =
        model_id.to_ascii_lowercase().contains("qwen3") && !tools.is_empty();
    let reasons = capabilities
        .and_then(|c| c.reasoning.as_ref())
        .is_some_and(|control| control.reasons());
    reasons && !is_qwen3_tool_request
}

/// Keeps the requested reasoning option only if this model's cached options
/// offer it (spec 012 FR-013). Anything else — no record, a model without
/// selectable options, an option the provider has since dropped — resolves
/// to `None`, so the model's own default applies and a stale id never
/// reaches a provider.
pub(crate) fn validated_reasoning_option(
    capabilities: Option<&ModelCapabilities>,
    requested: Option<String>,
) -> Option<String> {
    let requested = requested?;
    capabilities
        .and_then(|c| c.reasoning.as_ref())
        .filter(|control| control.offers(&requested))
        .map(|_| requested)
}

/// Persists the user message, starts the first step and spawns the rest of the turn; returns the
/// ids the caller follows the turn by. The assistant message is inserted on completion.
pub async fn start_turn(app: &AppHandle, turn: StartTurn) -> Result<SendMessageResult> {
    if turn.idempotency_key.trim().is_empty() {
        return Err(HolziError::InvalidIdempotencyKey);
    }
    let state = app.state::<AppState>();
    let chat = app.state::<ChatState>();

    // Reserve before reading the vault, while still letting an idempotent
    // retry return its existing ids when a turn already owns the reservation.
    let operation = chat.acquire_operation();
    let db = state.database()?;
    let cancel_token = CancellationToken::new();
    if operation.is_ok() {
        // Reserve cancellation before the first await, including DB staging.
        // A busy idempotent replay must never replace the live turn's token.
        *chat
            .turn_cancellation
            .lock()
            .map_err(|e| HolziError::CrdtInit {
                reason: format!("chat.turn_cancellation mutex poisoned: {e}"),
            })? = Some(cancel_token.clone());
    }

    // Dedup against any prior send sharing this idempotencyKey before
    // minting anything new (contract §send_message). This only guards
    // the frontend retrying its own uncertain `invoke()` call — it
    // never resumes a generation that failed after acceptance.
    let dedup_key = turn.idempotency_key.clone();
    let dedup_thread_id = turn.thread_id;
    let dedup_content = turn.content.clone();
    let decision = db
        .read(move |r| resolve_idempotent_send(r, &dedup_key, dedup_thread_id, &dedup_content))
        .await?;

    match decision {
        IdempotentSend::Mismatch => {
            return Err(HolziError::IdempotencyKeyConflict);
        }
        IdempotentSend::Duplicate {
            thread_id,
            user_message_id,
            assistant_message_id,
        } => {
            return Ok(SendMessageResult {
                thread_id,
                user_message_id,
                assistant_message_id,
                excluded_attachments: Vec::new(),
            });
        }
        IdempotentSend::Fresh { .. } => {}
    }

    let operation = operation?;
    let session = {
        let guard = chat.session.lock().map_err(|e| HolziError::CrdtInit {
            reason: format!("chat.session mutex poisoned: {e}"),
        })?;
        guard.clone().ok_or_else(|| HolziError::InvalidInput {
            reason: "no model loaded".into(),
        })?
    };

    let persist_key = turn.idempotency_key.clone();
    let persist_thread_id = turn.thread_id;
    let persist_content = turn.content.clone();
    let persist_origin = turn.origin;
    let persist_provider_id = session.provider_id;
    let persist_model_id = session.model_id.clone();
    let persisted = db
        .write(move |tx| {
            persist_send_transaction(
                tx,
                &persist_key,
                persist_thread_id,
                &persist_content,
                persist_origin,
                persist_provider_id,
                &persist_model_id,
                now_ms(),
            )
        })
        .await?;

    let (thread_id, user_message_id, assistant_message_id) = match persisted {
        PersistedSend::Mismatch => {
            return Err(HolziError::IdempotencyKeyConflict);
        }
        PersistedSend::UnknownThread { thread_id } => {
            return Err(HolziError::NotFound {
                name: thread_id.to_string(),
            });
        }
        PersistedSend::Duplicate {
            thread_id,
            user_message_id,
            assistant_message_id,
        } => {
            return Ok(SendMessageResult {
                thread_id,
                user_message_id,
                assistant_message_id,
                excluded_attachments: Vec::new(),
            });
        }
        PersistedSend::Fresh {
            thread_id,
            user_message_id,
            assistant_message_id,
        } => (thread_id, user_message_id, assistant_message_id),
    };

    // A newly persisted user message lets the built-in agent begin a new browser session after
    // the user stopped its last one (spec 065 FR-016). Do not resume on an idempotent retry or a
    // message that failed before it was accepted.
    if let Some(sessions) = app.try_state::<crate::agent_browser::sessions::BrowserSessions>() {
        sessions.resume(&crate::caller::Caller::BuiltinAgent);
    }

    // The thread and user message were persisted atomically above.
    // Spec 002 §FR-009: `chat.last_active_model_id` is written
    // EXCLUSIVELY here, never by `load_model` — the message is the user's
    // intent signal, a picker-only click is not. The preference is
    // committed after adapter startup succeeds so a failed send cannot
    // roll back a newer overlapping send's value.
    let session_model_id = session.model_id.clone();
    let this_device = db.device_id();
    let preference_model_id = session_model_id.clone();
    let is_new_thread = turn.thread_id.is_none();

    // Build the request from full history + the just-inserted user turn.
    let capabilities_model_id = session.model_id.clone();
    let (history, model_capabilities) = db
        .read(move |r| {
            let msgs = msg_store::list_messages(r, thread_id)?;
            // The selected model's cached capabilities, read in this same
            // blocking lookup (spec 012): one snapshot drives reasoning,
            // the validated option and the serializer.
            let capabilities = models_store::get_model(r, &capabilities_model_id)?
                .and_then(|row| row.capabilities);
            Ok((msgs, capabilities))
        })
        .await?;

    // For api_key models the adapter needs the raw remote id, not the
    // composite one — split it off here so the adapter stays vendor-
    // scoped and does not know about holzi's composite scheme.
    let request_model_id = session
        .model_id
        .split_once(':')
        .map(|(_, remote)| remote.to_string())
        .unwrap_or_else(|| session.model_id.clone());
    // The person comes first: a check of the model's tool use must not make the answer wait.
    chat.cancel_tool_check();
    // Tools only for a model that can use them; a delegate runs its own agent (spec 032 R14).
    let tool_availability =
        ToolAvailability::of(session.provider_kind, model_capabilities.as_ref());
    let tools = {
        let registry = chat
            .tool_registry
            .lock()
            .map_err(|e| HolziError::CrdtInit {
                reason: format!("chat.tool_registry mutex poisoned: {e}"),
            })?;
        // Spec 018: the extension tools that fit the message come with the first step.
        first_offer(tool_availability, &registry, &turn.content)
    };
    let reasoning_requested =
        reasoning_requested_for(model_capabilities.as_ref(), &request_model_id, &tools);
    let reasoning_option =
        validated_reasoning_option(model_capabilities.as_ref(), turn.reasoning_option.clone());

    // Re-read each attachment fresh at send time (FR-018) rather than
    // trusting the earlier `inspect_attachment` check — a file can vanish
    // or change in between. The selected model's cached capability snapshot
    // is checked here too, because the model may have changed since inspect.
    // A failure or unsupported kind excludes just that one attachment
    // (reported back via `excluded_attachments`) instead of failing the whole
    // send.
    let attachment_paths = turn.attachments;
    let attachment_capabilities = model_capabilities.clone();
    let opener = app.clone();
    let (read_attachments, excluded_attachments) =
        tauri::async_runtime::spawn_blocking(move || {
            let mut read = Vec::new();
            let mut excluded = Vec::new();
            for path in attachment_paths {
                let file_name = crate::files::picked::display_name(&opener, &path);
                match crate::chat::attachments::read_attachment_content(&opener, &path) {
                    Ok(attachment)
                        if matches!(
                            crate::chat::attachments::usability_for(
                                &attachment.kind,
                                attachment_capabilities.as_ref(),
                            ),
                            crate::chat::attachments::AttachmentUsability::Usable
                        ) =>
                    {
                        read.push(attachment)
                    }
                    Ok(_) | Err(_) => excluded.push(file_name),
                }
            }
            (read, excluded)
        })
        .await
        .map_err(|e| HolziError::CrdtInit {
            reason: format!("attachment read join: {e}"),
        })?;

    let mut messages = history_to_messages(&history);
    if let Some(current_turn_message) = messages.last_mut() {
        current_turn_message.attachments = read_attachments;
    }

    let request = ChatRequest {
        sampling: Default::default(),
        model_id: request_model_id,
        thread_id: Some(thread_id),
        system_prompt: system_prompt(turn.system_prompt.as_deref(), &tools),
        messages,
        reasoning_requested,
        max_new_tokens: turn.max_new_tokens,
        tools,
        // Only a `cli_delegate` session actually has an autonomy posture to
        // apply — a `local`/`api_key` turn must never carry a non-`Standard`
        // label just because the frontend happened to send one (code
        // review; `TurnRunner`/persistence take whatever `ChatRequest`
        // carries at face value).
        autonomy_mode: if session.provider_kind == ProviderKind::CliDelegate {
            turn.autonomy_mode.unwrap_or_default()
        } else {
            AutonomyMode::Standard
        },
        reasoning_option,
        capabilities: model_capabilities,
    };

    let emit = turn.emit;
    let mut attempt = 0;
    let mut forward = |name: &'static str, payload: Value| emit(name, payload);
    let stream = match start_step_stream(
        &session,
        &chat,
        &request,
        &cancel_token,
        &mut attempt,
        thread_id,
        assistant_message_id,
        &mut forward,
    )
    .await
    {
        Ok(stream) => stream,
        Err(error) => {
            let (error, untrusted) = match error {
                StreamStartError::Cancelled => ("generation cancelled".to_string(), false),
                StreamStartError::Failed(reason) => (reason, false),
                StreamStartError::UntrustedCertificate(reason) => (reason, true),
            };
            if let Err(cleanup_error) =
                cleanup_staged_send(db.clone(), user_message_id, thread_id, is_new_thread).await
            {
                return Err(HolziError::CrdtInit {
                    reason: format!("adapter start: {error}; {cleanup_error}"),
                });
            }
            if untrusted {
                return Err(HolziError::UntrustedCertificate { reason: error });
            }
            return Err(HolziError::InvalidInput {
                reason: format!("adapter start: {error}"),
            });
        }
    };

    db.write(move |tx| {
        preferences::insert_or_update(
            tx,
            PrefScope::Device(this_device),
            PREF_LAST_ACTIVE_MODEL,
            &preference_model_id,
        )
        .map(|_| ())
    })
    .await?;

    if let Ok(payload) = serde_json::to_value(ToolAvailabilityEvent {
        thread_id,
        state: tool_availability,
        delegate_kind: session
            .adapter
            .delegate_vendor()
            .map(crate::mcp_server::delegates::kind_of),
    }) {
        emit(EVENT_CHAT_TOOL_AVAILABILITY, payload);
    }

    let app_for_task = app.clone();
    let session_for_task = session.clone();
    let assistant_db = db.clone();

    let spawn_result = state.gate().spawn(async move {
        let _operation = operation;
        let chat_state = app_for_task.state::<ChatState>();
        let mut forward = |name: &'static str, payload: Value| emit(name, payload);
        run_turn(
            &assistant_db,
            &chat_state,
            &session_for_task,
            thread_id,
            user_message_id,
            assistant_message_id,
            request,
            stream,
            attempt,
            cancel_token,
            &mut forward,
        )
        .await;
    });
    if let Err(error) = spawn_result {
        if let Err(cleanup_error) =
            cleanup_staged_send(db, user_message_id, thread_id, is_new_thread).await
        {
            return Err(HolziError::CrdtInit {
                reason: format!("generation spawn rejected: {error}; {cleanup_error}"),
            });
        }
        return Err(error);
    }

    Ok(SendMessageResult {
        thread_id,
        user_message_id,
        assistant_message_id,
        excluded_attachments,
    })
}
