//! Chat files on this device (spec 072 research R14): sent and received files are kept in the
//! encrypted folder `Chat` of spec 048 inside the device folder `<app data>/chat-files/<instance>/`,
//! one file per attachment, named by the attachment id. Without the open vault they cannot be read,
//! and they never leave this device's data directory. Like the agent browser's profile
//! (`agent_browser::profile_store`).
//!
//! A file is written as its plaintext comes in (`PutStream`), a few parts in memory at a time, and
//! shows only when the caller commits (R14 amendment of 2026-11-21). Making the folder takes turns
//! behind a semaphore of one, so two first writes do not both create it.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, Weak};

use tokio::sync::{Mutex as AsyncMutex, OwnedMutexGuard, Semaphore};
use tokio_util::sync::CancellationToken;

use crate::error::{HaexError, Result};
use crate::files::encrypted::device::DeviceRoot;
use crate::files::encrypted::manage;
use crate::files::encrypted::opened;
use crate::files::encrypted::ops::{create_encrypted_folder, file_at};
use crate::files::encrypted::route::{route, Route};
use crate::files::encrypted::source::EncryptedFile;
use crate::files::encrypted::transfer::PutStream;
use crate::files::encrypted::{EncryptedFolders, Scope, VaultSource};
use crate::files::storage_source::StorageFiles;
use crate::files::{FilesError, FilesErrorCode};
use crate::instances::vault_paths::VaultPaths;
use crate::passwords::attachment_view::{copy_file_name, remove_copy};
use crate::remote_storage::local::LocalStore;

/// The encrypted folder that holds the chat files.
pub const FOLDER: &str = "Chat";

/// The device folder of the vault instance `instance`.
pub fn dir(app_local_data: &Path, instance: &str) -> PathBuf {
    VaultPaths::new(app_local_data).chat_files(instance)
}

/// The chat files of one vault on this device.
pub struct ChatFileStore {
    files: StorageFiles,
    root: DeviceRoot,
    dir: PathBuf,
    making: Semaphore,
    writing: Mutex<HashMap<String, Weak<AsyncMutex<()>>>>,
}

impl ChatFileStore {
    /// The store in `dir` (absolute), reading keys through `vault`.
    pub fn new(dir: &Path, vault: VaultSource) -> Result<Self> {
        let root = DeviceRoot::of(dir).ok_or_else(|| HaexError::InvalidInput {
            reason: "the chat files folder is not absolute".into(),
        })?;
        let files = root.files(
            Arc::new(LocalStore::new()),
            Scope {
                storage_id: root.storage_id.clone(),
                folders: Arc::new(EncryptedFolders::default()),
                vault,
                background: Arc::new(|_| {}),
            },
        );
        Ok(Self {
            files,
            root,
            dir: dir.to_path_buf(),
            making: Semaphore::new(1),
            writing: Mutex::new(HashMap::new()),
        })
    }

    fn view(&self, rel: &[&str]) -> String {
        let mut path = self.dir.clone();
        path.extend(rel);
        self.root.view_of(&path)
    }

    /// The file of attachment `id`; `None` when this device has none.
    pub async fn get(&self, id: &str) -> Result<Option<Vec<u8>>> {
        if !self.dir.is_dir() {
            return Ok(None);
        }
        let Route::Encrypted(at) = route(&self.files, &self.view(&[FOLDER, id]))
            .await
            .map_err(store_error)?
        else {
            return Ok(None);
        };
        let (folder, version) = match file_at(&self.files, &at).await {
            Ok(found) => found,
            Err(error) if error.code == FilesErrorCode::NotFound => return Ok(None),
            Err(error) => return Err(store_error(error)),
        };
        let file = EncryptedFile::open(&self.files, folder, &version)
            .await
            .map_err(store_error)?;
        file.read(0, file.size())
            .await
            .map(Some)
            .map_err(store_error)
    }

    /// A decrypted copy of the file of attachment `id` for the system's app (spec 048 FR-028, spec 072
    /// US3): in a folder of its own below `root`, named `name` (cleaned for every system), so the
    /// app can tell the type from the ending. `None` while the notice about the copy has to be shown
    /// first; `NotFound` while this device has no file.
    pub async fn copy_for_app(
        &self,
        id: &str,
        name: &str,
        root: &Path,
        confirmed: bool,
        dont_ask_again: bool,
    ) -> Result<Option<PathBuf>> {
        let missing = || HaexError::NotFound {
            name: "chat file".into(),
        };
        if !self.dir.is_dir() {
            return Err(missing());
        }
        let Route::Encrypted(at) = route(&self.files, &self.view(&[FOLDER, id]))
            .await
            .map_err(store_error)?
        else {
            return Err(missing());
        };
        let copy =
            match opened::copy_for_app(&self.files, &at, root, confirmed, dont_ask_again).await {
                Ok(copy) => copy,
                Err(error) if error.code == FilesErrorCode::NeedsNotice => return Ok(None),
                Err(error) if error.code == FilesErrorCode::NotFound => return Err(missing()),
                Err(error) => return Err(store_error(error)),
            };
        // The copy is named by the attachment id, like the file in the folder.
        let cleaned = copy_file_name(name);
        let target = copy.with_file_name(if cleaned.is_empty() { id } else { &cleaned });
        if target != copy {
            if let Err(error) = tokio::fs::rename(&copy, &target).await {
                remove_copy(&copy).await;
                return Err(HaexError::InvalidInput {
                    reason: format!("chat files: the copy could not be named ({})", error.kind()),
                });
            }
        }
        Ok(Some(target))
    }

