//! A folder of a storage as a place (research R1): S3, WebDAV, SFTP, another own device or an
//! encrypted folder, all through [`StorageFiles`] and its window paths. A storage has no trash, so
//! mirrored deletions go to the rule trash; moves across folders go through the transfers.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use sha2::{Digest, Sha256};
use tokio::io::AsyncReadExt;
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;

use super::place::{split, trash_path, unreachable, Facts, Listing, Place, SkipReason, Trashed};
use super::place_device::hex;
use crate::files::encrypted::route::{route, Route};
use crate::files::encrypted::{ops, tree};
use crate::files::storage_source::{object_key, storage_error, StorageFiles};
use crate::files::transfer::local::{Outcome, CHUNK};
use crate::files::transfer::remote::Side;
use crate::files::transfer::sync_copy::{transfer_one, OneTransfer};
use crate::files::transfer::TransferOp;
use crate::files::{EntryKind, FilesError, FilesErrorCode};

/// How long reading one file for its hash may take.
const READ: Duration = Duration::from_secs(60 * 60);

/// A folder of a storage; `root` is its window path (`/Fotos`, `/` for the whole storage).
#[derive(Clone)]
pub struct StoragePlace {
    files: Arc<StorageFiles>,
    root: String,
    /// The last listing was a flat one of a plain S3 place: its folders are only the prefixes of
    /// its files, so a folder needs no marker object of its own while a file goes in it.
    prefixes: Arc<AtomicBool>,
}

impl StoragePlace {
    pub fn new(files: Arc<StorageFiles>, root: impl Into<String>) -> Self {
        let root = root.into();
        let root = format!("/{}", root.trim_matches('/'));
        Self {
            files,
            root,
            prefixes: Arc::new(AtomicBool::new(false)),
        }
    }

    fn at(&self, path: &str) -> String {
        match (path.is_empty(), self.root == "/") {
            (true, _) => self.root.clone(),
            (false, true) => format!("/{path}"),
            (false, false) => format!("{}/{path}", self.root),
        }
    }

    /// Runs one transfer within this storage on a blocking thread.
    async fn transfer(
        &self,
        op: TransferOp,
        path: String,
        folder: String,
    ) -> Result<(), FilesError> {
        let side = self.side();
        tokio::task::spawn_blocking(move || {
            let handle = tokio::runtime::Handle::current();
            let one = OneTransfer {
                op,
                from: &side,
                path: &path,
                to: &side,
                folder: &folder,
                replaces: false,
                known_free: false,
                // A move within the place never takes a file out of an encrypted folder (into the
                // rule trash, say); that fails instead.
                decrypts: false,
            };
            match transfer_one(&one, &handle, &CancellationToken::new(), &mut |_| {}) {
                Outcome::Done => Ok(()),
                Outcome::Cancelled => {
                    Err(FilesError::new(FilesErrorCode::Unsupported, "cancelled"))
                }
                Outcome::Failed(error) => Err(error),
            }
        })
        .await
        .map_err(|_| FilesError::new(FilesErrorCode::Unsupported, "the work stopped"))?
    }
}

fn is_not_found(error: &FilesError) -> bool {
    error.code == FilesErrorCode::NotFound
}

