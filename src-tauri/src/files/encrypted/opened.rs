//! Copies of files of encrypted folders for the system's apps (spec 048 FR-028, research R10): the
//! window shows a notice first (a decrypted copy lands on the device, and the other app may keep
//! it), unless the user chose "Nicht mehr fragen" for that folder on this device (`open_ack`).
//! Each copy lies in a folder of its own under `<AppCache>/files-opened-encrypted/`, never under
//! `files-opened` of spec 044; the whole place goes when the vault closes and, after a crash, when
//! holzi starts again.

use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;

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
    root: &Path,
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
    let name = at
        .rest
        .last()
        .ok_or_else(|| FilesError::invalid_path("not a file"))?;
    let file = EncryptedFile::open(files, folder, &version).await?;
    let dir = root.join(uuid::Uuid::new_v4().to_string());
    let target = dir.join(name);
    let written = write_out(&file, &dir, &target, &expected).await;
    if let Err(error) = written {
        let _ = tokio::fs::remove_dir_all(&dir).await;
        return Err(error);
    }
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

/// Removes every copy below `root`: the vault closes, or holzi starts after a crash.
pub fn sweep(root: &Path) {
    // ponytail: a second holzi process that starts removes the copies the first one opened; a
    // folder per process with a lock would keep them, once more than one process runs at a time.
    let _ = std::fs::remove_dir_all(root);
}

#[cfg(test)]
#[path = "opened_tests.rs"]
mod tests;
