//! Encrypted folders in storages (spec 048). [`format`] is the pure format `HXEF` v1 (FR-041,
//! FR-042); the rest puts it into the file browser:
//!
//! - [`route`]: which window paths go through an encrypted folder (found by its `*.hxef/` prefix,
//!   shown by its decrypted name), and opening one.
//! - [`load`] and [`cache`]: heads and sidecars from the bucket, with the ETag cache in the vault.
//! - [`tree`]: the view of a folder from its sidecars.
//! - [`by_id`]: folders by their id, for callers outside the window (extensions).
//! - [`ops`]: list, stat, read and create inside; [`manage`]: rename, delete and the versions they
//!   replace; [`cleanup`]: what stays behind; [`source`]: reading ranges block by block;
//!   [`transfer`]: sealing uploads; [`opened`]: copies for the system's apps.
//!
//! Opened folders ([`EncryptedFolders`]) keep their keys in memory as `Zeroizing` until the vault
//! closes. Nothing here logs a name, a path or an object key of an encrypted folder (FR-027).

pub mod by_id;
pub mod cache;
pub mod cleanup;
pub mod device;
#[cfg(test)]
pub mod device_scene;
pub mod folder_view;
pub mod format;
pub mod journal;
pub mod load;
pub mod manage;
pub mod opened;
pub mod ops;
pub mod redacted;
pub mod route;
pub mod source;
pub mod transfer;
pub mod tree;
pub mod vault_keys;

#[cfg(test)]
pub mod test_support;

#[cfg(test)]
#[path = "folder_tests.rs"]
mod folder_tests;
#[cfg(test)]
#[path = "leak_tests.rs"]
mod leak_tests;
#[cfg(test)]
#[path = "perf_tests.rs"]
mod perf;
#[cfg(test)]
#[path = "protocols_tests.rs"]
mod protocols;

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use zeroize::Zeroizing;

pub use self::folder_view::FolderView;
use self::format::keys::FolderKeys;
use self::format::sidecar::{seal_sidecar, FileFacts, Sidecar, WrappedDek};
use self::format::{encode_id, FormatError, Id};
use self::load::LoadedFolder;
use crate::files::{EncryptedState, FilesError, FilesErrorCode};
use crate::vault_gate::VaultDb;

/// A fresh folder, entry, sidecar or content id.
pub fn new_id() -> Id {
    *crate::sync::keys::random_bytes::<16>()
}

/// Hands out the vault database when a call needs it; a `StorageFiles` never holds a `VaultDb`
/// itself, so a failed transfer kept for a retry does not hold the close up.
pub type VaultSource = Arc<dyn Fn() -> Result<VaultDb, FilesError> + Send + Sync>;

/// Runs work in the background of the vault session (the cleanup after a load), ending with it.
pub type Background = Arc<dyn Fn(futures::future::BoxFuture<'static, ()>) + Send + Sync>;

/// What a storage of the window needs for encrypted folders. Callers without it (agents, until
/// spec 048 US6) see encrypted folders by their raw prefix and cannot go into them.
#[derive(Clone)]
pub struct Scope {
    pub storage_id: String,
    pub folders: Arc<EncryptedFolders>,
    pub vault: VaultSource,
    pub background: Background,
}

impl Scope {
    pub fn db(&self) -> Result<VaultDb, FilesError> {
        (self.vault)()
    }
}

/// One encrypted folder as its parent's listing shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FolderHead {
    /// `P`: the parent's prefix, `base32(fid)`, `.hxef/`.
    pub prefix: String,
    pub fid: Option<Id>,
    pub state: EncryptedState,
    /// The decrypted name, a conflict name, or for an unreadable folder its raw prefix name (the
    /// window shows a neutral label for those).
    pub display: String,
    pub conflict: bool,
    pub modified_ms: Option<i64>,
    pub etag: Option<String>,
}

/// The error for a folder in `state`.
pub fn state_error(state: EncryptedState) -> FilesError {
    match state {
        EncryptedState::Readable => FilesError::new(FilesErrorCode::NotFound, "not found"),
        EncryptedState::OtherVault => FilesError::new(
            FilesErrorCode::OtherVault,
            "an encrypted folder of another vault",
        ),
        EncryptedState::NewerFormat => FilesError::new(
            FilesErrorCode::NewerFormat,
            "an encrypted folder of a newer format; holzi needs an update",
        ),
        EncryptedState::Damaged => damaged(),
    }
}

/// The error for an entry or folder that failed its checks; it says nothing about which.
pub fn damaged() -> FilesError {
    FilesError::new(FilesErrorCode::Damaged, "the encrypted data is damaged")
}

/// [`damaged`] for a format error; a name error stays what it is.
pub fn format_error(error: FormatError) -> FilesError {
    match error {
        FormatError::Name(error) => error,
        _ => damaged(),
    }
}

/// An opened encrypted folder: its keys and, once loaded, its view.
pub struct OpenFolder {
    pub prefix: String,
    pub fid: Id,
    pub etag: Option<String>,
    /// `FK`, for a new name in the head.
    fk: Zeroizing<[u8; 32]>,
    keys: FolderKeys,
    view: tokio::sync::Mutex<Option<FolderView>>,
}

impl std::fmt::Debug for OpenFolder {
    // Never the keys, the prefix or anything of the view (FR-027).
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("OpenFolder")
    }
}

impl OpenFolder {
    pub fn new(prefix: String, fid: Id, etag: Option<String>, fk: &Zeroizing<[u8; 32]>) -> Self {
        Self {
            prefix,
            fid,
            etag,
            fk: fk.clone(),
            keys: FolderKeys::derive(fk),
            view: tokio::sync::Mutex::new(None),
        }
    }

