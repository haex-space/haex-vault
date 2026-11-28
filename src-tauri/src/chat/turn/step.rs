//! One step of a turn: a single LLM request/response, including its
//! bounded retry of transient failures.
//!
//! Split out of `chat/turn.rs` (2026-09-16); see
//! `docs/plans/2026-09-16-turn-module-split-design.md`.

use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::adapters::types::{
    AdapterStream, ChatRequest, StreamChunk, StreamError, ToolCall as LlmToolCall,
};
use crate::adapters::AdapterError;
use crate::chat::events::{
    AgentActivityEvent, MessageErrorKind, RetryEvent, TokenEvent, EVENT_CHAT_AGENT_ACTIVITY,
    EVENT_CHAT_RETRY, EVENT_CHAT_TOKEN,
};
use crate::chat::session::{ActiveSession, ChatState};

use super::TurnRunner;

/// Bounded automatic retry for a transient LLM-request failure (spec.md
/// FR-012), either from `stream_chat` itself or mid-stream. Worst case
/// adds 500ms + 1s + 2s = 3.5s of backoff across the 3 retries (4 attempts
/// total) before falling back to a terminal error.
pub const MAX_RETRY_ATTEMPTS: usize = 3;

/// Exponential backoff for retry attempt `attempt` (0-based: the first
/// retry is `attempt == 0`).
fn retry_backoff(attempt: usize) -> std::time::Duration {
    std::time::Duration::from_millis(500u64 << attempt.min(4))
}

/// Result of driving one step (a single LLM request/response, including
/// any retries) to completion.
enum StepOutcome {
    /// Reached `Done`. Carries the successful attempt's content.
    Success {
        assembled: String,
        tool_calls: Vec<LlmToolCall>,
        prompt_tokens: Option<usize>,
        completion_tokens: Option<usize>,
        ttft_ms: Option<u64>,
    },
    /// `abort_current_generation` fired — either while consuming a
    /// stream, or during a retry's backoff wait. Carries whatever text
    /// the current (now-abandoned) attempt had already produced.
    Cancelled(String),
    /// Not retryable, or the retry budget was spent. Carries whatever
    /// text the final attempt had already produced before it failed.
    Error {
        reason: String,
        partial: String,
        kind: Option<MessageErrorKind>,
    },
}

/// A [`StepOutcome`] flattened into the shape the turn loop consumes.
/// Replaces the seven-element tuple the loop used to destructure into.
pub(super) struct StepResult {
    pub(super) assembled: String,
    pub(super) tool_calls: Vec<LlmToolCall>,
    pub(super) prompt_tokens: Option<usize>,
    pub(super) completion_tokens: Option<usize>,
    pub(super) ttft_ms: Option<u64>,
    pub(super) error_reason: Option<String>,
    /// What kind of failure `error_reason` is, where the person is told more than its text.
    pub(super) error_kind: Option<MessageErrorKind>,
    pub(super) saw_done: bool,
}

impl StepResult {
    /// Whether this step ended by asking for tools. An errored step never
    /// carries tool calls, but both halves are checked to keep the
    /// original condition verbatim.
    pub(super) fn wants_tools(&self) -> bool {
        self.error_reason.is_none() && !self.tool_calls.is_empty()
    }
}

impl From<StepOutcome> for StepResult {
    fn from(outcome: StepOutcome) -> Self {
        match outcome {
            StepOutcome::Success {
                assembled,
                tool_calls,
                prompt_tokens,
                completion_tokens,
                ttft_ms,
            } => Self {
                assembled,
                tool_calls,
                prompt_tokens,
                completion_tokens,
                ttft_ms,
                error_reason: None,
                error_kind: None,
                saw_done: true,
            },
            StepOutcome::Cancelled(partial) => Self {
                assembled: partial,
                tool_calls: Vec::new(),
                prompt_tokens: None,
                completion_tokens: None,
                ttft_ms: None,
                error_reason: None,
                error_kind: None,
                saw_done: false,
            },
            StepOutcome::Error {
                reason,
                partial,
                kind,
            } => Self {
                assembled: partial,
                tool_calls: Vec::new(),
                prompt_tokens: None,
                completion_tokens: None,
                ttft_ms: None,
                error_reason: Some(reason),
                error_kind: kind,
                saw_done: false,
            },
        }
    }
}

enum RetryDecision {
    Retry,
    Cancelled,
    Bail(String),
}

