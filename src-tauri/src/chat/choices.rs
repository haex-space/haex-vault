//! Questions the tool round puts to the user (spec 046, contracts/choice-contract.md): an action
//! that could not resolve an input, or the agent's `ask_user`. The round parks on a oneshot per
//! question and emits `chat-choice-request`; the chat answers through `respond_choice`. The same
//! lifecycle as the tool approvals (`pending_tool_approvals`): no timeout, a cancelled turn drops
//! every open question, and a late answer to one of those is harmless.

use std::collections::{HashMap, HashSet};
use std::sync::{Mutex, MutexGuard, PoisonError};

use serde::Deserialize;
use tauri::State;
use tokio::sync::oneshot;
use uuid::Uuid;

use super::session::ChatState;
use crate::error::{HaexError, Result};

/// What the user answered.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ChoiceAnswer {
    /// One of the proposed options, by its value.
    Option { value: String },
    /// "Etwas anderes …": the user's own words.
    Text { text: String },
    /// The user declined to answer.
    Cancel,
}

/// The open questions of the running turn, and tombstones of the cancelled ones.
#[derive(Default)]
pub struct PendingChoices {
    pending: Mutex<HashMap<Uuid, oneshot::Sender<ChoiceAnswer>>>,
    /// Session-lifetime tombstones make late answers to cancelled questions harmless.
    cancelled: Mutex<HashSet<Uuid>>,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

impl PendingChoices {
    /// Opens a question; the round waits on the receiver.
    pub fn open(&self) -> (Uuid, oneshot::Receiver<ChoiceAnswer>) {
        let id = Uuid::new_v4();
        let (sender, receiver) = oneshot::channel();
        lock(&self.pending).insert(id, sender);
        (id, receiver)
    }

    /// Delivers the user's answer. An unknown id is an input error; one whose turn was cancelled
    /// is not.
    pub fn resolve(&self, id: Uuid, answer: ChoiceAnswer) -> Result<()> {
        let sender = lock(&self.pending).remove(&id);
        let Some(sender) = sender else {
            if lock(&self.cancelled).contains(&id) {
                return Ok(());
            }
            return Err(HaexError::InvalidInput {
                reason: format!("no pending choice request: {id}"),
            });
        };
        // The round may already be gone (its turn ended some other way).
        let _ = sender.send(answer);
        Ok(())
    }

    /// Gives up one question the round stopped waiting for.
    pub fn cancel(&self, id: Uuid) {
        lock(&self.pending).remove(&id);
        lock(&self.cancelled).insert(id);
    }

    /// Drops every open question (`abort_turn`); the waiting round sees its receiver fail.
    pub fn cancel_all(&self) {
        let mut pending = lock(&self.pending);
        lock(&self.cancelled).extend(pending.keys().copied());
        pending.clear();
    }

    /// Forgets everything when the vault closes.
    pub fn reset(&self) {
        lock(&self.pending).clear();
        lock(&self.cancelled).clear();
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RespondChoiceArgs {
    pub request_id: Uuid,
    pub answer: ChoiceAnswer,
}

/// Answers one open `chat-choice-request`. Needs an open vault: not on the gate's app-scoped list.
#[tauri::command]
pub async fn respond_choice(chat: State<'_, ChatState>, args: RespondChoiceArgs) -> Result<()> {
    chat.pending_choices.resolve(args.request_id, args.answer)
}

#[cfg(test)]
#[path = "choices_tests.rs"]
mod tests;