#[async_trait]
impl Place for StoragePlace {
    async fn list_tree(&self, cancel: &CancellationToken) -> Result<Listing, FilesError> {
        if self.root != "/" {
            match self.files.stat(&self.root).await {
                Ok(entry) if entry.kind == EntryKind::Dir => {}
                Ok(_) => return Err(unreachable("the place is not a folder")),
                Err(error) if is_not_found(&error) => {
                    return Err(unreachable("the place's folder is missing"));
                }
                Err(error) if error.code == FilesErrorCode::Unreachable => return Err(error),
                Err(_) => return Err(unreachable("the place's folder cannot be read")),
            }
        }
        let mut listing = Listing::default();
        // One request per page of keys instead of one per folder.
        match self.files.list_flat(&self.root).await {
            Ok(Some(tree)) => {
                listing.folders = tree.folders;
                for (path, (size, mtime_ms)) in tree.files {
                    listing.files.insert(
                        path,
                        Facts {
                            size,
                            mtime_ms,
                            hash: None,
                        },
                    );
                }
                self.prefixes.store(true, Ordering::Relaxed);
                return Ok(listing);
            }
            Ok(None) => self.prefixes.store(false, Ordering::Relaxed),
            Err(error) if error.code == FilesErrorCode::Credentials => return Err(error),
            Err(_) => return Err(unreachable("the place's folder cannot be read")),
        }
        let mut folders = VecDeque::from([String::new()]);
        while let Some(folder) = folders.pop_front() {
            if cancel.is_cancelled() {
                return Err(FilesError::new(FilesErrorCode::Unsupported, "cancelled"));
            }
            // A folder that cannot be listed fails the whole listing (FR-022).
            let entries = self.files.list(&self.at(&folder)).await.map_err(|error| {
                if folder.is_empty() && error.code != FilesErrorCode::Credentials {
                    unreachable("the place's folder cannot be read")
                } else {
                    error
                }
            })?;
            for entry in entries {
                let rel = if folder.is_empty() {
                    entry.name.clone()
                } else {
                    format!("{folder}/{}", entry.name)
                };
                if entry.symlink {
                    listing.skipped.push((rel, SkipReason::Link));
                    continue;
                }
                match entry.kind {
                    EntryKind::Dir => {
                        listing.folders.insert(rel.clone());
                        folders.push_back(rel);
                    }
                    EntryKind::File if entry.no_access => {
                        listing.skipped.push((rel, SkipReason::NoAccess));
                    }
                    EntryKind::File => {
                        listing.files.insert(
                            rel,
                            Facts {
                                size: entry.size.unwrap_or(0),
                                mtime_ms: entry.modified_ms.unwrap_or(0),
                                hash: None,
                            },
                        );
                    }
                }
            }
        }
        Ok(listing)
    }

    async fn stat(&self, path: &str) -> Result<Option<Facts>, FilesError> {
        match self.files.stat(&self.at(path)).await {
            Ok(entry) if entry.kind == EntryKind::File => Ok(Some(Facts {
                size: entry.size.unwrap_or(0),
                mtime_ms: entry.modified_ms.unwrap_or(0),
                hash: None,
            })),
            Ok(_) => Ok(None),
            Err(error) if is_not_found(&error) => Ok(None),
            Err(error) => Err(error),
        }
    }

    async fn hash(&self, path: &str, cancel: &CancellationToken) -> Result<String, FilesError> {
        let at = self.at(path);
        if let Route::Encrypted(located) = route(&self.files, &at).await? {
            // The sidecar holds the hash of the plaintext; nothing is read (research R5).
            let (_, version) = ops::file_at(&self.files, &located).await?;
            let facts =
                tree::facts(&version).ok_or_else(|| FilesError::invalid_path("not a file"))?;
            return Ok(hex(&facts.sha256));
        }
        let size = self
            .stat(path)
            .await?
            .ok_or_else(|| FilesError::new(FilesErrorCode::NotFound, "not found"))?
            .size;
        let key = object_key(&at)?;
        let mut reader = self
            .files
            .store()
            .get_range(self.files.access(), &key, 0, size, Instant::now() + READ)
            .await
            .map_err(storage_error)?;
        let mut hasher = Sha256::new();
        let mut buf = vec![0u8; CHUNK];
        loop {
            if cancel.is_cancelled() {
                return Err(FilesError::new(FilesErrorCode::Unsupported, "cancelled"));
            }
            let read = reader.read(&mut buf).await.map_err(|_| {
                FilesError::new(FilesErrorCode::Unreachable, "the file could not be read")
            })?;
            if read == 0 {
                break;
            }
            hasher.update(&buf[..read]);
        }
        Ok(hex(&hasher.finalize()))
    }

