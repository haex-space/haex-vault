//! What the file browser keeps for the running app (spec 044): holzi's own places, the known places,
//! the thumbnail cache and the open folder watches of the window; spec 048 adds the opened encrypted
//! folders and the thumbnails of their images, both only in memory, and the copies of their files
//! for system apps; all of it goes when the vault closes.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use crate::files::encrypted::EncryptedFolders;
use crate::files::thumbnails::MemoryThumbnails;
use crate::vault_gate::VaultGate;

use crate::files::local::{known_places, OwnPlaces, Place};
use crate::files::thumbnails::CACHE_FOLDER;

/// The file browser's state, managed by Tauri.
pub struct FilesState {
    pub own: OwnPlaces,
    pub known: Vec<Place>,
    pub thumbnails: PathBuf,
    /// Copies of files of encrypted folders for the system's apps (spec 048 FR-028).
    pub opened_encrypted: PathBuf,
    #[cfg(not(target_os = "ios"))]
    watches: Mutex<HashMap<u64, crate::files::local::watch::FolderWatch>>,
    #[cfg(target_os = "ios")]
    watches: Mutex<HashMap<u64, ()>>,
    next_watch: AtomicU64,
    /// Opened encrypted folders with their keys (spec 048).
    pub encrypted: Arc<EncryptedFolders>,
    /// Thumbnails of images in encrypted folders (spec 048 FR-026).
    pub memory_thumbnails: Arc<MemoryThumbnails>,
    forgetting: AtomicBool,
}

impl FilesState {
    pub fn from_app<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> Self {
        use tauri::Manager;
        let thumbnails = app.path().app_cache_dir().map_or_else(
            |_| std::env::temp_dir().join("holzi-files-thumbnails"),
            |dir| dir.join(CACHE_FOLDER),
        );
        let state = Self::new(OwnPlaces::from_app(app), known_places(app), thumbnails);
        // Copies an earlier run left behind (it crashed before its vault closed).
        crate::files::encrypted::opened::sweep(&state.opened_encrypted);
        state
    }

    pub fn new(own: OwnPlaces, known: Vec<Place>, thumbnails: PathBuf) -> Self {
        let opened_encrypted = thumbnails
            .parent()
            .unwrap_or(&thumbnails)
            .join(crate::files::encrypted::opened::FOLDER);
        Self {
            own,
            known,
            thumbnails,
            opened_encrypted,
            watches: Mutex::new(HashMap::new()),
            next_watch: AtomicU64::new(1),
            encrypted: Arc::default(),
            memory_thumbnails: Arc::default(),
            forgetting: AtomicBool::new(false),
        }
    }

    /// Drops the opened encrypted folders, their thumbnails and the copies for system apps when
    /// `gate`'s session closes; the first call arranges it, later ones do nothing.
    pub fn forget_on_close(&self, gate: &VaultGate) {
        if self.forgetting.swap(true, Ordering::SeqCst) {
            return;
        }
        let (folders, thumbnails, opened, token) = (
            Arc::clone(&self.encrypted),
            Arc::clone(&self.memory_thumbnails),
            self.opened_encrypted.clone(),
            gate.token(),
        );
        let spawned = gate.spawn(async move {
            token.cancelled().await;
            folders.clear();
            thumbnails.clear();
            let _ = tokio::task::spawn_blocking(move || {
                crate::files::encrypted::opened::sweep(&opened);
            })
            .await;
        });
        if spawned.is_err() {
            // Closing already: nothing is kept for later.
            self.encrypted.clear();
            self.memory_thumbnails.clear();
            crate::files::encrypted::opened::sweep(&self.opened_encrypted);
            self.forgetting.store(false, Ordering::SeqCst);
        }
    }

    #[cfg(not(target_os = "ios"))]
    fn watches(&self) -> MutexGuard<'_, HashMap<u64, crate::files::local::watch::FolderWatch>> {
        self.watches.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Keeps `watch` until [`FilesState::unwatch`]; returns its id.
    #[cfg(not(target_os = "ios"))]
    pub fn keep_watch(&self, watch: crate::files::local::watch::FolderWatch) -> u64 {
        let id = self.next_watch.fetch_add(1, Ordering::SeqCst);
        self.watches().insert(id, watch);
        id
    }

    /// Ends the watch `id`; true when there was one.
    pub fn unwatch(&self, id: u64) -> bool {
        #[cfg(not(target_os = "ios"))]
        {
            self.watches().remove(&id).is_some()
        }
        #[cfg(target_os = "ios")]
        {
            let _ = id;
            false
        }
    }

    /// Ends every watch: the page that asked for them is gone (reloaded), and its channels lead
    /// nowhere.
    pub fn unwatch_all(&self) {
        #[cfg(not(target_os = "ios"))]
        self.watches().clear();
    }
}

#[cfg(all(test, not(target_os = "ios")))]
#[path = "state_tests.rs"]
mod tests;
