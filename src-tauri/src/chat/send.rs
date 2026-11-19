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
//! Maintainability exception (500-LoC boundary): keep the shared startup cleanup here for now;
//! a later split will move request construction into `send_request`, leaving admission and
//! stream startup here with their existing cleanup boundary.

use std::sync::Arc;

use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, Manager};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::adapters::cli_delegate::autonomy::AutonomyMode;
use crate::adapters::types::{ChatMessage as LlmMessage, ChatRequest, ChatRole, ToolSpec};
use crate::chat::events::EVENT_CHAT_TOOL_AVAILABILITY;
use crate::chat::reply_language::{read_typed, ReplyLanguage};
use crate::chat::tools::availability::{ToolAvailability, ToolAvailabilityEvent};
use crate::chat::tools::extension_offer::extensions_overview;
use crate::chat::tools::offer::earlier_user_messages;
use crate::chat::tools::offer::with_search;
use crate::chat::tools::prompt::{system_prompt_with, SearchPrompt};
use crate::chat::tools::route::{first_step_offer_in, last_turn_tools, RouteCall};
use crate::error::{HolziError, Result};
use crate::local_time::Clock;
use crate::model_capabilities::ModelCapabilities;
use crate::state::AppState;
use crate::storage::chat_threads::ThreadOrigin;
use crate::storage::providers::ProviderKind;
use crate::storage::{
    chat_messages::{self as msg_store, ChatMessage, MessageRole},
    chat_threads as thread_store, models as models_store,
};

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
    /// The window's offset from UTC in minutes, so the model is told the user's local date and
    /// time; `None` tells it the time in UTC.
    pub utc_offset_minutes: Option<i32>,
    /// The language a spoken turn is answered in: the one its conversation is held in. `None`
    /// takes the interface language (spec 066 FR-002a).
    pub reply_language: Option<ReplyLanguage>,
    /// Cap on generated tokens; `None` uses the adapter default.
    pub max_new_tokens: Option<usize>,
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
pub(crate) fn history_to_messages(history: &[ChatMessage]) -> Vec<LlmMessage> {
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
            // A summary is shown in the history; a request uses it only through `compact` (spec 086).
            MessageRole::System | MessageRole::Summary => None,
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
/// tool-call path, which must run with thinking disabled. The Qwen3
/// tool-calling path deliberately leaves thinking unspecified; enabling it
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
    // A model the active agent profile does not offer is not usable (spec 083 FR-006b); nothing is
    // stored for a turn that is refused here. A model of an Abo-Agent's provider is a model like
    // any other (PR J, FR-027).
    let model = session.model_id.clone();
    let available = db
        .read(move |q| crate::agent_profiles::resolve::model_available(q, &model))
        .await?;
    if !available {
        return Err(HolziError::InvalidInput {
            reason: "chat.error.modelNotInProfile".into(),
        });
    }

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

    // The thread and user message were persisted atomically above. Sending no longer writes
    // `chat.last_active_model_id`: only a pick in the model menu does (`select_model`, spec 002
    // FR-009 as amended 2026-11-02), so continuing a session with its own model does not change
    // what a new session starts with.
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
    // Spec 082: a routing call names the request's domains; the first step offers their tools.
    let call = RouteCall::of(&session, &request_model_id);
    // Spec 087: only a turn with a routing call (a local model) gets the routed instruction.
    let routes = call.is_some();
    // Spec 018, amendment 2026-11-09: the whole conversation counts for the offer, not only the
    // last message, so a follow-up keeps the tools its topic needs.
    let earlier = {
        let users: Vec<&str> = history
            .iter()
            .filter(|message| {
                matches!(message.role, MessageRole::User) && message.id != user_message_id
            })
            .map(|message| message.content.as_str())
            .chain(std::iter::once(turn.content.as_str()))
            .collect();
        earlier_user_messages(&users)
    };
    // The domains the last turn used stay offered, so a follow-up keeps its topic's tools.
    let used_tools = last_turn_tools(&history, user_message_id);
    let (tools, routed) = first_step_offer_in(
        tool_availability,
        &chat.tool_registry,
        call.as_ref(),
        &earlier,
        &used_tools,
        &turn.content,
    )
    .await;
    // Spec 083: the agent profile of the model decides which tools the first step offers, and
    // which extensions the system prompt names (spec 018, amendment 2026-11-09).
    let (tools, extensions_overview) = {
        let mut tools = tools;
        let mut overview = None;
        if !tools.is_empty() {
            let rights = crate::chat::tools::profile_gate::rights_of_session(&db, &session).await;
            let registry = chat.tool_registry.lock().unwrap_or_else(|e| e.into_inner());
            crate::chat::tools::profile_gate::retain_permitted(&rights, &registry, &mut tools);
            // Spec 065 FR-063: a model that is shown no pictures works with element numbers.
            crate::chat::turn::images::retain_usable(
                &mut tools,
                crate::chat::turn::images::accepts_images(model_capabilities.as_ref()),
            );
            overview = extensions_overview(&crate::chat::tools::profile_gate::permitted_summaries(
                &rights, &registry,
            ));
        }
        (tools, overview)
    };
    log::info!(
        "chat: the first step offers {} tools after profile filtering ({} routed domains, {} kept from the last turn {:?}, routing ran: {})",
        tools.len(),
        routed.keys.len(),
        routed.kept.len(),
        routed.kept,
        routed.ms.is_some()
    );
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

    // The language the agent answers in (spec 066 FR-002a): a spoken turn brings the language its
    // conversation is held in, a typed one takes the interface language. Without one the model
    // follows the user, so a failed read must not stop the turn.
    let reply_language = match turn.reply_language {
        Some(language) => Some(language),
        None => db.read(|r| read_typed(r)).await.unwrap_or_else(|error| {
            log::warn!("chat: could not read the reply language: {error}");
            None
        }),
    };
    let (autonomy_mode, autonomy_deny) = if session.provider_kind == ProviderKind::CliDelegate {
        let model = session.model_id.clone();
        let rights = db
            .read(move |q| crate::agent_profiles::resolve::delegate_rights(q, &model))
            .await?;
        (rights.autonomy, rights.deny.into_iter().collect())
    } else {
        (AutonomyMode::Standard, Vec::new())
    };
    let clock = Clock::now(turn.utc_offset_minutes.unwrap_or(0));
    let prompt_for = |tools: &[crate::adapters::types::ToolSpec]| {
        system_prompt_with(
            turn.system_prompt.as_deref(),
            tools,
            Some(clock),
            reply_language,
            extensions_overview.as_deref(),
            routes,
        )
    };
    let system_prompt = prompt_for(&tools);
    // A first step that lacks the tool search (spec 087 research R6.3, amended 2026-11-11: every
    // offer of holzi's has it now) gets it with the next step, and the prompt that goes with it.
    let search_prompt = {
        let mut later = tools.clone();
        let added = !tools.is_empty()
            && with_search(
                &chat.tool_registry.lock().unwrap_or_else(|e| e.into_inner()),
                &mut later,
            );
        added.then(|| SearchPrompt {
            first: system_prompt.clone(),
            with_search: prompt_for(&later),
        })
    };
    let mut request = ChatRequest {
        sampling: Default::default(),
        model_id: request_model_id,
        thread_id: Some(thread_id),
        system_prompt,
        messages,
        reasoning_requested,
        max_new_tokens: turn.max_new_tokens,
        tools,
        // Only a `cli_delegate` session has an autonomy posture to apply; it is the right of the
        // agent profile of its model (spec 083 FR-030), never a value the window sends.
        autonomy_mode,
        autonomy_deny,
        reasoning_option,
        capabilities: model_capabilities,
    };

    // A conversation that has grown past the context of a small model is sent shortened, and summarised
    // where that is not enough (spec 086).
    if let Some(limit) = session.adapter.context_limit() {
        let request_model_id = request.model_id.clone();
        let source = crate::chat::compact::Source {
            adapter: session.adapter.as_ref(),
            request_model_id: &request_model_id,
            model_id: &session.model_id,
            provider_id: session.provider_id,
            db: &db,
            thread_id,
            limit,
            language: reply_language,
            cancel: &cancel_token,
            emit: &*turn.emit,
        };
        crate::chat::compact::prepare(&source, &mut request, &history).await;
    }

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
            search_prompt,
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
