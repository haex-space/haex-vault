//! One process announcing "I am here" over the app-local-data directory, so of several processes
//! starting around the same time only the first, momentarily alone one runs the startup orphan
//! cleanup (spec 013 US4, FR-026, SC-010). Independent of the vault gate: this tracks processes,
//! not vault sessions.

use std::fs::{File, OpenOptions, TryLockError};
use std::path::Path;

use crate::error::{HolziError, Result};

pub const PRESENCE_LOCK_FILE: &str = "presence.lock";

/// Held by the process for its whole life once [`ProcessPresence::announce`] returns: an exclusive
/// lock taken and released while briefly alone (running the startup cleanup), downgraded to the
/// shared lock this handle then keeps until it drops. The OS releases the lock when the process
/// ends or crashes, so a crashed process never blocks a later start's cleanup.
pub struct ProcessPresence {
    _file: File,
    alone: bool,
}

impl ProcessPresence {
    /// Opens `<dir>/presence.lock` for read and write (Windows refuses a lock on an append-only
    /// handle) and announces this process's presence:
    ///
    /// 1. tries an exclusive lock;
    /// 2. if it succeeds, this process is alone right now — runs `on_alone` (the startup cleanup)
    ///    while holding it, then downgrades: `unlock`, then `lock_shared`. **Invariant: a handle
    ///    that already holds a lock is never locked again** — `std` leaves a second lock on an
    ///    already-locked handle unspecified and possibly deadlocking, so there is no atomic
    ///    exclusive-to-shared downgrade, and none is needed: no process starts work before it holds
    ///    its own shared lock, so whoever sits between this `unlock` and `lock_shared` has nothing
    ///    to lose, and once every alive process holds a shared lock, any later exclusive attempt
    ///    fails until they are all gone;
    /// 3. if it fails, another process is still starting or cleaning up — waits on a blocking
    ///    shared lock instead and skips `on_alone` entirely.
    ///
    /// ponytail: cleanup only runs when the process announcing is alone; ceiling "leftovers of a
    /// crashed process stay on disk for as long as another process's presence overlaps it";
    /// upgrade path "per-item locks" (one lock per orphan instead of one for the whole directory).
    pub fn announce(dir: &Path, on_alone: impl FnOnce()) -> Result<Self> {
        std::fs::create_dir_all(dir).map_err(HolziError::from)?;
        let path = dir.join(PRESENCE_LOCK_FILE);
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&path)
            .map_err(HolziError::from)?;

        let alone = match file.try_lock() {
            Ok(()) => {
                on_alone();
                file.unlock().map_err(HolziError::from)?;
                file.lock_shared().map_err(HolziError::from)?;
                true
            }
            Err(TryLockError::WouldBlock) => {
                file.lock_shared().map_err(HolziError::from)?;
                false
            }
            Err(TryLockError::Error(e)) => return Err(HolziError::from(e)),
        };

        Ok(Self { _file: file, alone })
    }

    /// Whether this process took the exclusive lock in [`ProcessPresence::announce`], that is no
    /// other holzi process was running or starting then (spec 060 research R9: a link opened by a
    /// new process goes to the others' inbox only when there are others).
    pub fn was_alone(&self) -> bool {
        self.alone
    }
}

#[cfg(test)]
#[path = "presence_tests.rs"]
mod presence_tests;
