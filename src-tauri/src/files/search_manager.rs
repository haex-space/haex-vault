//! The running searches of the window (spec 044 FR-028): one task each, below the vault gate. A
//! search of the device goes on into the encrypted folders it passed, by their decrypted names
//! (spec 048 FR-049).

use std::path::{Path, PathBuf};

use tokio_util::sync::CancellationToken;

use super::encrypted::device::DeviceRoot;
use super::search::{search_passing, search_storage, FilesSearchEvent, SearchEnd, SearchOptions};
use super::search_device::search_passed;
use super::storage_source::StorageFiles;

/// A running search: the device folder it walks (`None` for a storage) and its token.
type Running = (Option<PathBuf>, CancellationToken);

/// The running searches of this session; a Tauri managed state. Each has its own token below the
/// gate's, so closing the vault ends it; a new search of a tab cancels the old one from the window
/// (FR-028).
#[derive(Clone, Default)]
pub struct SearchManager {
    running: std::sync::Arc<std::sync::Mutex<std::collections::HashMap<String, Running>>>,
}

impl SearchManager {
    fn running(&self) -> std::sync::MutexGuard<'_, std::collections::HashMap<String, Running>> {
        self.running
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Keeps a new search of `root` with its token; returns its id.
    fn track(&self, root: Option<PathBuf>, cancel: CancellationToken) -> String {
        let id = uuid::Uuid::new_v4().to_string();
        self.running().insert(id.clone(), (root, cancel));
        id
    }

    /// Starts a search of the device on the blocking pool; returns its id. With `encrypted` (the
    /// device's storage) it goes on into the encrypted folders the walk passed.
    pub fn start(
        &self,
        gate: &crate::vault_gate::VaultGate,
        root: std::path::PathBuf,
        query: String,
        options: SearchOptions,
        channel: tauri::ipc::Channel<FilesSearchEvent>,
        encrypted: Option<(StorageFiles, DeviceRoot)>,
    ) -> Result<String, crate::files::FilesError> {
        let cancel = gate.token().child_token();
        let id = self.track(Some(root.clone()), cancel.clone());
        let manager = self.clone();
        let owned_id = id.clone();
        let spawned = gate.spawn_blocking(move || {
            let found = std::sync::atomic::AtomicUsize::new(0);
            let mut passed = Vec::new();
            let mut send = |hits: Vec<_>| {
                found.fetch_add(hits.len(), std::sync::atomic::Ordering::Relaxed);
                let _ = channel.send(FilesSearchEvent::Hits { hits });
            };
            let mut end = search_passing(&root, &query, &options, &cancel, &mut send, &mut passed);
            if let (SearchEnd::Done { truncated: false }, Some((files, device))) = (end, encrypted)
            {
                if !passed.is_empty() {
                    let handle = tokio::runtime::Handle::current();
                    end = handle.block_on(search_passed(
                        &files,
                        &device,
                        passed,
                        &query,
                        &options,
                        found.load(std::sync::atomic::Ordering::Relaxed),
                        &cancel,
                        &mut send,
                    ));
                }
            }
            manager.running().remove(&owned_id);
            if let SearchEnd::Done { truncated } = end {
                let _ = channel.send(FilesSearchEvent::Done { truncated });
            }
        });
        if let Err(error) = spawned {
            self.running().remove(&id);
            log::warn!("files: a search could not start: {error}");
            return Err(crate::files::FilesError::new(
                crate::files::FilesErrorCode::Unsupported,
                "the vault is closing",
            ));
        }
        Ok(id)
    }

    /// Starts a search of a storage as tracked session work; returns its id. With `device` (an
    /// encrypted folder on this device) hits carry device paths.
    #[allow(clippy::too_many_arguments)]
    pub fn start_storage(
        &self,
        gate: &crate::vault_gate::VaultGate,
        files: crate::files::storage_source::StorageFiles,
        root: String,
        query: String,
        options: SearchOptions,
        channel: tauri::ipc::Channel<FilesSearchEvent>,
        device: Option<DeviceRoot>,
    ) -> Result<String, crate::files::FilesError> {
        let cancel = gate.token().child_token();
        let id = self.track(None, cancel.clone());
        let manager = self.clone();
        let owned_id = id.clone();
        let spawned = gate.spawn(async move {
            let (hits, steps) = (channel.clone(), channel.clone());
            let end = search_storage(
                &files,
                &root,
                &query,
                &options,
                &cancel,
                &mut |found: Vec<super::search::SearchHit>| {
                    let found = match &device {
                        Some(root) => found
                            .into_iter()
                            .map(|hit| super::search::SearchHit {
                                entry: root.entry(hit.entry),
                                score: hit.score,
                            })
                            .collect(),
                        None => found,
                    };
                    let _ = hits.send(FilesSearchEvent::Hits { hits: found });
                },
                &mut |dirs| {
                    let _ = steps.send(FilesSearchEvent::Progress { dirs });
                },
            )
            .await;
            manager.running().remove(&owned_id);
            if let SearchEnd::Done { truncated } = end {
                let _ = channel.send(FilesSearchEvent::Done { truncated });
            }
        });
        if let Err(error) = spawned {
            self.running().remove(&id);
            log::warn!("files: a search could not start: {error}");
            return Err(crate::files::FilesError::new(
                crate::files::FilesErrorCode::Unsupported,
                "the vault is closing",
            ));
        }
        Ok(id)
    }

    pub fn cancel(&self, id: &str) {
        if let Some((_, cancel)) = self.running().remove(id) {
            cancel.cancel();
        }
    }

    /// Cancels the searches of device folders at or below `prefix`, before a drive is released
    /// (spec 064 research R9); returns how many.
    pub fn cancel_under(&self, prefix: &Path) -> usize {
        let mut running = self.running();
        let before = running.len();
        running.retain(|_, (root, cancel)| {
            let under = root.as_ref().is_some_and(|root| root.starts_with(prefix));
            if under {
                cancel.cancel();
            }
            !under
        });
        before - running.len()
    }
}

#[cfg(test)]
#[path = "search_manager_tests.rs"]
mod tests;
