//! A storage of spec 038 as a source of the file browser (spec 044 US5, FR-034, FR-038). The window
//! sees a storage like a device: paths start with `/`, `/Fotos/2026` is the prefix `Fotos/2026/`
//! and `/Fotos/a.jpg` the key `Fotos/a.jpg`; only this module knows keys. S3 has no folders: a
//! folder is a prefix, kept even when empty by a marker object named like the prefix. Renaming
//! copies on the provider and deletes; a folder's objects are renamed or deleted one by one.
//! Errors say what kind of failure it was, never the endpoint, a key of the connection or the
//! provider's text (FR-038).

use std::sync::Arc;
use std::time::Duration;

use tokio::io::AsyncReadExt;
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;

use crate::files::encrypted::route::{self, decorate, route, Route};
use crate::files::encrypted::{manage, ops, Scope};
use crate::files::kind::mime_for;
use crate::files::local::edit::check_name;
pub use crate::files::storage_time::{parse_http_date_ms, parse_iso_ms};
use crate::files::{Entry, EntryKind, FilesError, FilesErrorCode};
use crate::remote_storage::{Access, ObjectInfo, ObjectStore, StorageError};

/// How long one call to the provider may take.
const CALL: Duration = Duration::from_secs(30);

/// How long a copy may take (the provider copies up to 5 GB).
const COPY: Duration = Duration::from_secs(10 * 60);

/// The most entries of one level, and of objects under a folder that is renamed or deleted.
const MAX_ENTRIES: usize = 100_000;

/// How many keys an error about leftovers names.
const NAMED_LEFTOVERS: usize = 5;

/// The error of a provider's failure (FR-038: no text of the provider).
pub fn storage_error(error: StorageError) -> FilesError {
    match error {
        StorageError::NotFound => FilesError::new(FilesErrorCode::NotFound, "not found"),
        StorageError::AccessDenied => FilesError::new(
            FilesErrorCode::Credentials,
            "the storage refused the credentials",
        ),
        StorageError::MissingRight => FilesError::new(
            FilesErrorCode::NoAccess,
            "the credentials lack the right for this",
        ),
        StorageError::Network | StorageError::TimedOut | StorageError::Protocol => {
            FilesError::new(FilesErrorCode::Unreachable, "the storage is not reachable")
        }
        StorageError::HostKeyChanged | StorageError::HostKeyUnknown => FilesError::new(
            FilesErrorCode::HostKey,
            "the server's host key is not the confirmed one",
        ),
        StorageError::TooLarge => FilesError::new(FilesErrorCode::TooLarge, "too many entries"),
        StorageError::QuotaExceeded => {
            FilesError::new(FilesErrorCode::NoSpace, "the storage is full")
        }
        StorageError::Unsupported => {
            FilesError::new(FilesErrorCode::Unsupported, "the storage cannot do this")
        }
    }
}

/// Whether `name` may name an ordinary entry of a storage: the rules of [`check_name`], and not the
/// form of an encrypted folder (spec 048), so it never shows as a damaged one.
pub fn check_storage_name(name: &str) -> Result<(), FilesError> {
    check_name(name)?;
    if crate::files::encrypted::format::is_folder_prefix(name) {
        return Err(FilesError::new(
            FilesErrorCode::InvalidName,
            "this name is kept for encrypted folders",
        ));
    }
    Ok(())
}

fn soon() -> Instant {
    Instant::now() + CALL
}

/// The parts of a window path; `/` has none. `.`, `..` and empty parts are refused.
pub(crate) fn parts(path: &str) -> Result<Vec<&str>, FilesError> {
    let rest = path
        .strip_prefix('/')
        .ok_or_else(|| FilesError::invalid_path("a storage path starts with /"))?;
    if rest.is_empty() {
        return Ok(Vec::new());
    }
    let parts: Vec<&str> = rest.split('/').collect();
    if parts
        .iter()
        .any(|part| part.is_empty() || *part == "." || *part == "..")
    {
        return Err(FilesError::invalid_path(
            "the path holds an empty part, . or ..",
        ));
    }
    Ok(parts)
}

