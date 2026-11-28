//! Starting, asking about and ending turns on [`ChatState`] (spec 068 research R1).
//!
//! The registry itself is [`TurnSlots`](super::turn_slots::TurnSlots); this file is the part that
//! needs the rest of the chat state: the operation lock, and the approval and question maps an abort
//! empties.

use std::collections::HashSet;
use std::time::Duration;

use tokio::sync::OwnedRwLockReadGuard;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use super::session::ChatState;
use super::turn_slots::{Taken, TurnSlotGuard};
use crate::error::{HaexError, Result};

/// What a running turn holds: a shared place on the operation lock and its slot. Both end when it
/// drops, whichever way the turn ended.
pub struct TurnPermit {
    _operation: OwnedRwLockReadGuard<()>,
    slot: TurnSlotGuard,
}

impl TurnPermit {
    /// The id of the turn (the id of its assistant message).
    pub fn turn_id(&self) -> Uuid {
        self.slot.turn_id()
    }
}

fn in_progress() -> HaexError {
    HaexError::InvalidInput {
        reason: "a chat or vault operation is still in progress".into(),
    }
}

impl ChatState {
    /// Admits a turn: a shared place on the operation lock (refused while a model load, a vault
    /// transition or a model job's loan holds it) and a slot (refused while the conversation has a
    /// turn, or while a send of the same turn is still running). A refused turn changes nothing of
    /// the turns already running, so a retried send never replaces the live turn's token.
    pub fn begin_turn(
        &self,
        turn_id: Uuid,
        thread_id: Option<Uuid>,
        cancel: CancellationToken,
    ) -> Result<TurnPermit> {
        let operation = self
            .operation
            .clone()
            .try_read_owned()
            .map_err(|_| in_progress())?;
        let slot = self
            .turns
            .begin(turn_id, thread_id, cancel)
            .map_err(|_| in_progress())?;
        Ok(TurnPermit {
            _operation: operation,
            slot,
        })
    }

    /// Whether a model load, a vault transition or a model job's loan holds the operation lock right
    /// now. Turns do not count: they run beside each other.
    pub fn operation_busy(&self) -> bool {
        self.operation.try_read().is_err()
    }

    /// Whether a send to `thread_id` (`None`: a new conversation) has to wait: an exclusive
    /// operation is running, or the conversation has a turn. The answer can be stale a moment later;
    /// [`ChatState::begin_turn`] stays the real check.
    pub fn busy_for(&self, thread_id: Option<Uuid>) -> bool {
        self.operation_busy() || thread_id.is_some_and(|thread| self.turns.thread_busy(thread))
    }

    /// [`ChatState::wait_until_free_for`] for a send that names no conversation.
    pub async fn wait_until_free(&self, max: Duration) {
        self.wait_until_free_for(None, max).await;
    }

    /// Waits until [`ChatState::busy_for`] says no, at most `max`. A typed message sent while a
    /// spoken round or a model load ends waits instead of failing (operator 2026-11-03).
    pub async fn wait_until_free_for(&self, thread_id: Option<Uuid>, max: Duration) {
        const POLL: Duration = Duration::from_millis(100);
        let mut waited = Duration::ZERO;
        while self.busy_for(thread_id) && waited < max {
            tokio::time::sleep(POLL).await;
            waited += POLL;
        }
    }

    /// Ends one turn: its stream, its token and the approvals and questions it waits on. The other
    /// turns are untouched. `false` when no such turn runs (idempotent).
    pub fn abort_turn_by_id(&self, turn_id: Uuid) -> bool {
        match self.turns.take_for_abort(turn_id) {
            Some(taken) => {
                self.end_turn(taken);
                true
            }
            None => false,
        }
    }

    /// Ends the turn running in `thread_id`, if any.
    pub fn abort_turn_of_thread(&self, thread_id: Uuid) -> bool {
        match self.turns.take_for_thread_abort(thread_id) {
            Some(taken) => {
                self.end_turn(taken);
                true
            }
            None => false,
        }
    }

    /// Ends every turn and drops every open approval and question, also those no slot owns. The
    /// vault close and a stop of the voice assistant use it.
    pub fn abort_all_turns(&self) {
        for taken in self.turns.take_all_for_abort() {
            self.end_turn(taken);
        }
        let mut pending = self
            .pending_tool_approvals
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let mut cancelled = self
            .cancelled_tool_approvals
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        // Dropping every pending sender resolves its receiver with an `Err`, which the turn loop
        // treats as cancelled rather than denied (distinct from a `respond_tool_permission` deny).
        cancelled.extend(pending.keys().copied());
        pending.clear();
        drop((pending, cancelled));
        self.pending_choices.cancel_all();
    }

    fn end_turn(&self, taken: Taken) {
        if let Some(generation) = taken.generation {
            generation.abort();
        }
        // The turn's own cooperative signal: `chat/turn/step.rs` races every step against it, so this
        // is what actually stops the stream promptly (T032); CLI and MCP tools race their own work
        // against it too and tear it down on the spot.
        taken.cancel.cancel();
        self.drop_prompts(&taken.open_prompts);
    }

    /// Drops the approvals and questions in `ids` that are still open, and remembers them as
    /// cancelled so a late answer is harmless. An id that was answered meanwhile is left alone.
    fn drop_prompts(&self, ids: &HashSet<Uuid>) {
        if ids.is_empty() {
            return;
        }
        {
            let mut pending = self
                .pending_tool_approvals
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            let mut cancelled = self
                .cancelled_tool_approvals
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            for id in ids {
                if pending.remove(id).is_some() {
                    cancelled.insert(*id);
                }
            }
        }
        self.pending_choices.cancel_ids(ids);
    }
}

#[cfg(test)]
#[path = "turn_control_tests.rs"]
mod tests;