    async fn create_folder(&self, path: &str) -> Result<(), FilesError> {
        let mut done = String::new();
        for name in path.split('/').filter(|name| !name.is_empty()) {
            let parent = done.clone();
            done = if done.is_empty() {
                name.to_owned()
            } else {
                format!("{done}/{name}")
            };
            match self.files.stat(&self.at(&done)).await {
                Ok(entry) if entry.kind == EntryKind::Dir => continue,
                Ok(_) => {
                    return Err(FilesError::new(
                        FilesErrorCode::Exists,
                        "a file has the folder's name",
                    ))
                }
                Err(error) if is_not_found(&error) => {}
                Err(error) => return Err(error),
            }
            match self.files.create_folder(&self.at(&parent), name).await {
                Ok(_) => {}
                Err(error) if error.code == FilesErrorCode::Exists => {}
                Err(error) => return Err(error),
            }
        }
        Ok(())
    }

    async fn rename(&self, from: &str, to: &str) -> Result<(), FilesError> {
        let (from_folder, from_name) = split(from);
        let (to_folder, to_name) = split(to);
        if from_folder != to_folder {
            self.create_folder(to_folder).await?;
            self.transfer(TransferOp::Move, self.at(from), self.at(to_folder))
                .await?;
        }
        if from_name != to_name {
            let moved = if to_folder.is_empty() {
                from_name.to_owned()
            } else {
                format!("{to_folder}/{from_name}")
            };
            self.files.rename(&self.at(&moved), to_name).await?;
        }
        Ok(())
    }

    async fn delete(&self, path: &str) -> Result<(), FilesError> {
        self.files
            .delete(&self.at(path), &CancellationToken::new(), &mut |_| {})
            .await
    }

    async fn trash(&self, path: &str, run: &str) -> Result<Trashed, FilesError> {
        let rule = trash_path(run, path);
        self.rename(path, &rule).await?;
        Ok(Trashed::Rule(rule))
    }

    async fn read_text(&self, path: &str, max: usize) -> Result<Option<String>, FilesError> {
        let Some(facts) = self.stat(path).await? else {
            return Ok(None);
        };
        if facts.size > max as u64 {
            return Err(FilesError::new(
                FilesErrorCode::TooLarge,
                "the file is too large",
            ));
        }
        let bytes = self
            .files
            .read_start(&self.at(path), facts.size, max as u64)
            .await?;
        String::from_utf8(bytes)
            .map(Some)
            .map_err(|_| FilesError::new(FilesErrorCode::Binary, "the file is no text"))
    }

    async fn write_text(&self, path: &str, text: &str) -> Result<(), FilesError> {
        let (folder, name) = split(path);
        let replaces = self.stat(path).await?.is_some();
        let dir = tempfile::tempdir()
            .map_err(|_| FilesError::new(FilesErrorCode::NoSpace, "no temporary folder"))?;
        let local = dir.path().join(name);
        std::fs::write(&local, text)
            .map_err(|_| FilesError::new(FilesErrorCode::NoSpace, "no temporary file"))?;
        let (side, folder) = (self.side(), self.at(folder));
        let source = local.to_string_lossy().into_owned();
        tokio::task::spawn_blocking(move || {
            let handle = tokio::runtime::Handle::current();
            let one = OneTransfer {
                op: TransferOp::Copy,
                from: &Side::Device,
                path: &source,
                to: &side,
                folder: &folder,
                replaces,
                known_free: false,
                decrypts: false,
            };
            let outcome = transfer_one(&one, &handle, &CancellationToken::new(), &mut |_| {});
            drop(dir);
            match outcome {
                Outcome::Done => Ok(()),
                Outcome::Cancelled => {
                    Err(FilesError::new(FilesErrorCode::Unsupported, "cancelled"))
                }
                Outcome::Failed(error) => Err(error),
            }
        })
        .await
        .map_err(|_| FilesError::new(FilesErrorCode::Unsupported, "the work stopped"))?
    }

    fn implies_folders(&self) -> bool {
        self.prefixes.load(Ordering::Relaxed)
    }

    fn side(&self) -> Side {
        Side::Storage(Arc::clone(&self.files))
    }

    fn full_path(&self, path: &str) -> String {
        self.at(path)
    }
}

#[cfg(test)]
#[path = "place_storage_tests.rs"]
mod tests;