/// The prefix of the folder `path` (`""` for `/`).
pub fn folder_prefix(path: &str) -> Result<String, FilesError> {
    Ok(parts(path)?.iter().map(|part| format!("{part}/")).collect())
}

/// The key of the file `path`.
pub fn object_key(path: &str) -> Result<String, FilesError> {
    let parts = parts(path)?;
    if parts.is_empty() {
        return Err(FilesError::invalid_path("the root is no file"));
    }
    Ok(parts.join("/"))
}

/// The window path of a key or a prefix.
pub fn path_of(key: &str) -> String {
    format!("/{}", key.trim_end_matches('/'))
}

fn name_of(key: &str) -> String {
    key.trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or_default()
        .to_owned()
}

fn file_entry(key: &str, size: u64, modified_ms: Option<i64>) -> Entry {
    let name = name_of(key);
    Entry {
        mime: mime_for(&name).map(str::to_owned),
        hidden: name.starts_with('.'),
        path: path_of(key),
        name,
        kind: EntryKind::File,
        size: Some(size),
        modified_ms,
        symlink: false,
        no_access: false,
        holzi_owned: false,
        encrypted: None,
        conflict: false,
    }
}

fn folder_entry(prefix: &str) -> Entry {
    let name = if prefix.is_empty() {
        "/".to_owned()
    } else {
        name_of(prefix)
    };
    Entry {
        hidden: name.starts_with('.'),
        path: path_of(prefix),
        name,
        kind: EntryKind::Dir,
        size: None,
        modified_ms: None,
        mime: None,
        symlink: false,
        no_access: false,
        holzi_owned: false,
        encrypted: None,
        conflict: false,
    }
}

/// The files of one storage. With a [`Scope`] (the window) paths go through encrypted folders by
/// their decrypted names (spec 048, [`route`]); without one such paths are refused. On a server
/// with real folders (WebDAV, SFTP; spec 050) finding, renaming and deleting an entry go their own
/// way (`storage_source_tree.rs`).
pub struct StorageFiles {
    store: Arc<dyn ObjectStore>,
    access: Access,
    scope: Option<Scope>,
}

impl StorageFiles {
    pub fn new(store: Arc<dyn ObjectStore>, access: Access) -> Self {
        Self {
            store,
            access,
            scope: None,
        }
    }

    pub fn with_scope(self, scope: Scope) -> Self {
        Self {
            scope: Some(scope),
            ..self
        }
    }

    pub fn scope(&self) -> Option<&Scope> {
        self.scope.as_ref()
    }

    pub fn store(&self) -> &Arc<dyn ObjectStore> {
        &self.store
    }

    pub fn access(&self) -> &Access {
        &self.access
    }

    /// Every entry of the folder `path`, unsorted (the window sorts).
    pub async fn list(&self, path: &str) -> Result<Vec<Entry>, FilesError> {
        if let Route::Encrypted(at) = route(self, path).await? {
            return ops::list(self, &at).await;
        }
        let prefix = folder_prefix(path)?;
        let listing = self
            .store
            .list_dir(&self.access, &prefix, MAX_ENTRIES, soon())
            .await
            .map_err(storage_error)?;
        let heads = match &self.scope {
            Some(scope) => decorate(self, scope, &prefix, &listing).await?,
            None => Vec::new(),
        };
        let folders = listing
            .prefixes
            .iter()
            .filter(|sub| !heads.iter().any(|head| head.prefix == **sub))
            .map(|prefix| folder_entry(prefix))
            .chain(heads.iter().map(|head| route::folder_entry(head, path)));
        let files = listing.objects.iter().map(|object| {
            file_entry(
                &object.key,
                object.size,
                parse_iso_ms(&object.last_modified),
            )
        });
        Ok(folders.chain(files).collect())
    }

