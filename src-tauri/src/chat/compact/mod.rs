//! A conversation that outgrows the context of its model goes on (spec 086 US3): before a turn's first
//! request goes out, the cheap steps come first and only when they do not leave room, the older messages
//! are summarised by the model itself. The summary is a row of its own in the thread (role `summary`, its
//! `parent_id` the last row it covers), shown in the history; the covered rows stay as they are and a
//! request uses the summary only where it would not fit without it.
//!
//! One decision per turn, at its start (research R10): the rows before the latest user message do not
//! change during a turn, so the summary is made once; what grows inside a turn is the open tool round,
//! which `chat::turn::fit` shrinks.
//!
//! Order (FR-012): the results of earlier tool results are left out, then a stored summary is used, then a
//! new one is written, and the latest round's largest result is cut last. "Fits" leaves a quarter of the
//! context free for the round (the share one tool result may take).

pub mod select;
pub mod summarize;

use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use self::select::{latest_summary, messages_before, span, tail_start, Applied};
use self::summarize::{summarize as write_summary, SummaryError};
use crate::adapters::types::{ChatMessage as LlmMessage, ChatRequest};
use crate::adapters::ProviderAdapter;
use crate::chat::events::EVENT_CHAT_COMPACTION;
use crate::chat::reply_language::ReplyLanguage;
use crate::chat::tools::context_budget::SHARE;
use crate::chat::turn::fit::{chars_per_token, fit, leave_out_old_results, tokens};
use crate::storage::chat_messages::{self as msg_store, ChatMessage, FinishReason, MessageRole};
use crate::vault_gate::VaultDb;

/// What a turn's compaction did.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Compaction {
    /// A summary stands in for older messages in this turn's request.
    pub used: bool,
    /// A new summary was written and stored.
    pub wrote: bool,
    /// A summary was needed and could not be written; the request was shortened the other way.
    pub failed: bool,
}

/// What `prepare` needs to know about the turn.
pub struct Source<'a> {
    pub adapter: &'a dyn ProviderAdapter,
    /// The model id the adapter takes (the remote id for a provider's model).
    pub request_model_id: &'a str,
    /// The id kept on the rows this writes.
    pub model_id: &'a str,
    pub provider_id: Option<Uuid>,
    pub db: &'a VaultDb,
    pub thread_id: Uuid,
    /// The model's context in tokens.
    pub limit: usize,
    /// The language the summary is written in: the one of the turn (the interface language for a typed
    /// one); `None` when it is not known, and the model then follows the person.
    pub language: Option<ReplyLanguage>,
    pub cancel: &'a CancellationToken,
    pub emit: &'a (dyn Fn(&'static str, Value) + Send + Sync),
}

/// The paragraph of the system prompt a summary becomes.
fn section(text: &str) -> String {
    format!(
        "\n\nEarlier in this conversation (a summary of the messages that are no longer shown):\n{text}"
    )
}

/// What a request looked like before compaction touched it.
struct Original {
    messages: Vec<LlmMessage>,
    system_prompt: Option<String>,
}

impl Original {
    fn of(request: &ChatRequest) -> Self {
        Self {
            messages: request.messages.clone(),
            system_prompt: request.system_prompt.clone(),
        }
    }

    /// `request` as it was, with the rows up to `covers` replaced by `text`.
    fn apply(&self, request: &mut ChatRequest, history: &[ChatMessage], covers: usize, text: &str) {
        let skipped = messages_before(history, covers + 1);
        request.messages = self.messages[skipped.min(self.messages.len())..].to_vec();
        request.system_prompt = Some(format!(
            "{}{}",
            self.system_prompt.as_deref().unwrap_or_default(),
            section(text)
        ));
    }

    fn restore(&self, request: &mut ChatRequest) {
        request.messages = self.messages.clone();
        request.system_prompt = self.system_prompt.clone();
    }
}

fn announce(source: &Source<'_>, state: &str) {
    (source.emit)(
        EVENT_CHAT_COMPACTION,
        json!({ "threadId": source.thread_id, "state": state }),
    );
}

/// The row that keeps a summary in the thread, directly after the last row it covers.
fn summary_row(source: &Source<'_>, covered: &ChatMessage, text: String) -> ChatMessage {
    ChatMessage {
        id: Uuid::new_v4(),
        thread_id: source.thread_id,
        parent_id: Some(covered.id),
        role: MessageRole::Summary,
        content: text,
        provider_id: source.provider_id,
        model_id: Some(source.model_id.to_owned()),
        prompt_tokens: None,
        completion_tokens: None,
        finish_reason: Some(FinishReason::Complete),
        created_at: covered.created_at,
        idempotency_key: None,
        tool_name: None,
        tool_call_id: None,
        tool_input: None,
        tool_is_error: None,
        tool_source: None,
        autonomy_mode: None,
        tool_origin: None,
    }
}

