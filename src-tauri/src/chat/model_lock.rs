//! Which place on the operation lock a model load takes (spec 068 T012, research R2).
//!
//! Every running turn holds the `ActiveSession` it started with, so a load that only changes what
//! the *next* answer of a conversation uses touches none of them: a provider's model (the adapter is
//! a thin client), a delegate, or a local model whose engine is already in memory. Such a load takes
//! a shared place beside the turns, and the person can open or pick a cloud model in one conversation
//! while another one answers. A load that puts a local engine into memory takes the lock exclusively,
//! as before: it is refused while any turn runs, since the engine is one for all of them.

use tokio::sync::{OwnedRwLockReadGuard, OwnedRwLockWriteGuard};

use super::session::ChatState;
use super::turn_control::in_progress;
use crate::error::Result;

/// The place a model load holds on the operation lock until it ends.
pub enum LoadGuard {
    /// Beside the running turns; a vault transition or an engine load waits for it.
    Shared(OwnedRwLockReadGuard<()>),
    /// Alone: no turn runs, no other operation runs.
    Exclusive(OwnedRwLockWriteGuard<()>),
}

impl LoadGuard {
    /// Whether the load runs alone.
    pub fn is_exclusive(&self) -> bool {
        matches!(self, LoadGuard::Exclusive(_))
    }
}

impl ChatState {
    /// Takes the place a load of `model_id` needs: shared for a provider's model (`<provider>:<id>`)
    /// and for a local model that is in memory, exclusive for a local model that has to be loaded.
    /// Refused while an exclusive operation runs, and, for an engine load, while any turn runs.
    pub fn acquire_for_load(&self, model_id: &str) -> Result<LoadGuard> {
        let shared = self
            .operation
            .clone()
            .try_read_owned()
            .map_err(|_| in_progress())?;
        // Looked up under the shared place: an activation that would unload the engine takes the lock
        // exclusively and is refused while this guard lives.
        if model_id.contains(':') || self.session_for_local_model(model_id).is_some() {
            return Ok(LoadGuard::Shared(shared));
        }
        drop(shared);
        self.acquire_operation().map(LoadGuard::Exclusive)
    }
}
