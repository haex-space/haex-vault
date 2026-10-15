//! Row persistence for one turn, plus the event helpers every terminal
//! path shares.
//!
//! Split out of `chat/turn.rs` (2026-09-16); see
//! `docs/plans/2026-09-16-turn-module-split-design.md`.

use serde::Serialize;
use uuid::Uuid;

use crate::adapters::cli_delegate::autonomy::AutonomyMode;
use crate::chat::events::{
    MessageCompleteEvent, MessageErrorEvent, TurnCompleteEvent, EVENT_CHAT_MESSAGE_COMPLETE,
    EVENT_CHAT_MESSAGE_ERROR, EVENT_CHAT_TURN_COMPLETE,
};
use crate::storage::chat_messages::{self as msg_store, ChatMessage, FinishReason, MessageRole};
use crate::storage::chat_threads as thread_store;

use super::{now_ms, TurnRunner};

/// Persists one row. Used for every row except the turn's terminal
/// assistant row, which also needs `chat_threads` updated atomically
/// (see [`persist_final_message`]).
pub(super) async fn persist_message(
    db: &crate::vault_gate::VaultDb,
    msg: ChatMessage,
) -> std::result::Result<(), String> {
    db.write(move |tx| msg_store::insert_message(tx, &msg).map(|_| ()))
        .await
        .map_err(|e| format!("persist failed: {e}"))
}

/// Persists one tool call together with its result so a failed write cannot
/// leave an unmatched call row in the conversation history.
pub(super) async fn persist_tool_pair(
    db: &crate::vault_gate::VaultDb,
    call: ChatMessage,
    result: ChatMessage,
) -> std::result::Result<(), String> {
    db.write(move |tx| {
        msg_store::insert_message(tx, &call)?;
        msg_store::insert_message(tx, &result)?;
        Ok(())
    })
    .await
    .map_err(|e| format!("persist failed: {e}"))
}

/// Persists the turn's terminal assistant row and updates `chat_threads`
/// in the same transaction — mirrors the pre-tool-loop behavior where
/// both happened atomically together.
pub(super) async fn persist_final_message(
    db: &crate::vault_gate::VaultDb,
    msg: ChatMessage,
    provider_id: Option<Uuid>,
    model_id: String,
) -> std::result::Result<(), String> {
    let thread_id = msg.thread_id;
    let now = msg.created_at;
    db.write(move |tx| {
        let thread = thread_store::get_thread(tx, thread_id)?
            .ok_or(haex_crdt::rusqlite::Error::QueryReturnedNoRows)?;
        msg_store::insert_message(tx, &msg)?;
        thread_store::update_thread(
            tx,
            thread_id,
            &thread.title,
            provider_id,
            Some(&model_id),
            now,
        )?;
        Ok(())
    })
    .await
    .map_err(|e| format!("persist failed: {e}"))
}

