//! Encrypted folders on this device (spec 048 US7, research R16, R17): the device is a storage
//! behind the one storage seam ([`crate::remote_storage::local::LocalStore`]) with the id
//! `device:/` (`device:C:` per drive on Windows), and its keys are the paths without that root.
//!
//! The window sees an encrypted folder on the device as in a storage, by its decrypted name:
//! `/home/anna/Unterlagen/Belege` goes through `/home/anna/<fid>.hxef/` when the head there opens to
//! "Unterlagen". Such a path does not exist on the disk, so a path that exists is an ordinary one
//! and only a missing one is looked up among the heads of its levels ([`route_device`]). On Unix
//! and Android the window path in the storage view is the device path itself; on Windows the drive
//! is cut off and the separators turn ([`DeviceRoot::view_of`], [`DeviceRoot::device_of`]).

use std::collections::HashSet;
use std::path::{Component, Path, PathBuf, Prefix};
use std::sync::Arc;

use tokio::time::Instant;

use super::cache;
use super::format::is_folder_prefix;
use super::route::{decorate, folder_entry, route, vault_error, Route};
use super::Scope;
use crate::files::storage_source::{folder_prefix, storage_error, StorageFiles};
use crate::files::{Entry, EntryKind, FilesError, FilesErrorCode};
use crate::remote_storage::local::access_of;
use crate::remote_storage::RemoteStore;

/// The most entries of one device folder holzi reads to name its encrypted folders.
const MAX_ENTRIES: usize = 100_000;

/// The root of a device path as a storage: `/`, or a drive on Windows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceRoot {
    /// The folder the keys start in.
    pub dir: PathBuf,
    /// `device:/` or `device:C:`.
    pub storage_id: String,
}

impl DeviceRoot {
    /// The root of the absolute `path`; `None` for a relative path or a network share.
    pub fn of(path: &Path) -> Option<Self> {
        if !path.is_absolute() {
            return None;
        }
        match path.components().next()? {
            Component::RootDir => Some(Self {
                dir: PathBuf::from("/"),
                storage_id: "device:/".to_owned(),
            }),
            Component::Prefix(prefix) => match prefix.kind() {
                Prefix::Disk(letter) | Prefix::VerbatimDisk(letter) => {
                    let letter = char::from(letter).to_ascii_uppercase();
                    Some(Self {
                        dir: PathBuf::from(format!("{letter}:\\")),
                        storage_id: format!("device:{letter}:"),
                    })
                }
                _ => None,
            },
            _ => None,
        }
    }

    /// The window path of the storage view for the device path `path` (`/a/b`).
    pub fn view_of(&self, path: &Path) -> String {
        let mut view = String::new();
        for component in path.components() {
            if let Component::Normal(name) = component {
                view.push('/');
                view.push_str(&name.to_string_lossy());
            }
        }
        if view.is_empty() {
            view.push('/');
        }
        view
    }

    /// The device path of the window path `view` of the storage view.
    pub fn device_of(&self, view: &str) -> String {
        let mut path = self.dir.clone();
        for part in view.split('/').filter(|part| !part.is_empty()) {
            path.push(part);
        }
        path.to_string_lossy().into_owned()
    }

    /// `entry` of the storage view with its device path.
    pub fn entry(&self, mut entry: Entry) -> Entry {
        entry.path = self.device_of(&entry.path);
        entry
    }

    /// The device as a storage with `store` (its local store) and `scope`.
    pub fn files(&self, store: Arc<dyn RemoteStore>, scope: Scope) -> StorageFiles {
        StorageFiles::new(store, access_of(&self.dir)).with_scope(scope)
    }
}

/// Whether a part of `path` is the raw directory of an encrypted folder; the window never shows
/// one, so such a path is not a path of the window.
pub fn has_raw_part(path: &Path) -> bool {
    path.components().any(|component| match component {
        Component::Normal(name) => name.to_str().is_some_and(is_folder_prefix),
        _ => false,
    })
}