    /// Starts the file of attachment `id`, making the folders on first use. It shows when the
    /// writer commits; a writer that is dropped without that leaves no file.
    pub async fn writer(&self, id: &str) -> Result<StoreWriter<'_>> {
        let same_id = self.lock_attachment(id).await;
        self.writer_with_lock(id, same_id).await
    }

    /// Serializes the full download-to-commit path for this attachment. Different ids remain
    /// independent; the guard can be passed to [`Self::writer_with_lock`].
    pub async fn lock_attachment(&self, id: &str) -> OwnedMutexGuard<()> {
        let same_id = {
            let mut writing = self
                .writing
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            writing.retain(|_, lock| lock.strong_count() > 0);
            // The weak registry must not be the only owner of a lock.
            let lock = writing
                .get(id)
                .and_then(Weak::upgrade)
                .unwrap_or_else(|| Arc::new(AsyncMutex::new(())));
            writing.insert(id.to_owned(), Arc::downgrade(&lock));
            lock
        };
        Arc::clone(&same_id).lock_owned().await
    }

    /// Starts a writer while keeping an attachment guard from before the download began.
    pub async fn writer_with_lock(
        &self,
        id: &str,
        same_id: OwnedMutexGuard<()>,
    ) -> Result<StoreWriter<'_>> {
        let at = {
            let _turn = self
                .making
                .acquire()
                .await
                .map_err(|_| HaexError::InvalidInput {
                    reason: "the chat files store is closed".into(),
                })?;
            std::fs::create_dir_all(&self.dir).map_err(|e| HaexError::InvalidInput {
                reason: format!("the chat files folder: {e}"),
            })?;
            if !matches!(
                route(&self.files, &self.view(&[FOLDER]))
                    .await
                    .map_err(store_error)?,
                Route::Encrypted(_)
            ) {
                create_encrypted_folder(&self.files, &self.view(&[]), FOLDER)
                    .await
                    .map_err(store_error)?;
            }
            match route(&self.files, &self.view(&[FOLDER, id]))
                .await
                .map_err(store_error)?
            {
                Route::Encrypted(at) => at,
                _ => {
                    return Err(HaexError::InvalidInput {
                        reason: "the encrypted chat folder is gone".into(),
                    })
                }
            }
        };
        let put = PutStream::begin(&self.files, &at)
            .await
            .map_err(store_error)?;
        Ok(StoreWriter {
            put,
            _same_id: same_id,
        })
    }

    /// Deletes the file of attachment `id`; a missing one is not an error.
    pub async fn remove(&self, id: &str) -> Result<()> {
        if !self.dir.is_dir() {
            return Ok(());
        }
        let _same_id = self.lock_attachment(id).await;
        let Route::Encrypted(at) = route(&self.files, &self.view(&[FOLDER, id]))
            .await
            .map_err(store_error)?
        else {
            return Ok(());
        };
        match manage::delete(&self.files, &at, &CancellationToken::new(), &mut |_| {}).await {
            Err(error) if error.code == FilesErrorCode::NotFound => Ok(()),
            other => other.map_err(store_error),
        }
    }

    /// Stores `bytes` as the file of attachment `id`.
    pub async fn put(&self, id: &str, bytes: &[u8]) -> Result<()> {
        let mut writer = self.writer(id).await?;
        if let Err(error) = writer.push(bytes).await {
            writer.abort().await;
            return Err(error);
        }
        writer.commit().await
    }
}

/// A chat file being written (see [`ChatFileStore::writer`]). After an error the caller calls
/// [`Self::abort`].
pub struct StoreWriter<'a> {
    put: PutStream<'a>,
    _same_id: OwnedMutexGuard<()>,
}

impl StoreWriter<'_> {
    /// Takes the next plaintext.
    pub async fn push(&mut self, bytes: &[u8]) -> Result<()> {
        self.put.push(bytes).await.map_err(store_error)
    }

    /// Makes the file show.
    pub async fn commit(self) -> Result<()> {
        self.put.commit().await.map_err(store_error)
    }

    /// Drops what was written.
    pub async fn abort(self) {
        self.put.abort().await;
    }
}

fn store_error(error: FilesError) -> HaexError {
    HaexError::InvalidInput {
        reason: format!("chat files: {}", error.message),
    }
}

#[cfg(test)]
#[path = "store_tests.rs"]
mod tests;
