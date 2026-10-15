//! The turn/step loop: runs one `send_message` turn to completion,
//! including tool-permission gating, automatic retry of transient
//! failures, and persisting every row along the way.
//!
//! Split out of `chat/commands.rs` (2026-09-15 review), then split again
//! into this directory module (2026-09-16) — see
//! `docs/plans/2026-09-16-turn-module-split-design.md`.
//!
//! [`TurnRunner`] owns everything one turn needs: the borrowed collaborators
//! (database, chat state, session, event sink) plus the state that has to
//! survive across tool rounds (`request`, `parent_id`, `rounds_used`,
//! `next_created_at`). Holding that state in one place is what lets the
//! loop's sub-flows live in sibling modules:
//!
//! - [`step`] — one LLM request/response and its bounded retry
//! - [`tool_round`] — plan, execute, persist one round of tool calls
//! - [`persist`] — row writes, the shared event helpers, the logical clock

mod call_source;
mod choices;
mod images;
mod persist;
mod step;
mod tool_round;

pub use step::MAX_RETRY_ATTEMPTS;
pub(crate) use step::{start_step_stream, StreamStartError};
pub use tool_round::MAX_TOOL_ROUNDS;

use serde_json::Value;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::adapters::types::{AdapterStream, ChatRequest};
use crate::chat::events::{
    MessageCompleteEvent, MessageErrorEvent, TurnCompleteEvent, EVENT_CHAT_MESSAGE_COMPLETE,
    EVENT_CHAT_MESSAGE_ERROR, EVENT_CHAT_TURN_COMPLETE,
};
use crate::chat::session::{ActiveSession, ChatState};
use crate::storage::chat_messages::{ChatMessage, FinishReason, MessageRole};

use persist::{empty_tool_message, persist_final_message};
use step::StepResult;
use tool_round::RoundOutcome;

/// One in-flight turn. Fields are private to this module tree; the
/// sub-flows reach them from the sibling modules that `impl` on this type.
struct TurnRunner<'a> {
    db: &'a crate::vault_gate::VaultDb,
    chat_state: &'a ChatState,
    session: &'a ActiveSession,
    thread_id: Uuid,
    assistant_message_id: Uuid,
    cancel: CancellationToken,
    emit: &'a mut (dyn FnMut(&'static str, Value) + Send),
    /// Already carries the model/system-prompt/tools used for the turn's
    /// first step; grows as each tool round is appended.
    request: ChatRequest,
    /// Parent of the next row written — walks down the chain as interim,
    /// tool-call and tool-result rows are persisted.
    parent_id: Uuid,
    /// Tool rounds completed so far, against [`MAX_TOOL_ROUNDS`].
    rounds_used: usize,
    /// The turn's logical clock; see [`TurnRunner::bump_created_at`].
    /// Carried across tool rounds (not reset per round) so two rounds
    /// landing in the same millisecond still sort in round order.
    next_created_at: i64,
    /// The tools of the first step; a search never takes them away (spec 018: the extension
    /// tools that fit the message stay for the whole turn).
    first_offer: std::collections::HashSet<String>,
}

/// Drives one `send_message` turn to completion: consumes the
/// already-started first step's stream, executes any tool calls the model
/// requests, issues further steps as needed (bounded by
/// `MAX_TOOL_ROUNDS`), and persists every row along the way. Takes a plain
/// `emit` callback rather than an `AppHandle` so it runs without a live
/// Tauri app — `tests/chat_tool_loop.rs` passes a closure that records
/// events instead of dispatching them; the production caller wraps
/// `app.emit`.
#[allow(clippy::too_many_arguments)]
pub async fn run_turn(
    db: &crate::vault_gate::VaultDb,
    chat_state: &ChatState,
    session: &ActiveSession,
    thread_id: Uuid,
    user_message_id: Uuid,
    assistant_message_id: Uuid,
    request: ChatRequest,
    stream: AdapterStream,
    initial_attempt: usize,
    cancel: CancellationToken,
    emit: &mut (dyn FnMut(&'static str, Value) + Send),
) {
    TurnRunner {
        db,
        chat_state,
        session,
        thread_id,
        assistant_message_id,
        cancel,
        emit,
        first_offer: request.tools.iter().map(|tool| tool.name.clone()).collect(),
        request,
        parent_id: user_message_id,
        rounds_used: 0,
        next_created_at: now_ms(),
    }
    .run(stream, initial_attempt)
    .await
}

impl TurnRunner<'_> {
    /// The step loop. Every exit path has already emitted the turn's
    /// terminal events by the time it returns.
    async fn run(mut self, stream: AdapterStream, initial_attempt: usize) {
        // `Some` only for the first iteration (the already-in-flight stream
        // `send_message` started); every later iteration leaves this `None`
        // so `run_step` starts a fresh `stream_chat` call itself — the same
        // uniform path a retry attempt also takes (T037).
        let mut next_stream = Some(stream);
        let mut attempt = initial_attempt;

        loop {
            let step = self.run_step(next_stream.take(), &mut attempt).await;
            if step.wants_tools() {
                match self.tool_round(step).await {
                    RoundOutcome::Continue => continue,
                    RoundOutcome::Ended => return,
                }
            }
            self.finish_turn(step).await;
            return;
        }
    }

    /// Ends the turn on a step that asked for no tools: a plain answer, a
    /// step-level adapter error, or cancellation (the stream closed
    /// without `Done` and without an error — `abort_current_generation`).
    async fn finish_turn(&mut self, step: StepResult) {
        let finish_reason = if step.error_reason.is_some() {
            FinishReason::Error
        } else if step.saw_done {
            FinishReason::Complete
        } else {
            FinishReason::Cancelled
        };
        let created_at = self.bump_created_at();
        let (thread_id, message_id) = (self.thread_id, self.assistant_message_id);
        let autonomy_mode = persist::autonomy_mode_label(self.request.autonomy_mode);
        let final_msg = ChatMessage {
            role: MessageRole::Assistant,
            content: step.assembled,
            provider_id: self.session.provider_id,
            model_id: Some(self.session.model_id.clone()),
            prompt_tokens: step.prompt_tokens.map(|n| n as i64),
            completion_tokens: step.completion_tokens.map(|n| n as i64),
            finish_reason: Some(finish_reason),
            created_at,
            autonomy_mode,
            ..empty_tool_message(message_id, thread_id, Some(self.parent_id))
        };
        if let Err(reason) = persist_final_message(
            self.db,
            final_msg,
            self.session.provider_id,
            self.session.model_id.clone(),
        )
        .await
        {
            self.fail(reason);
            return;
        }

        if let Some(reason) = step.error_reason {
            self.emit_event(
                EVENT_CHAT_MESSAGE_ERROR,
                MessageErrorEvent {
                    message_id,
                    thread_id,
                    reason,
                    kind: step.error_kind,
                },
            );
        } else {
            self.emit_event(
                EVENT_CHAT_MESSAGE_COMPLETE,
                MessageCompleteEvent {
                    message_id,
                    thread_id,
                    prompt_tokens: step.prompt_tokens,
                    completion_tokens: step.completion_tokens,
                    ttft_ms: step.ttft_ms,
                },
            );
        }
        self.emit_event(
            EVENT_CHAT_TURN_COMPLETE,
            TurnCompleteEvent {
                thread_id,
                assistant_message_id: Some(message_id),
                finish_reason,
            },
        );
    }
}

pub(crate) fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}
