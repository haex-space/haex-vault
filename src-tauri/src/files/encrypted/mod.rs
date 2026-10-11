//! Encrypted folders in storages (spec 048). [`format`] is the pure format `HXEF` v1 (FR-041,
//! FR-042); the rest puts it into the file browser:
//!
//! - [`route`]: which window paths go through an encrypted folder (found by its `*.hxef/` prefix,
//!   shown by its decrypted name), and opening one.
//! - [`load`] and [`cache`]: heads and sidecars from the bucket, with the ETag cache in the vault.
//! - [`tree`]: the view of a folder from its sidecars.
//! - [`ops`]: list, stat, read and create inside; [`manage`]: rename, delete and the versions they
//!   replace; [`cleanup`]: what stays behind; [`source`]: reading ranges block by block;
//!   [`transfer`]: sealing uploads.
//!
//! Opened folders ([`EncryptedFolders`]) keep their keys in memory as `Zeroizing` until the vault
//! closes. Nothing here logs a name, a path or an object key of an encrypted folder (FR-027).

pub mod cache;
pub mod cleanup;
pub mod format;
pub mod load;
pub mod manage;
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

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use zeroize::Zeroizing;

use self::format::keys::FolderKeys;
use self::format::sidecar::{seal_sidecar, FileFacts, Sidecar, WrappedDek};
use self::format::{encode_id, FormatError, Id};
use self::load::LoadedFolder;
use self::tree::{Inputs, Tree, Version};
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
        Ok(tokio::sync::MutexGuard::map(guard, |view| {
            view.as_mut().expect("loaded just above")
        }))
    }

    /// Replaces the view with a new load.
    pub async fn reload(&self, loaded: LoadedFolder) {
        *self.view.lock().await = Some(FolderView::new(self.fid, loaded));
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

/// What a folder holds: every version that opened, what failed, and the tree built from them.
pub struct FolderView {
    versions: HashMap<Id, Version>,
    broken: Vec<Id>,
    contents: HashMap<Id, u64>,
    damaged: HashSet<Id>,
    pub tree: Tree,
}

impl FolderView {
    fn new(fid: Id, loaded: LoadedFolder) -> Self {
        let mut view = Self {
            versions: loaded
                .versions
                .into_iter()
                .map(|row| {
                    (
                        row.sid,
                        Version {
                            sid: row.sid,
                            sidecar: row.sidecar,
                        },
                    )
                })
                .collect(),
            broken: loaded.broken,
            contents: loaded.contents,
            damaged: HashSet::new(),
            tree: Tree::build(&Inputs {
                fid,
                versions: &[],
                broken: &[],
                contents: None,
                damaged: &HashSet::new(),
            }),
        };
        view.rebuild(fid);
        view
    }

    /// Adds a version this device wrote (its content object, if any, of `content_len` bytes).
    pub fn add(&mut self, fid: Id, version: Version, content: Option<(Id, u64)>) {
        if let Some((oid, len)) = content {
            self.contents.insert(oid, len);
        }
        self.versions.insert(version.sid, version);
        self.rebuild(fid);
    }

    /// Drops versions this device deleted; their content objects go with them unless another
    /// version still points at one (a rename keeps the content).
    pub fn remove(&mut self, fid: Id, sids: &[Id]) {
        for sid in sids {
            if let Some(version) = self.versions.remove(sid) {
                if let Some(facts) = tree::facts(&version) {
                    if !self.references(&facts.content, &[]) {
                        self.contents.remove(&facts.content);
                    }
                }
            }
        }
        self.rebuild(fid);
    }

    /// Whether a version of this view other than `except` points at the content object `oid`.
    pub fn references(&self, oid: &Id, except: &[Id]) -> bool {
        self.versions.values().any(|version| {
            !except.contains(&version.sid)
                && tree::facts(version).is_some_and(|facts| facts.content == *oid)
        })
    }

    /// The version `sid`, whether it shows or not.
    pub fn version(&self, sid: &Id) -> Option<&Version> {
        self.versions.get(sid)
    }

    /// The versions of `entry` another version of it names as `base`: replaced, never shown.
    pub fn superseded(&self, entry: &Id) -> Vec<Version> {
        let of_entry: Vec<&Version> = self
            .versions
            .values()
            .filter(|version| version.sidecar.entry == *entry)
            .collect();
        of_entry
            .iter()
            .filter(|version| {
                of_entry
                    .iter()
                    .any(|other| other.sidecar.base == Some(version.sid))
            })
            .map(|version| (*version).clone())
            .collect()
    }

    /// `gone` and the versions that would show again without them: a replaced version whose
    /// every successor is in the set (format "Ansicht": a version nobody names counts).
    pub fn with_replaced(&self, gone: &[Id]) -> Vec<Version> {
        let mut out: HashSet<Id> = gone.iter().copied().collect();
        let entries: HashSet<Id> = gone
            .iter()
            .filter_map(|sid| self.versions.get(sid))
            .map(|version| version.sidecar.entry)
            .collect();
        let candidates: Vec<&Version> = self
            .versions
            .values()
            .filter(|version| entries.contains(&version.sidecar.entry))
            .collect();
        loop {
            let mut grown = false;
            for version in &candidates {
                if out.contains(&version.sid) {
                    continue;
                }
                let mut successors = candidates
                    .iter()
                    .filter(|other| other.sidecar.base == Some(version.sid))
                    .peekable();
                if successors.peek().is_some() && successors.all(|other| out.contains(&other.sid)) {
                    out.insert(version.sid);
                    grown = true;
                }
            }
            if !grown {
                break;
            }
        }
        let mut versions: Vec<Version> = out
            .iter()
            .filter_map(|sid| self.versions.get(sid).cloned())
            .collect();
        // Older versions first: what stays after a failure never shows an older one again.
        versions.sort_by_key(|version| (version.sidecar.revision, version.sid));
        versions
    }

    fn rebuild(&mut self, fid: Id) {
        let mut versions: Vec<Version> = self.versions.values().cloned().collect();
        versions.sort_by_key(|version| version.sid);
        self.tree = Tree::build(&Inputs {
            fid,
            versions: &versions,
            broken: &self.broken,
            contents: Some(&self.contents),
            damaged: &self.damaged,
        });
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
