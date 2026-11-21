//! Chat files on this device (spec 072 research R14): sent and received files are kept in the
//! encrypted folder `Chat` of spec 048 inside the device folder `<app data>/chat-files/<instance>/`,
//! one file per attachment, named by the attachment id. Without the open vault they cannot be read,
//! and they never leave this device's data directory. Like the agent browser's profile
//! (`agent_browser::profile_store`).
//!
//! A file is written as its plaintext comes in (`PutStream`), a few parts in memory at a time, and
//! shows only when the caller commits (R14 amendment of 2026-11-21). Making the folder takes turns
//! behind a semaphore of one, so two first writes do not both create it.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use tokio::sync::Semaphore;

use crate::error::{HaexError, Result};
use crate::files::encrypted::device::DeviceRoot;
use crate::files::encrypted::ops::{create_encrypted_folder, file_at};
use crate::files::encrypted::route::{route, Route};
use crate::files::encrypted::source::EncryptedFile;
use crate::files::encrypted::transfer::PutStream;
use crate::files::encrypted::{EncryptedFolders, Scope, VaultSource};
use crate::files::storage_source::StorageFiles;
use crate::files::{FilesError, FilesErrorCode};
use crate::instances::vault_paths::VaultPaths;
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

    /// Starts the file of attachment `id`, making the folders on first use. It shows when the
    /// writer commits; a writer that is dropped without that leaves no file.
    pub async fn writer(&self, id: &str) -> Result<StoreWriter<'_>> {
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
        Ok(StoreWriter { put })
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
