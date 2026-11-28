//! The turns that run right now, one slot each (spec 068 research R1, ADR 0021).
//!
//! A turn used to own the whole chat: one lock, one generation handle, one cancellation token. Now
//! every running turn registers a [`TurnSlot`] here, and what belonged to "the" turn belongs to its
//! slot: the cancellation token, the abort handle of the stream in flight, and the open approvals and
//! questions it is waiting on. Aborting one turn cancels only that slot's token and drops only the
//! approvals and questions that slot opened, so the other conversations go on.
//!
//! The turn id is the id of the assistant message the turn writes: it is derived from the send's
//! idempotency key before anything is stored, every event of the turn already carries it
//! (`messageId` / `assistantMessageId`), and a retried send of the same key finds its slot taken.
//! One conversation has at most one turn at a time; two conversations can both run.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::adapters::AbortHandle;

/// What one running turn owns.
struct TurnSlot {
    thread_id: Option<Uuid>,
    /// The orchestrator task this turn works for (spec 068 PR C); `None` for a typed or spoken turn.
    task_id: Option<Uuid>,
    /// The composite id of the model the turn runs on; read once the session is known.
    model_id: Option<String>,
    cancel: CancellationToken,
    /// The stream of the step in flight; replaced with every step.
    generation: Option<AbortHandle>,
    /// The ids of the approvals and questions this turn waits on.
    open_prompts: HashSet<Uuid>,
}

/// What an abort takes out of a slot, so the caller can end it without holding the registry lock.
pub struct Taken {
    pub turn_id: Uuid,
    pub cancel: CancellationToken,
    pub generation: Option<AbortHandle>,
    pub open_prompts: HashSet<Uuid>,
}

/// A running turn as the window and the tools may ask about it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunningTurn {
    pub turn_id: Uuid,
    pub thread_id: Option<Uuid>,
    pub task_id: Option<Uuid>,
    pub model_id: Option<String>,
}

/// Why a turn got no slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refused {
    /// The conversation already has a turn running.
    ThreadBusy,
    /// This turn id is registered: a retried send of a turn that is still running.
    AlreadyRunning,
    /// The vault close has stopped new turns.
    Closing,
}

/// The registry of running turns. Cheap to clone through its `Arc`.
pub struct TurnSlots {
    slots: Mutex<HashMap<Uuid, TurnSlot>>,
    accepting: std::sync::atomic::AtomicBool,
}