    /// Whether anything lies under `prefix` (a folder exists as long as it has a marker or
    /// objects).
    async fn has_prefix(&self, prefix: &str) -> Result<bool, FilesError> {
        match self.store.list(&self.access, prefix, 1, soon()).await {
            Ok(objects) => Ok(!objects.is_empty()),
            Err(StorageError::TooLarge) => Ok(true),
            Err(error) => Err(storage_error(error)),
        }
    }

    /// The entry at `path`: a file when its key exists, else a folder when its prefix does.
    pub async fn stat(&self, path: &str) -> Result<Entry, FilesError> {
        if let Route::Encrypted(at) = route(self, path).await? {
            return ops::stat(self, &at).await;
        }
        if self.access.location.is_tree() {
            return self.tree_stat(path).await;
        }
        let prefix = folder_prefix(path)?;
        if prefix.is_empty() {
            return Ok(folder_entry(""));
        }
        let key = object_key(path)?;
        match self.store.head(&self.access, &key, soon()).await {
            Ok(head) => {
                let modified = head.last_modified.as_deref().and_then(parse_http_date_ms);
                return Ok(file_entry(&key, head.size, modified));
            }
            Err(StorageError::NotFound) => {}
            Err(error) => return Err(storage_error(error)),
        }
        if self.has_prefix(&prefix).await? {
            Ok(folder_entry(&prefix))
        } else {
            Err(FilesError::new(FilesErrorCode::NotFound, "not found"))
        }
    }

    /// Whether `path` names a file or a folder already (on a server with real folders also one
    /// whose name differs only in case).
    async fn taken(&self, path: &str) -> Result<bool, FilesError> {
        if self.access.location.is_tree() {
            return self.tree_taken(path, None).await;
        }
        match self.stat(path).await {
            Ok(_) => Ok(true),
            Err(error) if error.code == FilesErrorCode::NotFound => Ok(false),
            Err(error) => Err(error),
        }
    }

    /// Creates the folder `name` in `folder` (its marker object).
    pub async fn create_folder(&self, folder: &str, name: &str) -> Result<Entry, FilesError> {
        if let Route::Encrypted(at) = route(self, folder).await? {
            return ops::create_folder(self, &at, name).await;
        }
        check_storage_name(name)?;
        let prefix = format!("{}{name}/", folder_prefix(folder)?);
        if self.taken(&path_of(&prefix)).await? {
            return Err(FilesError::new(FilesErrorCode::Exists, "the name is taken"));
        }
        self.store
            .put(&self.access, &prefix, Vec::new(), soon())
            .await
            .map_err(storage_error)?;
        Ok(folder_entry(&prefix))
    }

    /// Every object under `prefix`, the marker included.
    async fn objects_under(&self, prefix: &str) -> Result<Vec<ObjectInfo>, FilesError> {
        self.store
            .list(&self.access, prefix, MAX_ENTRIES, soon())
            .await
            .map_err(storage_error)
    }

    /// Renames the entry `path` in its folder: a file is copied on the provider and deleted, a
    /// folder object by object.
    pub async fn rename(&self, path: &str, new_name: &str) -> Result<Entry, FilesError> {
        if let Route::Encrypted(at) = route(self, path).await? {
            return manage::rename(self, &at, new_name).await;
        }
        check_storage_name(new_name)?;
        let entry = self.stat(path).await?;
        let parent = match path.rfind('/') {
            Some(0) | None => "/",
            Some(slash) => &path[..slash],
        };
        let target = if parent == "/" {
            format!("/{new_name}")
        } else {
            format!("{parent}/{new_name}")
        };
        if target == path {
            return Ok(entry);
        }
        if self.access.location.is_tree() {
            if self.tree_taken(&target, Some(&entry.name)).await? {
                return Err(FilesError::new(FilesErrorCode::Exists, "the name is taken"));
            }
            return self.tree_rename(&entry, path, &target).await;
        }
        if self.taken(&target).await? {
            return Err(FilesError::new(FilesErrorCode::Exists, "the name is taken"));
        }
        if entry.kind == EntryKind::File {
            let (from, to) = (object_key(path)?, object_key(&target)?);
            self.move_object(&from, &to).await?;
            return self.stat(&target).await;
        }
        let (from, to) = (folder_prefix(path)?, folder_prefix(&target)?);
        for object in self.objects_under(&from).await? {
            let rest = &object.key[from.len()..];
            self.move_object(&object.key, &format!("{to}{rest}"))
                .await?;
        }
        Ok(folder_entry(&to))
    }

