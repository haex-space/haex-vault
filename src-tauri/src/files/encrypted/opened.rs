//! Copies of files of encrypted folders for the system's apps (spec 048 FR-028, research R10): the
//! window shows a notice first (a decrypted copy lands on the device, and the other app may keep
//! it), unless the user chose "Nicht mehr fragen" for that folder on this device (`open_ack`).
//! Each copy lies in a folder of its own under `<AppCache>/files-opened-encrypted/`, never under
//! `files-opened` of spec 044; the whole place goes when the vault closes and, after a crash, when
//! haex-vault starts again.

use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use sha2::{Digest, Sha256};
use tokio::{
    io::AsyncWriteExt,
    sync::{OwnedRwLockReadGuard, RwLock},
};

use super::cache;
use super::damaged;
use super::format::encode_id;
use super::ops::file_at;
use super::route::{in_encrypted, vault_error, Located};
use super::source::{EncryptedFile, WINDOW};
use super::tree::facts;
use crate::files::storage_source::StorageFiles;
use crate::files::{FilesError, FilesErrorCode};

/// The folder below the app cache that holds the copies.
pub const FOLDER: &str = "files-opened-encrypted";

/// Coordinates decrypted copies with the vault-close sweep.
pub struct OpenedCopies {
    root: PathBuf,
    closing: AtomicBool,
    active: Arc<RwLock<()>>,
}

impl OpenedCopies {
    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            closing: AtomicBool::new(false),
            active: Arc::new(RwLock::new(())),
        }
    }

    /// Starts a copy only while the vault is open; the lease lasts through the output write.
    async fn begin_copy(&self) -> Result<OwnedRwLockReadGuard<()>, FilesError> {
        let guard = self.active.clone().read_owned().await;
        if self.closing.load(Ordering::Acquire) {
            return Err(FilesError::new(
                FilesErrorCode::NoAccess,
                "the vault is closing",
            ));
        }
        Ok(guard)
    }

    /// Stops new copies, waits for existing ones, then removes all output.
    pub async fn close_and_sweep(&self) -> io::Result<()> {
        self.closing.store(true, Ordering::Release);
        let _active = self.active.clone().write_owned().await;
        let root = self.root.clone();
        tokio::task::spawn_blocking(move || sweep(&root))
            .await
            .map_err(io::Error::other)?
    }
}

/// Removes a partial output if the copy future is cancelled or exits early.
struct PartialCopy(PathBuf);

impl PartialCopy {
    async fn remove(&mut self) {
        match remove_copy(&self.0).await {
            Ok(()) => self.0.clear(),
            Err(error) => {
                log::warn!("files: a partial encrypted-file copy could not be removed: {error}")
            }
        }
    }
}

impl Drop for PartialCopy {
    fn drop(&mut self) {
        if self.0.as_os_str().is_empty() {
            return;
        }
        let dir = std::mem::take(&mut self.0);
        if let Ok(runtime) = tokio::runtime::Handle::try_current() {
            runtime.spawn(async move {
                if let Err(error) = remove_copy(&dir).await {
                    log::warn!(
                        "files: a partial encrypted-file copy could not be removed: {error}"
                    );
                }
            });
        } else if let Err(error) = std::fs::remove_dir_all(dir) {
            if error.kind() != io::ErrorKind::NotFound {
                log::warn!("files: a partial encrypted-file copy could not be removed: {error}");
            }
        }
    }
}

async fn remove_copy(dir: &Path) -> io::Result<()> {
    match tokio::fs::remove_dir_all(dir).await {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        other => other,
    }
}

/// The answer while the notice was not confirmed.
pub fn needs_notice() -> FilesError {
    FilesError::new(
        FilesErrorCode::NeedsNotice,
        "opening this file with another app needs the notice first",
    )
}

/// The decrypted copy of the file at `at` in a new folder below `root`, for the system's app.
/// Without `confirmed` the notice must have been answered with "Nicht mehr fragen" for its folder
/// before; `confirmed` with `dont_ask_again` remembers that answer.
pub async fn copy_for_app(
    files: &StorageFiles,
    at: &Located,
    copies: &OpenedCopies,
    confirmed: bool,
    dont_ask_again: bool,
) -> Result<PathBuf, FilesError> {
    copy_for_app_named(files, at, copies, None, confirmed, dont_ask_again).await
}

/// Like [`copy_for_app`], with a caller-selected, already cleaned output filename.
pub async fn copy_for_app_named(
    files: &StorageFiles,
    at: &Located,
    copies: &OpenedCopies,
    output_name: Option<&str>,
    confirmed: bool,
    dont_ask_again: bool,
) -> Result<PathBuf, FilesError> {
    let scope = files.scope().ok_or_else(in_encrypted)?;
    let (folder, version) = file_at(files, at).await?;
    let (storage, folder_id) = (scope.storage_id.clone(), encode_id(&folder.fid));
    let db = scope.db()?;
    if !confirmed {
        let acked = db
            .read(move |q| cache::open_ack(q, &storage, &folder_id))
            .await
            .map_err(vault_error)?;
        if !acked {
            return Err(needs_notice());
        }
    } else if dont_ask_again {
        let head = format!("{}h", folder.prefix);
        db.write(move |tx| cache::set_open_ack(tx, &storage, &folder_id, &head))
            .await
            .map_err(vault_error)?;
    }
    let expected = facts(&version).ok_or_else(damaged)?.sha256;
    let name = match output_name {
        Some(name) => name,
        None => at
            .rest
            .last()
            .ok_or_else(|| FilesError::invalid_path("not a file"))?,
    };
    let file = EncryptedFile::open(files, folder, &version).await?;
    let _lease = copies.begin_copy().await?;
    let dir = copies.root.join(uuid::Uuid::new_v4().to_string());
    let target = dir.join(name);
    let mut partial = PartialCopy(dir.clone());
    let written = write_out(&file, &dir, &target, &expected).await;
    if let Err(error) = written {
        partial.remove().await;
        return Err(error);
    }
    partial.0.clear();
    Ok(target)
}

/// Writes the plaintext of `file` to `target`, window by window, and checks it whole.
async fn write_out(
    file: &EncryptedFile,
    dir: &Path,
    target: &Path,
    expected: &[u8; 32],
) -> Result<(), FilesError> {
    let io = |_| FilesError::new(FilesErrorCode::NoAccess, "the copy could not be written");
    tokio::fs::create_dir_all(dir).await.map_err(io)?;
    let mut out = tokio::fs::File::create(target).await.map_err(io)?;
    let mut sha = Sha256::new();
    let mut at = 0;
    while at < file.size() {
        let end = file.size().min(at + WINDOW);
        let plain = file.read(at, end).await?;
        sha.update(&plain);
        out.write_all(&plain).await.map_err(io)?;
        at = end;
    }
    out.sync_all().await.map_err(io)?;
    if sha.finalize().as_slice() != expected {
        return Err(damaged());
    }
    Ok(())
}

/// Removes every copy below `root`: the vault closes, or haex-vault starts after a crash.
pub fn sweep(root: &Path) -> io::Result<()> {
    // ponytail: a second haex-vault process that starts removes the copies the first one opened; a
    // folder per process with a lock would keep them, once more than one process runs at a time.
    match std::fs::remove_dir_all(root) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        other => other,
    }
}

#[cfg(test)]
#[path = "opened_tests.rs"]
mod tests;