impl Default for TurnSlots {
    fn default() -> Self {
        Self {
            slots: Mutex::new(HashMap::new()),
            accepting: std::sync::atomic::AtomicBool::new(true),
        }
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

impl TurnSlots {
    /// Registers a turn. Refused while the conversation has a turn or the id is taken; nothing of
    /// the turn already running is touched then.
    pub fn begin(
        self: &Arc<Self>,
        turn_id: Uuid,
        thread_id: Option<Uuid>,
        cancel: CancellationToken,
    ) -> Result<TurnSlotGuard, Refused> {
        if !self.accepting.load(std::sync::atomic::Ordering::Acquire) {
            return Err(Refused::Closing);
        }
        let mut slots = lock(&self.slots);
        if !self.accepting.load(std::sync::atomic::Ordering::Acquire) {
            return Err(Refused::Closing);
        }
        if slots.contains_key(&turn_id) {
            return Err(Refused::AlreadyRunning);
        }
        if let Some(thread) = thread_id {
            if slots.values().any(|slot| slot.thread_id == Some(thread)) {
                return Err(Refused::ThreadBusy);
            }
        }
        slots.insert(
            turn_id,
            TurnSlot {
                thread_id,
                task_id: None,
                model_id: None,
                cancel,
                generation: None,
                open_prompts: HashSet::new(),
            },
        );
        Ok(TurnSlotGuard {
            slots: Arc::clone(self),
            turn_id,
        })
    }

    /// Whether `thread_id` has a turn running.
    pub fn thread_busy(&self, thread_id: Uuid) -> bool {
        lock(&self.slots)
            .values()
            .any(|slot| slot.thread_id == Some(thread_id))
    }

    /// The turn running in `thread_id`, if any.
    pub fn turn_of_thread(&self, thread_id: Uuid) -> Option<Uuid> {
        lock(&self.slots)
            .iter()
            .find(|(_, slot)| slot.thread_id == Some(thread_id))
            .map(|(id, _)| *id)
    }

    /// Every running turn.
    pub fn running(&self) -> Vec<RunningTurn> {
        lock(&self.slots)
            .iter()
            .map(|(id, slot)| RunningTurn {
                turn_id: *id,
                thread_id: slot.thread_id,
                task_id: slot.task_id,
                model_id: slot.model_id.clone(),
            })
            .collect()
    }

    /// Whether any turn runs.
    pub fn any_running(&self) -> bool {
        !lock(&self.slots).is_empty()
    }

    /// Names the conversation of a turn that started a new one: it is known once the send is stored.
    pub fn set_thread(&self, turn_id: Uuid, thread_id: Uuid) -> Result<(), Refused> {
        let mut slots = lock(&self.slots);
        let Some(slot) = slots.get(&turn_id) else {
            return Err(Refused::AlreadyRunning);
        };
        if slots
            .iter()
            .any(|(id, other)| *id != turn_id && other.thread_id == Some(thread_id))
        {
            return Err(Refused::ThreadBusy);
        }
        slots
            .get_mut(&turn_id)
            .expect("slot checked above")
            .thread_id = Some(thread_id);
        Ok(())
    }

    /// Names the model a turn runs on.
    pub fn set_model(&self, turn_id: Uuid, model_id: &str) {
        if let Some(slot) = lock(&self.slots).get_mut(&turn_id) {
            slot.model_id = Some(model_id.to_owned());
        }
    }

    /// Keeps the abort handle of the stream the turn just started, replacing the one of the step
    /// before.
    pub fn set_generation(&self, turn_id: Uuid, handle: AbortHandle) {
        if let Some(slot) = lock(&self.slots).get_mut(&turn_id) {
            slot.generation = Some(handle);
        }
    }

    /// Records that `prompt_id` (an approval or a question) is open for `turn_id`.
    pub fn own_prompt(&self, turn_id: Uuid, prompt_id: Uuid) {
        if let Some(slot) = lock(&self.slots).get_mut(&turn_id) {
            slot.open_prompts.insert(prompt_id);
        }
    }

    /// Records that `prompt_id` is open for the turn running in `thread_id`. A delegate asks for its
    /// approvals from inside its stream, where only the conversation is known.
    pub fn own_prompt_of_thread(&self, thread_id: Uuid, prompt_id: Uuid) {
        let mut slots = lock(&self.slots);
        if let Some(slot) = slots
            .values_mut()
            .find(|slot| slot.thread_id == Some(thread_id))
        {
            slot.open_prompts.insert(prompt_id);
        }
    }

    /// Records that `prompt_id` is answered or given up.
    pub fn disown_prompt(&self, turn_id: Uuid, prompt_id: Uuid) {
        if let Some(slot) = lock(&self.slots).get_mut(&turn_id) {
            slot.open_prompts.remove(&prompt_id);
        }
    }

    /// Takes what is needed to end one turn. The slot stays until its guard drops, so a turn that
    /// was told to stop still counts as running until it has unwound.
    pub fn take_for_abort(&self, turn_id: Uuid) -> Option<Taken> {
        let mut slots = lock(&self.slots);
        let slot = slots.get_mut(&turn_id)?;
        Some(take_slot(turn_id, slot))
    }

    /// Takes the turn of `thread_id` under the same lock that finds it. This keeps a stop from
    /// missing a new turn if the old one ends and another starts between lookup and cancellation.
    pub fn take_for_thread_abort(&self, thread_id: Uuid) -> Option<Taken> {
        let mut slots = lock(&self.slots);
        let (turn_id, slot) = slots
            .iter_mut()
            .find(|(_, slot)| slot.thread_id == Some(thread_id))?;
        Some(take_slot(*turn_id, slot))
    }

    /// [`Self::take_for_abort`] for every running turn.
    pub fn take_all_for_abort(&self) -> Vec<Taken> {
        let ids: Vec<Uuid> = lock(&self.slots).keys().copied().collect();
        ids.into_iter()
            .filter_map(|id| self.take_for_abort(id))
            .collect()
    }

    /// Atomically refuses future turns and takes every current turn for a vault close.
    pub fn stop_accepting_and_take_all(&self) -> Vec<Taken> {
        self.accepting
            .store(false, std::sync::atomic::Ordering::Release);
        let mut slots = lock(&self.slots);
        slots
            .iter_mut()
            .map(|(id, slot)| take_slot(*id, slot))
            .collect()
    }

    /// Forgets every slot (the vault closes); a guard that drops later finds nothing to remove.
    pub fn clear(&self) -> Vec<AbortHandle> {
        let mut slots = lock(&self.slots);
        slots
            .drain()
            .filter_map(|(_, slot)| slot.generation)
            .collect()
    }

    fn remove(&self, turn_id: Uuid) {
        let removed = lock(&self.slots).remove(&turn_id);
        // Dropped after the guard is gone, so no drop of a handle runs while the lock is held.
        drop(removed);
    }
}

fn take_slot(turn_id: Uuid, slot: &mut TurnSlot) -> Taken {
    Taken {
        turn_id,
        cancel: slot.cancel.clone(),
        generation: slot.generation.take(),
        open_prompts: std::mem::take(&mut slot.open_prompts),
    }
}

/// Keeps a turn's slot registered; dropping it ends the slot, whichever way the turn ended.
pub struct TurnSlotGuard {
    slots: Arc<TurnSlots>,
    turn_id: Uuid,
}

impl TurnSlotGuard {
    pub fn turn_id(&self) -> Uuid {
        self.turn_id
    }
}

impl Drop for TurnSlotGuard {
    fn drop(&mut self) {
        self.slots.remove(self.turn_id);
    }
}

#[cfg(test)]
#[path = "turn_slots_tests.rs"]
mod tests;
