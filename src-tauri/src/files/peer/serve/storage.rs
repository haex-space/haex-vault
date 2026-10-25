//! Shares of a storage on the serving device (spec 056 T081, FR-043): the storage is opened here
//! with its credentials and the vault's content keys, which never leave this device; a path through
//! an encrypted folder goes by its decrypted names, so the contact sees plain files. Reads come in
//! ranges, an encrypted file block by block. A move stays in its folder (a rename); a delete is for
//! good, as a storage has no trash.

use std::future::Future;
use std::io::Cursor;
use std::pin::Pin;
use std::sync::Arc;

use tokio::io::AsyncRead;
use tokio_util::sync::CancellationToken;

use super::roots::StorageAt;
use super::ServeContext;
use crate::files::encrypted::format::encode_id;
use crate::files::encrypted::ops::file_at;
use crate::files::encrypted::route::{route, Route};
use crate::files::encrypted::source::{EncryptedFile, WINDOW};
use crate::files::encrypted::tree::facts;
use crate::files::peer::wire::{FailCode, WireEntry, WireKind};
use crate::files::share::store::FileShareRow;
use crate::files::storage_source::{object_key, storage_error, StorageFiles};
use crate::files::{EntryKind, FilesError, FilesErrorCode};
use crate::remote_storage::StorageError;

/// Opens the storage of a share by its id, with the scope of its encrypted folders.
pub type StorageOpener = Arc<
    dyn Fn(String) -> Pin<Box<dyn Future<Output = Result<StorageFiles, FilesError>> + Send>>
        + Send
        + Sync,
>;

/// A request's target in a storage.
#[derive(Clone)]
pub struct InStorage {
    pub files: Arc<StorageFiles>,
    pub at: StorageAt,
    /// The share it is reached through, for the state of its source (FR-022).
    pub share: FileShareRow,
}

impl InStorage {
    pub fn path(&self) -> String {
        self.at.path()
    }

    /// The folder of the entry and its name.
    fn parent_and_name(&self) -> Result<(String, String), FailCode> {
        let path = self.path();
        let (parent, name) = path.rsplit_once('/').ok_or(FailCode::Refused)?;
        if name.is_empty() {
            return Err(FailCode::Refused);
        }
        Ok((
            if parent.is_empty() { "/" } else { parent }.to_owned(),
            name.to_owned(),
        ))
    }
}

/// The storage `storage_id` for a contact's request; another device or a received share is never
/// served on (FR-015), and without an opener no storage is.
pub async fn open(ctx: &ServeContext, storage_id: &str) -> Result<StorageFiles, FailCode> {
    if crate::files::browser_commands::is_remote(storage_id) || storage_id.starts_with("device:") {
        return Err(FailCode::Refused);
    }
    let opener = ctx.storages.as_ref().ok_or(FailCode::Unsupported)?;
    opener(storage_id.to_owned()).await.map_err(fail_of)
}

/// A failure of the file layer as the other side may see it, without any text (FR-042).
pub fn fail_of(error: FilesError) -> FailCode {
    match error.code {
        FilesErrorCode::NotFound => FailCode::NotFound,
        FilesErrorCode::Exists => FailCode::NameTaken,
        FilesErrorCode::TooLarge => FailCode::TooLarge,
        FilesErrorCode::Conflict => FailCode::Conflict,
        FilesErrorCode::Unsupported => FailCode::Unsupported,
        FilesErrorCode::Credentials
        | FilesErrorCode::Unreachable
        | FilesErrorCode::HostKey
        | FilesErrorCode::NoKey
        | FilesErrorCode::NoSpace
        | FilesErrorCode::Broken
        | FilesErrorCode::Damaged
        | FilesErrorCode::OtherVault
        | FilesErrorCode::NewerFormat => FailCode::SourceUnavailable,
        _ => FailCode::Refused,
    }
}

fn wire(entry: &crate::files::Entry, etag: String) -> WireEntry {
    WireEntry {
        name: entry.name.clone(),
        kind: match entry.kind {
            EntryKind::Dir => WireKind::Folder,
            EntryKind::File => WireKind::File,
        },
        size: entry.size.unwrap_or(0),
        modified_ms: entry.modified_ms,
        etag,
    }
}

/// One level of the folder `at`, at most `max` entries; only the shared file of a file share.
/// Entries carry no version tag; a stat names it.
pub async fn list(at: &InStorage, max: usize) -> Result<Vec<WireEntry>, FailCode> {
    let entries = at.files.list(&at.path()).await.map_err(fail_of)?;
    Ok(entries
        .iter()
        .filter(|entry| at.at.only.as_deref().is_none_or(|only| entry.name == only))
        .take(max)
        .map(|entry| wire(entry, String::new()))
        .collect())
}

/// The state of the entry `at`, with its version tag.
pub async fn stat(at: &InStorage) -> Result<WireEntry, FailCode> {
    let entry = at.files.stat(&at.path()).await.map_err(fail_of)?;
    let etag = match version(&at.files, &at.path()).await? {
        Some((_, etag)) => etag,
        None => return Err(FailCode::NotFound),
    };
    Ok(wire(&entry, etag))
}