/// Decides what to do about one failure while running a step (spec.md
/// FR-012): bail immediately if it is not transient or `MAX_RETRY_ATTEMPTS`
/// is already spent; otherwise emit `chat-retry` and wait out the backoff
/// (cancellable) before telling the caller to try again. Shared by both
/// `stream_chat` itself failing and a mid-stream `StreamError`, so one
/// budget covers either — a failure restarting the stream counts the same
/// as one that happened mid-stream.
async fn retry_or_bail(
    is_transient: bool,
    error_msg: String,
    attempt: &mut usize,
    cancel: &CancellationToken,
    thread_id: Uuid,
    assistant_message_id: Uuid,
    emit: &mut (dyn FnMut(&'static str, serde_json::Value) + Send),
) -> RetryDecision {
    if !is_transient || *attempt >= MAX_RETRY_ATTEMPTS {
        return RetryDecision::Bail(error_msg);
    }
    *attempt += 1;
    emit(
        EVENT_CHAT_RETRY,
        serde_json::to_value(RetryEvent {
            thread_id,
            assistant_message_id,
            attempt: *attempt,
        })
        .expect("RetryEvent always serializes"),
    );
    let sleep = tokio::time::sleep(retry_backoff(*attempt - 1));
    tokio::select! {
        biased;
        _ = cancel.cancelled() => RetryDecision::Cancelled,
        _ = sleep => RetryDecision::Retry,
    }
}

pub(crate) enum StreamStartError {
    Cancelled,
    Failed(String),
    /// The provider's certificate is not trusted (spec 043 FR-024); never retried.
    UntrustedCertificate(String),
}

/// Shared by the initial send and subsequent steps; retries share the
/// caller's budget with mid-stream failures and remain cancellable.
///
/// Stays a free function rather than a [`TurnRunner`] method because
/// `chat::commands` calls it for the turn's first step, before a
/// `TurnRunner` exists.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn start_step_stream(
    session: &ActiveSession,
    chat_state: &ChatState,
    request: &ChatRequest,
    cancel: &CancellationToken,
    attempt: &mut usize,
    thread_id: Uuid,
    assistant_message_id: Uuid,
    emit: &mut (dyn FnMut(&'static str, serde_json::Value) + Send),
) -> std::result::Result<AdapterStream, StreamStartError> {
    loop {
        let started = tokio::select! {
            biased;
            _ = cancel.cancelled() => return Err(StreamStartError::Cancelled),
            result = session.adapter.stream_chat(request.clone()) => result,
        };
        match started {
            Ok(stream) => {
                // The turn's slot keeps the handle (spec 068 R1): an abort of this turn ends this
                // generation and no other turn's.
                chat_state
                    .turns
                    .set_generation(assistant_message_id, stream.abort_handle());
                return Ok(stream);
            }
            Err(AdapterError::UntrustedCertificate { reason }) => {
                return Err(StreamStartError::UntrustedCertificate(reason))
            }
            Err(error) => match retry_or_bail(
                error.is_transient(),
                format!("adapter start: {error}"),
                attempt,
                cancel,
                thread_id,
                assistant_message_id,
                emit,
            )
            .await
            {
                RetryDecision::Retry => continue,
                RetryDecision::Cancelled => return Err(StreamStartError::Cancelled),
                RetryDecision::Bail(reason) => return Err(StreamStartError::Failed(reason)),
            },
        }
    }
}

/// Everything one pass over a live stream produced before it ended —
/// whether it ended on `Done`, on a closed channel, on a `StreamError`,
/// or on cancellation.
struct StreamPass {
    assembled: String,
    tool_calls: Vec<LlmToolCall>,
    prompt_tokens: Option<usize>,
    completion_tokens: Option<usize>,
    ttft_ms: Option<u64>,
    saw_done: bool,
    error: Option<StreamError>,
    cancelled: bool,
}

impl TurnRunner<'_> {
    /// Drives one step to completion, automatically retrying a transient
    /// failure — from either the initial `stream_chat` call or mid-stream
    /// — with backoff, up to `MAX_RETRY_ATTEMPTS` (spec.md FR-012).
    /// `stream` is `Some` only for the turn's already-in-flight first
    /// step; every other call (a new round after tool use, or a retry
    /// attempt) passes `None` and this function calls `stream_chat`
    /// itself.
    pub(super) async fn run_step(
        &mut self,
        mut stream: Option<AdapterStream>,
        attempt: &mut usize,
    ) -> StepResult {
        // Cloned once so the `tokio::select!` arms below borrow a local
        // rather than `self`, leaving `self` free for `emit_event`.
        let cancel = self.cancel.clone();
        let thread_id = self.thread_id;
        let assistant_message_id = self.assistant_message_id;

        loop {
            let mut live_stream = match stream.take() {
                Some(s) => s,
                None => match start_step_stream(
                    self.session,
                    self.chat_state,
                    &self.request,
                    &cancel,
                    attempt,
                    thread_id,
                    assistant_message_id,
                    &mut *self.emit,
                )
                .await
                {
                    Ok(stream) => stream,
                    Err(StreamStartError::Cancelled) => {
                        return StepOutcome::Cancelled(String::new()).into()
                    }
                    Err(StreamStartError::Failed(reason)) => {
                        return StepOutcome::Error {
                            reason,
                            partial: String::new(),
                            kind: None,
                        }
                        .into()
                    }
                    Err(StreamStartError::UntrustedCertificate(reason)) => {
                        return StepOutcome::Error {
                            reason,
                            partial: String::new(),
                            kind: Some(MessageErrorKind::UntrustedCertificate),
                        }
                        .into()
                    }
                },
            };

            let pass = self.consume_stream(&mut live_stream, &cancel).await;

            if pass.cancelled {
                return StepOutcome::Cancelled(pass.assembled).into();
            }

            if let Some(e) = pass.error {
                match retry_or_bail(
                    e.is_transient(),
                    e.to_string(),
                    attempt,
                    &cancel,
                    thread_id,
                    assistant_message_id,
                    &mut *self.emit,
                )
                .await
                {
                    RetryDecision::Retry => continue,
                    RetryDecision::Cancelled => {
                        return StepOutcome::Cancelled(pass.assembled).into()
                    }
                    RetryDecision::Bail(reason) => {
                        return StepOutcome::Error {
                            reason,
                            partial: pass.assembled,
                            kind: None,
                        }
                        .into()
                    }
                }
            }

            if !pass.saw_done && pass.tool_calls.is_empty() {
                // `abort_current_generation` aborted the stream producer —
                // the channel closed with neither a `Done` frame nor an
                // error. A closed channel right after `ToolCalls` (no `Done`
                // in between) is not itself a cancellation signal — every
                // adapter emits `Done` in the same terminal frame as
                // `ToolCalls`, so the caller decides what to do next purely
                // from `tool_calls` being non-empty, same as before this
                // function existed.
                return StepOutcome::Cancelled(pass.assembled).into();
            }

            return StepOutcome::Success {
                assembled: pass.assembled,
                tool_calls: pass.tool_calls,
                prompt_tokens: pass.prompt_tokens,
                completion_tokens: pass.completion_tokens,
                ttft_ms: pass.ttft_ms,
            }
            .into();
        }
    }

    /// Consumes one attempt's stream to its end, emitting `chat-token` as
    /// deltas arrive. Tokens stream live rather than being buffered
    /// server-side: on a transient failure the caller's `chat-retry` fires
    /// immediately so the frontend can clear that attempt's now-discarded
    /// partial text before the next attempt's tokens arrive (T039).
    async fn consume_stream(
        &mut self,
        live_stream: &mut AdapterStream,
        cancel: &CancellationToken,
    ) -> StreamPass {
        let mut pass = StreamPass {
            assembled: String::new(),
            tool_calls: Vec::new(),
            prompt_tokens: None,
            completion_tokens: None,
            ttft_ms: None,
            saw_done: false,
            error: None,
            cancelled: false,
        };
        let assistant_message_id = self.assistant_message_id;

        loop {
            let item = tokio::select! {
                biased;
                _ = cancel.cancelled() => {
                    pass.cancelled = true;
                    return pass;
                }
                item = live_stream.next() => item,
            };
            match item {
                None => return pass,
                Some(Ok(StreamChunk::Delta { content, reasoning })) => {
                    if !content.is_empty() || reasoning.is_some() {
                        pass.assembled.push_str(&content);
                        self.emit_event(
                            EVENT_CHAT_TOKEN,
                            TokenEvent {
                                message_id: assistant_message_id,
                                delta: content,
                                reasoning,
                            },
                        );
                    }
                }
                Some(Ok(StreamChunk::ToolCalls(calls))) => pass.tool_calls = calls,
                Some(Ok(StreamChunk::AgentActivity {
                    active_count,
                    batch_size,
                })) => {
                    self.emit_event(
                        EVENT_CHAT_AGENT_ACTIVITY,
                        AgentActivityEvent {
                            message_id: assistant_message_id,
                            active_count,
                            batch_size,
                        },
                    );
                }
                Some(Ok(StreamChunk::Done {
                    prompt_tokens,
                    completion_tokens,
                    ttft_ms,
                    ..
                })) => {
                    pass.saw_done = true;
                    pass.prompt_tokens = prompt_tokens;
                    pass.completion_tokens = completion_tokens;
                    pass.ttft_ms = ttft_ms;
                    return pass;
                }
                Some(Err(e)) => {
                    pass.error = Some(e);
                    return pass;
                }
            }
        }
    }
}

#[cfg(test)]
#[path = "step_tests.rs"]
mod tests;