/// Fits `request` (made from `history`, the rows of the thread with the person's latest message last) to the
/// context of the model: see the module documentation for the order. The rows are never changed, only the
/// request, and a new summary is added as a row.
pub async fn prepare(
    source: &Source<'_>,
    request: &mut ChatRequest,
    history: &[ChatMessage],
) -> Compaction {
    let mut state = Compaction::default();
    // The model's own count where the adapter has its tokenizer: a guess lets JSON results through.
    let count = |request: &ChatRequest| source.adapter.count_tokens(request);
    let room = source.limit - source.limit / SHARE;
    let first = tokens(request, room, &count);
    if first <= room {
        return state;
    }
    // Measured on the whole request; what is left out first (tool results) is the densest part, so the
    // figure stays on the safe side for the rest.
    let scale = chars_per_token(request, first);
    let original = Original::of(request);
    leave_out_old_results(request, room, scale);
    if tokens(request, room, &count) <= room {
        return state;
    }

    // A summary, stored or new, if one can be had.
    let stored = latest_summary(history);
    let fitted = match stored {
        Some(applied) => try_summary(&original, request, history, applied, room, scale, &count),
        None => false,
    };
    if fitted {
        state.used = true;
        fit(request, source.limit, &count);
        return state;
    }
    match write_new(source, history, stored, scale).await {
        Ok(Some((covers, text, row))) => {
            original.apply(request, history, covers, &text);
            leave_out_old_results(request, room, scale);
            state.used = true;
            state.wrote = row;
            announce(source, "done");
        }
        Ok(None) => {
            // Nothing newer to summarise. A stored summary that does not leave the round its room may still
            // fit the context, and the whole history may not.
            let kept = stored.is_some_and(|applied| {
                original.apply(
                    request,
                    history,
                    applied.covers,
                    &history[applied.summary].content,
                );
                leave_out_old_results(request, room, scale);
                tokens(request, source.limit, &count) <= source.limit
            });
            if kept {
                state.used = true;
            } else {
                original.restore_and_leave_out(request, room, scale);
            }
        }
        Err(SummaryError::Cancelled) => {
            // The notice of the summary goes away with the turn.
            announce(source, "cancelled");
            original.restore_and_leave_out(request, room, scale);
        }
        Err(SummaryError::Failed(reason)) => {
            log::warn!("chat: the conversation could not be summarised: {reason}");
            state.failed = true;
            announce(source, "failed");
            original.restore_and_leave_out(request, room, scale);
        }
    }
    fit(request, source.limit, &count);
    state
}

impl Original {
    fn restore_and_leave_out(&self, request: &mut ChatRequest, room: usize, scale: f64) {
        self.restore(request);
        leave_out_old_results(request, room, scale);
    }
}

/// Applies the stored summary to `request` and reports whether the request then leaves its room.
fn try_summary(
    original: &Original,
    request: &mut ChatRequest,
    history: &[ChatMessage],
    applied: Applied,
    room: usize,
    scale: f64,
    count: crate::chat::turn::fit::Count<'_>,
) -> bool {
    original.apply(
        request,
        history,
        applied.covers,
        &history[applied.summary].content,
    );
    leave_out_old_results(request, room, scale);
    if tokens(request, room, count) <= room {
        return true;
    }
    original.restore(request);
    false
}

/// Writes a summary of what lies between the stored one and the tail and stores it. `Ok(None)` when there is
/// nothing older than the tail; the last field says whether the row was stored.
async fn write_new(
    source: &Source<'_>,
    history: &[ChatMessage],
    stored: Option<Applied>,
    scale: f64,
) -> Result<Option<(usize, String, bool)>, SummaryError> {
    let after = stored.map(|applied| applied.covers);
    let Some(tail) = tail_start(history, source.limit, after) else {
        return Ok(None);
    };
    let covered_rows = span(history, after, tail);
    if covered_rows.is_empty() {
        return Ok(None);
    }
    let rows: Vec<&ChatMessage> = covered_rows.iter().map(|at| &history[*at]).collect();
    let prior = stored.map(|applied| history[applied.summary].content.as_str());
    announce(source, "summarising");
    let written = write_summary(
        source.adapter,
        source.request_model_id,
        prior,
        &rows,
        summarize::Context {
            tokens: source.limit,
            chars_per_token: scale,
        },
        source.language,
        source.cancel,
    )
    .await?;
    let last = covered_rows[written.covered.clamp(1, covered_rows.len()) - 1];
    let row = summary_row(source, &history[last], written.text.clone());
    // A summary that is not in the history is not used either: it would be written again next turn.
    source
        .db
        .write(move |tx| msg_store::insert_message(tx, &row).map(|_| ()))
        .await
        .map_err(|error| {
            SummaryError::Failed(format!("the summary could not be stored: {error}"))
        })?;
    Ok(Some((last, written.text, true)))
}

#[cfg(test)]
#[path = "compact_tests.rs"]
mod tests;