fn raw_path() -> FilesError {
    FilesError::new(FilesErrorCode::NotFound, "not found")
}

/// The window path of the storage view when the device path `path` leads into or onto an
/// encrypted folder; `None` for an ordinary path (it exists, or no encrypted folder has its name).
/// A raw path through an encrypted folder's directory is `notFound`.
pub async fn route_device(
    files: &StorageFiles,
    root: &DeviceRoot,
    path: &Path,
) -> Result<Option<String>, FilesError> {
    if has_raw_part(path) {
        return Err(raw_path());
    }
    if tokio::fs::symlink_metadata(path).await.is_ok() {
        return Ok(None);
    }
    let view = root.view_of(path);
    match route(files, &view).await {
        Ok(Route::Encrypted(_)) => Ok(Some(view)),
        // A level that cannot be read leaves the path to the device, which tells why.
        Ok(Route::Plain) | Err(_) => Ok(None),
    }
}

/// The entries of the ordinary device folder `folder` with each encrypted folder's directory
/// shown by its head (name and state, as a storage lists it); the rest stays as listed.
pub async fn name_encrypted(
    files: &StorageFiles,
    root: &DeviceRoot,
    folder: &Path,
    entries: Vec<Entry>,
) -> Result<Vec<Entry>, FilesError> {
    let raw = |entry: &Entry| entry.kind == EntryKind::Dir && is_folder_prefix(&entry.name);
    let scope = files.scope().ok_or_else(super::route::in_encrypted)?;
    let view = root.view_of(folder);
    let prefix = folder_prefix(&view)?;
    let present: HashSet<String> = entries
        .iter()
        .filter(|entry| raw(entry))
        .map(|entry| entry.name.clone())
        .collect();
    // A level this session already knows without encrypted folders needs no look at the cache.
    let known_plain = scope
        .folders
        .heads(&scope.storage_id, &prefix)
        .is_some_and(|heads| heads.is_empty());
    if present.is_empty() && known_plain {
        return Ok(entries);
    }
    drop_gone(scope, &prefix, present.clone()).await?;
    if present.is_empty() {
        scope
            .folders
            .set_heads(&scope.storage_id, &prefix, Vec::new());
        return Ok(entries);
    }
    let listing = files
        .store()
        .list_dir(
            files.access(),
            &prefix,
            MAX_ENTRIES,
            Instant::now() + std::time::Duration::from_secs(30),
        )
        .await
        .map_err(storage_error)?;
    let heads = decorate(files, scope, &prefix, &listing).await?;
    let mut named: Vec<Entry> = entries.into_iter().filter(|entry| !raw(entry)).collect();
    named.extend(
        heads
            .iter()
            .map(|head| root.entry(folder_entry(head, &view))),
    );
    Ok(named)
}

/// Forgets the cached folders directly in `prefix` whose directory is not among `present` any more
/// (another program moved or deleted it): their head rows and their entry rows (research R16).
async fn drop_gone(
    scope: &Scope,
    prefix: &str,
    present: HashSet<String>,
) -> Result<(), FilesError> {
    let db = scope.db()?;
    let (storage, parent) = (scope.storage_id.clone(), prefix.to_owned());
    let cached = db
        .read(move |q| cache::heads_under(q, &storage, &parent))
        .await
        .map_err(vault_error)?;
    let gone: Vec<String> = cached
        .into_iter()
        .filter(|row| {
            let rest = &row.object_key[prefix.len()..];
            rest.strip_suffix("/h")
                .is_some_and(|name| !name.contains('/') && !present.contains(name))
        })
        .map(|row| row.folder_id)
        .collect();
    if gone.is_empty() {
        return Ok(());
    }
    let storage = scope.storage_id.clone();
    db.write(move |tx| {
        for folder_id in &gone {
            cache::drop_folder(tx, &storage, folder_id)?;
        }
        Ok(())
    })
    .await
    .map_err(vault_error)
}

#[cfg(test)]
#[path = "device_tests.rs"]
mod tests;