pub(super) fn empty_tool_message(
    id: Uuid,
    thread_id: Uuid,
    parent_id: Option<Uuid>,
) -> ChatMessage {
    ChatMessage {
        id,
        thread_id,
        parent_id,
        role: MessageRole::User, // overwritten by every caller
        content: String::new(),
        provider_id: None,
        model_id: None,
        prompt_tokens: None,
        completion_tokens: None,
        finish_reason: None,
        created_at: now_ms(),
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

/// The label persisted on a turn's assistant/cancellation row (spec
/// 009-autonomous-delegate-mode FR-005): `Some` only for a non-default
/// mode. `Standard` stays `None` rather than being spelled out —
/// indistinguishable in practice from a non-delegate turn (which never
/// sets a non-default mode at all), and carries no "this ran unsupervised"
/// information FR-005 cares about either way. A free function (not a
/// `TurnRunner` method) so it is unit-testable without constructing one.
pub(super) fn autonomy_mode_label(mode: AutonomyMode) -> Option<String> {
    (mode != AutonomyMode::Standard).then(|| mode.as_str().to_string())
}

impl TurnRunner<'_> {
    /// Emits one chat event. Every payload type in `chat::events` is a
    /// plain derive-`Serialize` struct of owned scalars and `Value`s, so
    /// `to_value` cannot fail; the panic names the type should that ever
    /// stop being true.
    pub(super) fn emit_event<T: Serialize>(&mut self, name: &'static str, payload: T) {
        let value = serde_json::to_value(payload)
            .unwrap_or_else(|e| panic!("{} must serialize: {e}", std::any::type_name::<T>()));
        (self.emit)(name, value);
    }

    /// Ends the turn after a persistence failure: the
    /// `chat-message-error` + `chat-turn-complete{Error}` pair that every
    /// such failure emitted inline before this helper existed. The
    /// terminal event deliberately carries no `assistant_message_id` —
    /// nothing was written.
    pub(super) fn fail(&mut self, reason: String) {
        let (thread_id, message_id) = (self.thread_id, self.assistant_message_id);
        self.emit_event(
            EVENT_CHAT_MESSAGE_ERROR,
            MessageErrorEvent {
                message_id,
                thread_id,
                reason,
                kind: None,
            },
        );
        self.emit_event(
            EVENT_CHAT_TURN_COMPLETE,
            TurnCompleteEvent {
                thread_id,
                assistant_message_id: None,
                finish_reason: FinishReason::Error,
            },
        );
    }

    /// Advances the turn's logical clock past both wall-clock time and the
    /// last row it issued, and returns the new value.
    ///
    /// SQLite orders rows by `created_at, id`; UUIDv4 is random, so rows
    /// created in one millisecond must receive distinct logical timestamps
    /// to keep each call immediately before its result. The `max(now_ms())`
    /// keeps the clock honest across slower rounds; the `+1` keeps a fast
    /// round from colliding with the row it just wrote — without it,
    /// `(created_at, id)` would fall back to comparing random UUIDs and
    /// could sort a terminal row before the round it concludes.
    pub(super) fn bump_created_at(&mut self) -> i64 {
        self.next_created_at = self.next_created_at.max(now_ms()).saturating_add(1);
        self.next_created_at
    }

    /// Ends a turn as `Cancelled` from inside a tool round, persisting a
    /// terminal assistant row with no content (spec.md FR-009–FR-011).
    /// Mirrors the per-step event pair the plain-generation cancellation
    /// path already emits (`chat-message-complete` then
    /// `chat-turn-complete`, not `chat-message-error` — cancelling is not
    /// itself an error).
    pub(super) async fn end_cancelled(&mut self) {
        let created_at = self.bump_created_at();
        let (thread_id, message_id) = (self.thread_id, self.assistant_message_id);
        let autonomy_mode = autonomy_mode_label(self.request.autonomy_mode);
        let final_msg = ChatMessage {
            role: MessageRole::Assistant,
            finish_reason: Some(FinishReason::Cancelled),
            created_at,
            autonomy_mode,
            ..empty_tool_message(message_id, thread_id, Some(self.parent_id))
        };
        match persist_final_message(
            self.db,
            final_msg,
            self.session.provider_id,
            self.session.model_id.clone(),
        )
        .await
        {
            Ok(()) => {
                self.emit_event(
                    EVENT_CHAT_MESSAGE_COMPLETE,
                    MessageCompleteEvent {
                        message_id,
                        thread_id,
                        prompt_tokens: None,
                        completion_tokens: None,
                        ttft_ms: None,
                    },
                );
                self.emit_event(
                    EVENT_CHAT_TURN_COMPLETE,
                    TurnCompleteEvent {
                        thread_id,
                        assistant_message_id: Some(message_id),
                        finish_reason: FinishReason::Cancelled,
                    },
                );
            }
            Err(reason) => self.fail(reason),
        }
    }
}

#[cfg(test)]
#[path = "persist_tests.rs"]
mod tests;