/// Size and version tag of the file at `path`: the provider's ETag, or in an encrypted folder the
/// id of the version shown; a folder has neither; `None` when nothing is there.
pub async fn version(files: &StorageFiles, path: &str) -> Result<Option<(u64, String)>, FailCode> {
    if let Route::Encrypted(at) = route(files, path).await.map_err(fail_of)? {
        if at.rest.is_empty() {
            return Ok(Some((0, String::new())));
        }
        return match file_at(files, &at).await {
            Ok((_, version)) => Ok(Some((
                facts(&version).map_or(0, |facts| facts.size),
                encode_id(&version.sid),
            ))),
            Err(error) if error.code == FilesErrorCode::NotFound => Ok(None),
            Err(error) if error.code == FilesErrorCode::InvalidPath => Ok(Some((0, String::new()))),
            Err(error) => Err(fail_of(error)),
        };
    }
    match files.stat(path).await {
        Ok(entry) if entry.kind == EntryKind::Dir => Ok(Some((0, String::new()))),
        Ok(_) => {
            let key = object_key(path).map_err(fail_of)?;
            let head = files
                .store()
                .head(files.access(), &key, soon())
                .await
                .map_err(|error| fail_of(storage_error(error)))?;
            Ok(Some((head.size, head.etag.unwrap_or_default())))
        }
        Err(error) if error.code == FilesErrorCode::NotFound => Ok(None),
        Err(error) => Err(fail_of(error)),
    }
}

/// A reader of at most `len` bytes of the file `at` from `start`, the file's size, and how many
/// bytes the reader gives.
pub async fn open_read(
    at: &InStorage,
    start: u64,
    len: u64,
) -> Result<(u64, u64, Box<dyn AsyncRead + Send + Unpin>), FailCode> {
    let path = at.path();
    if at.at.key.is_empty() {
        return Err(FailCode::NotFound);
    }
    let files = &at.files;
    if let Route::Encrypted(located) = route(files, &path).await.map_err(fail_of)? {
        let (folder, version) = file_at(files, &located).await.map_err(fail_of)?;
        let file = Arc::new(
            EncryptedFile::open(files, folder, &version)
                .await
                .map_err(fail_of)?,
        );
        let size = file.size();
        let end = size.min(start.saturating_add(len));
        let len = end.saturating_sub(start);
        // Window by window, so a large file never sits in memory whole.
        let windows = futures::stream::try_unfold(start, move |from| {
            let file = Arc::clone(&file);
            async move {
                if from >= end {
                    return Ok(None);
                }
                let to = end.min(from + WINDOW);
                let bytes = file
                    .read(from, to)
                    .await
                    .map_err(|_| std::io::Error::other("an encrypted file could not be read"))?;
                Ok::<_, std::io::Error>(Some((Cursor::new(bytes), to)))
            }
        });
        let reader = tokio_util::io::StreamReader::new(Box::pin(windows));
        return Ok((size, len, Box::new(reader)));
    }
    let key = object_key(&path).map_err(fail_of)?;
    let store = files.store();
    let head = store
        .head(files.access(), &key, soon())
        .await
        .map_err(|error| fail_of(storage_error(error)))?;
    let len = len.min(head.size.saturating_sub(start));
    let reader = store
        .get_range(files.access(), &key, start, len, soon())
        .await
        .map_err(|error| fail_of(storage_error(error)))?;
    Ok((head.size, len, reader))
}

/// Creates the folder `at`; an entry of that name answers `NameTaken`.
pub async fn create_folder(at: &InStorage) -> Result<(), FailCode> {
    let (parent, name) = at.parent_and_name()?;
    at.files
        .create_folder(&parent, &name)
        .await
        .map(|_| ())
        .map_err(fail_of)
}

/// Renames `from` to `to` in the same folder; a move into another folder of a storage is not
/// offered (`Unsupported`).
pub async fn move_entry(from: &InStorage, to: &InStorage) -> Result<(), FailCode> {
    let ((from_parent, _), (to_parent, name)) = (from.parent_and_name()?, to.parent_and_name()?);
    if from.share.id != to.share.id || from_parent != to_parent {
        return Err(FailCode::Unsupported);
    }
    // A taken name answers `Exists`, which is `NameTaken` here.
    from.files
        .rename(&from.path(), &name)
        .await
        .map(|_| ())
        .map_err(fail_of)
}

/// Removes the entry `at` for good.
pub async fn delete(at: &InStorage) -> Result<(), FailCode> {
    at.files
        .delete(&at.path(), &CancellationToken::new(), &mut |_| {})
        .await
        .map_err(fail_of)
}

pub(super) fn soon() -> tokio::time::Instant {
    tokio::time::Instant::now() + std::time::Duration::from_secs(30)
}

/// A provider failure without its text.
pub(super) fn provider(error: StorageError) -> FailCode {
    fail_of(storage_error(error))
}

#[cfg(test)]
#[path = "storage_tests.rs"]
mod tests;