    async fn move_object(&self, from: &str, to: &str) -> Result<(), FilesError> {
        self.store
            .copy(&self.access, from, to, Instant::now() + COPY)
            .await
            .map_err(storage_error)?;
        self.store
            .delete(&self.access, from, soon())
            .await
            .map_err(storage_error)
    }

    /// Deletes the entry `path` for good (a storage has no trash, FR-023): a folder's objects one
    /// by one, its marker last. `progress` hears how many objects are gone; a failure names what
    /// is left.
    pub async fn delete(
        &self,
        path: &str,
        cancel: &CancellationToken,
        progress: &mut (dyn FnMut(u64) + Send),
    ) -> Result<(), FilesError> {
        if let Route::Encrypted(at) = route(self, path).await? {
            return manage::delete(self, &at, cancel, progress).await;
        }
        let kind = self.stat(path).await?.kind;
        if self.access.location.is_tree() {
            return self.tree_delete(path, kind, progress).await;
        }
        if kind == EntryKind::File {
            let key = object_key(path)?;
            self.store
                .delete(&self.access, &key, soon())
                .await
                .map_err(storage_error)?;
            progress(1);
            return Ok(());
        }
        let prefix = folder_prefix(path)?;
        if prefix.is_empty() {
            return Err(FilesError::invalid_path("the root of a storage stays"));
        }
        let mut keys: Vec<String> = self
            .objects_under(&prefix)
            .await?
            .into_iter()
            .map(|object| object.key)
            .collect();
        // The marker last, so a folder stays visible until it is empty; the head of an encrypted
        // folder in it after the folder's other objects (spec 048, format "Löschen des Ordners").
        keys.sort_by_key(|key| (*key == prefix, key.ends_with(".hxef/h")));
        for (done, key) in keys.iter().enumerate() {
            if cancel.is_cancelled() {
                return Err(FilesError::new(
                    FilesErrorCode::Unsupported,
                    "cancelled with objects left",
                ));
            }
            if let Err(error) = self.store.delete(&self.access, key, soon()).await {
                let left = &keys[done..];
                let named: Vec<&str> = left
                    .iter()
                    .take(NAMED_LEFTOVERS)
                    .map(String::as_str)
                    .collect();
                let error = storage_error(error);
                return Err(FilesError::new(
                    error.code,
                    format!(
                        "{} objects are left: {}{}",
                        left.len(),
                        named.join(", "),
                        if left.len() > NAMED_LEFTOVERS {
                            ", …"
                        } else {
                            ""
                        }
                    ),
                ));
            }
            progress(done as u64 + 1);
        }
        Ok(())
    }

    /// The first `limit + 1` bytes of the file `path` of `size` bytes (for the text viewer; one
    /// more than `limit` says there is more).
    pub async fn read_start(
        &self,
        path: &str,
        size: u64,
        limit: u64,
    ) -> Result<Vec<u8>, FilesError> {
        if let Route::Encrypted(at) = route(self, path).await? {
            return ops::read_start(self, &at, limit).await;
        }
        let key = object_key(path)?;
        let len = size.min(limit.saturating_add(1));
        let mut reader = self
            .store
            .get_range(&self.access, &key, 0, len, soon())
            .await
            .map_err(storage_error)?;
        let mut bytes = Vec::new();
        reader
            .read_to_end(&mut bytes)
            .await
            .map_err(|_| storage_error(StorageError::Network))?;
        Ok(bytes)
    }
}

#[path = "storage_source_tree.rs"]
mod tree;

#[cfg(test)]
#[path = "storage_source_tests.rs"]
mod tests;
#[cfg(test)]
#[path = "storage_source_tree_tests.rs"]
mod tree_tests;
