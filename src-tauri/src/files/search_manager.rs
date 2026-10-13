//! The running searches of the window (spec 044 FR-028): one task each, below the vault gate. A
//! search of the device goes on into the encrypted folders it passed, by their decrypted names
//! (spec 048 FR-049).

use tokio_util::sync::CancellationToken;

use super::encrypted::device::DeviceRoot;
use super::search::{search_passing, search_storage, FilesSearchEvent, SearchEnd, SearchOptions};
use super::search_device::search_passed;
use super::storage_source::StorageFiles;

/// The running searches of this session; a Tauri managed state. Each has its own token below the
/// gate's, so closing the vault ends it; a new search of a tab cancels the old one from the window
/// (FR-028).
#[derive(Clone, Default)]
pub struct SearchManager {
    running: std::sync::Arc<std::sync::Mutex<std::collections::HashMap<String, CancellationToken>>>,
}

impl SearchManager {
    fn running(
        &self,
    ) -> std::sync::MutexGuard<'_, std::collections::HashMap<String, CancellationToken>> {
        self.running
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
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
        let id = uuid::Uuid::new_v4().to_string();
        let cancel = gate.token().child_token();
        self.running().insert(id.clone(), cancel.clone());
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
        let id = uuid::Uuid::new_v4().to_string();
        let cancel = gate.token().child_token();
        self.running().insert(id.clone(), cancel.clone());
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
        if let Some(cancel) = self.running().remove(id) {
            cancel.cancel();
        }
    }
}