    /// The view, loaded by `load` when this is its first use.
    pub async fn view<F, Fut>(
        &self,
        load: F,
    ) -> Result<tokio::sync::MappedMutexGuard<'_, FolderView>, FilesError>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = Result<LoadedFolder, FilesError>>,
    {
        let mut guard = self.view.lock().await;
        if guard.is_none() {
            *guard = Some(FolderView::new(self.fid, load().await?));
        }
        let fid = self.fid;
        Ok(tokio::sync::MutexGuard::map(guard, |view| {
            let view = view.as_mut().expect("loaded just above");
            // Writes only mark the tree stale: a transfer of many files builds it once, not per file.
            if view.stale {
                view.rebuild(fid);
            }
            view
        }))
    }

    /// The number of this device's last change to the view; a reload takes it before it loads.
    pub async fn mark(&self) -> u64 {
        self.view
            .lock()
            .await
            .as_ref()
            .map_or(0, |view| view.journal.mark)
    }

    /// Replaces the view with a new load that began at `mark`; what this device changed since
    /// stays ([`journal`]).
    pub async fn reload(&self, loaded: LoadedFolder, mark: u64) {
        let mut guard = self.view.lock().await;
        let mut fresh = FolderView::new(self.fid, loaded);
        if let Some(old) = guard.as_ref() {
            for change in old.journal.since(mark) {
                fresh.apply(change);
            }
            fresh.journal = old.journal.clone();
            fresh.rebuild(self.fid);
        }
        *guard = Some(fresh);
    }

    /// Marks the version `sid` damaged after a failed read (FR-022, FR-025).
    pub async fn mark_damaged(&self, sid: Id) {
        if let Some(view) = self.view.lock().await.as_mut() {
            view.damaged.insert(sid);
            view.rebuild(self.fid);
        }
    }

    pub fn keys(&self) -> &FolderKeys {
        &self.keys
    }

    pub fn fk(&self) -> &[u8; 32] {
        &self.fk
    }

    /// The sealed sidecar `sid` of this folder.
    pub fn seal(&self, sid: &Id, sidecar: &Sidecar) -> Result<Vec<u8>, FilesError> {
        seal_sidecar(&self.keys, &self.fid, sid, sidecar).map_err(format_error)
    }

    /// The `DEK` of a file version of entry `entry`.
    pub fn unwrap_dek(
        &self,
        entry: &Id,
        facts: &FileFacts,
    ) -> Result<Zeroizing<[u8; 32]>, FilesError> {
        facts
            .dek
            .unwrap(&self.keys, &self.fid, entry, &facts.cid)
            .map_err(|_| damaged())
    }

    /// `dek` wrapped for entry `entry` of this folder.
    pub fn wrap_dek(&self, entry: &Id, cid: &Id, dek: &[u8; 32]) -> WrappedDek {
        WrappedDek::wrap(&self.keys, &self.fid, entry, cid, dek)
    }

    /// The object key of a sidecar or a content object.
    pub fn sidecar_key(&self, sid: &Id) -> String {
        format!("{}m/{}", self.prefix, encode_id(sid))
    }

    pub fn content_key(&self, oid: &Id) -> String {
        format!("{}c/{}", self.prefix, encode_id(oid))
    }
}

type Place = (String, String);

#[derive(Default)]
struct Inner {
    /// The encrypted folders right below a prefix of a storage, as its last listing found them.
    heads: HashMap<Place, Vec<Arc<FolderHead>>>,
    /// Opened folders by storage and `P`.
    open: HashMap<Place, Arc<OpenFolder>>,
}

/// The encrypted folders of this session, kept in [`crate::files::state::FilesState`] until the
/// vault closes.
#[derive(Default)]
pub struct EncryptedFolders {
    inner: Mutex<Inner>,
}

impl EncryptedFolders {
    fn inner(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// The heads below `parent` of `storage`, if a listing found them in this session.
    pub fn heads(&self, storage: &str, parent: &str) -> Option<Vec<Arc<FolderHead>>> {
        self.inner()
            .heads
            .get(&(storage.to_owned(), parent.to_owned()))
            .cloned()
    }

    pub fn set_heads(&self, storage: &str, parent: &str, heads: Vec<Arc<FolderHead>>) {
        self.inner()
            .heads
            .insert((storage.to_owned(), parent.to_owned()), heads);
    }

    /// Forgets the heads below `parent` (a folder there was renamed, moved or deleted), so the
    /// next path through it lists that level again.
    pub fn forget_heads(&self, storage: &str, parent: &str) {
        self.inner()
            .heads
            .remove(&(storage.to_owned(), parent.to_owned()));
    }

    /// The folder `prefix` of `storage`, opened with the head of `etag`.
    pub fn opened(
        &self,
        storage: &str,
        prefix: &str,
        etag: Option<&str>,
    ) -> Option<Arc<OpenFolder>> {
        self.inner()
            .open
            .get(&(storage.to_owned(), prefix.to_owned()))
            .filter(|folder| folder.etag.as_deref() == etag)
            .cloned()
    }

    pub fn keep(&self, storage: &str, folder: Arc<OpenFolder>) {
        self.inner()
            .open
            .insert((storage.to_owned(), folder.prefix.clone()), folder);
    }

    /// Forgets every head and opened folder, with their keys (the vault closes).
    pub fn clear(&self) {
        let mut inner = self.inner();
        inner.heads.clear();
        inner.open.clear();
    }
}

#[cfg(test)]
#[path = "mod_tests.rs"]
mod tests;
