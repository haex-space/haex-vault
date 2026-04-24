//! Per-vault advisory file lock.
//!
//! Prevents the same vault DB from being opened by two processes
//! concurrently, which would corrupt CRDT HLC clocks and cause SQLite WAL
//! races. Replaces the old app-wide `tauri-plugin-single-instance` lock:
//! different vaults remain independently openable, only the same one is
//! mutually exclusive.
//!
//! Uses `fs2`'s cross-platform advisory `try_lock_exclusive()` — flock on
//! Unix, LockFileEx on Windows. The lock is tied to the `File` handle;
//! closing the file (on Drop) releases it automatically. On abrupt process
//! termination the OS drops the handle for us so stale locks cannot strand
//! a vault permanently unreachable.

use std::fs::{File, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};

use fs2::FileExt;

/// Suffix appended to the vault DB path for the lock file.
/// Kept separate from the DB so SQLite's own WAL/SHM files are not confused.
const LOCK_SUFFIX: &str = ".lock";

/// Holds an acquired exclusive advisory lock on a vault's `.lock` file for
/// the lifetime of this handle. Dropping it releases the lock.
#[derive(Debug)]
pub struct VaultLock {
    /// Keeps the OS-level lock alive. Must stay owned for the whole duration
    /// of the open vault — closing this handle releases the advisory lock.
    handle: File,
    lock_path: PathBuf,
}

impl VaultLock {
    /// Try to acquire an exclusive advisory lock on `<vault_path>.lock`.
    ///
    /// Returns `Ok(VaultLock)` if we got the lock, or an error if another
    /// process already holds it (or we cannot create the lock file at all).
    ///
    /// The lock file is created on first acquire and kept around — this is
    /// fine because advisory locks are scoped to open file handles, not the
    /// file's content.
    pub fn try_acquire(vault_path: &Path) -> Result<Self, VaultLockError> {
        let lock_path = lock_path_for(vault_path);

        // Create the parent dir if missing (mirrors create_encrypted_database).
        if let Some(parent) = lock_path.parent() {
            std::fs::create_dir_all(parent).map_err(|source| VaultLockError::Io {
                path: parent.display().to_string(),
                source,
            })?;
        }

        let handle = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .truncate(false)
            .open(&lock_path)
            .map_err(|source| VaultLockError::Io {
                path: lock_path.display().to_string(),
                source,
            })?;

        handle
            .try_lock_exclusive()
            .map_err(|source| VaultLockError::AlreadyHeld {
                path: lock_path.display().to_string(),
                source,
            })?;

        Ok(Self { handle, lock_path })
    }
}

impl Drop for VaultLock {
    fn drop(&mut self) {
        // Best-effort release. The OS would also release on handle close,
        // but doing it explicitly lets us catch double-drop bugs in tests.
        let _ = fs2::FileExt::unlock(&self.handle);
        // Do NOT delete the lock file: deletion creates a TOCTOU race where a
        // concurrent acquirer may recreate+open the file between our unlock
        // and remove, ending up with two live handles both holding exclusive
        // locks. The file is cheap (0 bytes on disk) so we leave it in place.
        let _ = &self.lock_path; // keep field live (linter)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum VaultLockError {
    #[error("Vault is already open in another instance (lock at '{path}')")]
    AlreadyHeld { path: String, source: io::Error },

    #[error("Failed to prepare lock file at '{path}': {source}")]
    Io { path: String, source: io::Error },
}

fn lock_path_for(vault_path: &Path) -> PathBuf {
    let mut lock = vault_path.to_path_buf();
    // Extend the existing filename rather than replace the extension so the
    // dotted vault name + `.db.lock` is unambiguous next to `.db-shm`/`.db-wal`.
    let filename = lock
        .file_name()
        .map(|s| s.to_os_string())
        .unwrap_or_default();
    let mut extended = filename;
    extended.push(LOCK_SUFFIX);
    lock.set_file_name(extended);
    lock
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn second_acquire_same_path_fails() {
        let dir = tempfile::tempdir().expect("tempdir");
        let vault = dir.path().join("my.db");

        let first = VaultLock::try_acquire(&vault).expect("first acquire");
        let second = VaultLock::try_acquire(&vault);
        assert!(matches!(second, Err(VaultLockError::AlreadyHeld { .. })));

        drop(first);

        // After release, acquisition works again — confirms the lock isn't
        // stranded by a lingering file.
        let _third = VaultLock::try_acquire(&vault).expect("post-release acquire");
    }

    #[test]
    fn different_vaults_can_lock_independently() {
        let dir = tempfile::tempdir().expect("tempdir");
        let a = VaultLock::try_acquire(&dir.path().join("a.db")).expect("a");
        let b = VaultLock::try_acquire(&dir.path().join("b.db")).expect("b");
        drop((a, b));
    }

    #[test]
    fn lock_path_for_appends_suffix() {
        let p = lock_path_for(Path::new("/vaults/my.db"));
        assert_eq!(p, Path::new("/vaults/my.db.lock"));
    }
}
