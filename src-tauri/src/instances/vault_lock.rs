//! Whether a vault is open in another process (spec 084, research R5).
//!
//! `haex-crdt` holds an exclusive advisory lock on `<database>.lock` for as long as a vault is
//! open. Moving the files of a vault is only safe while nobody holds it, so the move asks here
//! first and keeps the lock it got until the last file is renamed.

use std::path::Path;

use haex_crdt::{DatabaseLock, DatabaseLockError};

/// What the lock of a vault says.
#[derive(Debug)]
pub enum Probe {
    /// Nobody has the vault open. The guard keeps it that way until it is dropped.
    Free(DatabaseLock),
    /// Another handle holds the vault, usually another holzi process.
    Held,
}

/// Tries the lock of the database `db_path` without waiting. An error is a lock file that cannot
/// be prepared (a read-only or missing folder); the caller treats it like a held vault and leaves
/// the files alone.
pub fn probe(db_path: &Path) -> Result<Probe, String> {
    match DatabaseLock::try_acquire(db_path) {
        Ok(lock) => Ok(Probe::Free(lock)),
        Err(DatabaseLockError::AlreadyHeld { .. }) => Ok(Probe::Held),
        Err(error @ DatabaseLockError::Io { .. }) => Err(error.to_string()),
    }
}

#[cfg(test)]
#[path = "vault_lock_tests.rs"]
mod tests;
