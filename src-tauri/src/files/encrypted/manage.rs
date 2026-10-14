//! Renaming and deleting in and of encrypted folders (spec 048 US5, FR-018, research R7; format
//! "Regeln für Leser und Schreiber"):
//!
//! - An entry is renamed or moved by one new sidecar: same `entry`, `revision + 1`, `base` = the
//!   sidecar it replaces, the same content. The versions it replaces go after it; no content
//!   object is written (SC-006).
//! - The folder itself is renamed by a new head with a new `name_nonce`; nothing else is written.
//! - Deleting an entry removes the content objects and sidecars of everything below it, deepest
//!   first, its own sidecar last; deleting the folder removes every object under `P`, the head last.
//!
//! A version goes together with the older versions that would show again without it (a replaced
//! version counts once nobody names it as `base`).

use std::collections::HashMap;
use std::sync::Arc;

use tokio::time::Instant;
use tokio_util::sync::CancellationToken;

use super::cache;
use super::format::head::rename_head;
use super::format::sidecar::Sidecar;
use super::format::{encode_id, Id};
use super::load::MAX_HEAD;
use super::ops::{child_named, now_ms, stat, write_version};
use super::route::{forget_level, open, vault_error, view, Located};
use super::tree::{facts, Item, Version};
use super::{damaged, format_error, new_id, OpenFolder};
use crate::files::local::edit::check_name;
use crate::files::storage_source::{storage_error, StorageFiles};
use crate::files::{Entry, FilesError, FilesErrorCode};

/// The most objects under one encrypted folder that deleting it takes (as loading it).
const MAX_OBJECTS: usize = 250_000;

fn soon() -> Instant {
    Instant::now() + std::time::Duration::from_secs(30)
}

fn not_found() -> FilesError {
    FilesError::new(FilesErrorCode::NotFound, "not found")
}

fn cancelled() -> FilesError {
    FilesError::new(FilesErrorCode::Unsupported, "cancelled with objects left")
}

/// Renames the encrypted folder at `at` (its head) or the entry at `at` (its sidecar).
pub async fn rename(files: &StorageFiles, at: &Located, name: &str) -> Result<Entry, FilesError> {
    check_name(name)?;
    if at.rest.is_empty() {
        return rename_folder(files, at, name).await;
    }
    let folder = open(files, at).await?;
    let version = {
        let view = view(files, &folder).await?;
        let index = view.tree.find(&at.rest).ok_or_else(not_found)?;
        let version = shown_version(&view.tree.node(index).item)?;
        let parent = view
            .tree
            .find(&at.rest[..at.rest.len() - 1])
            .ok_or_else(not_found)?;
        if view.tree.node(index).name == name {
            None
        } else if child_named(&view.tree, parent, name) {
            return Err(FilesError::new(FilesErrorCode::Exists, "the name is taken"));
        } else {
            Some(version)
        }
    };
    if let Some(version) = version {
        let parent = version.sidecar.parent;
        relink(files, &folder, &version, parent, name).await?;
    }
    let mut rest = at.rest.clone();
    if let Some(last) = rest.last_mut() {
        name.clone_into(last);
    }
    stat(
        files,
        &Located {
            head: Arc::clone(&at.head),
            root: at.root.clone(),
            rest,
        },
    )
    .await
}

/// The version a node shows, for renaming or moving it; a damaged entry can only be deleted.
pub fn shown_version(item: &Item) -> Result<Version, FilesError> {
    match item {
        Item::Version {
            version,
            damaged: false,
        } => Ok((**version).clone()),
        Item::Version { damaged: true, .. } | Item::Broken { .. } => Err(damaged()),
        Item::Root | Item::Recovered => Err(FilesError::invalid_path("this folder stays")),
    }
}

/// Writes `version` anew as `name` below the entry `parent` (a rename or a move inside the
/// folder), then drops the versions it replaces. Returns the new version.
pub async fn relink(
    files: &StorageFiles,
    folder: &OpenFolder,
    version: &Version,
    parent: Id,
    name: &str,
) -> Result<Version, FilesError> {
    let new = Version {
        sid: new_id(),
        sidecar: Sidecar {
            parent,
            name: name.to_owned(),
            revision: version.sidecar.revision + 1,
            base: Some(version.sid),
            written: now_ms(),
            ..version.sidecar.clone()
        },
    };
    write_version(files, folder, new.clone(), None).await?;
    drop_replaced(files, folder, &new.sidecar.entry).await;
    Ok(new)
}

/// Drops the versions of `entry` another version replaced (format "Schreiben"). The new version
/// is written already; what fails here stays hidden and goes with the cleanup.
pub async fn drop_replaced(files: &StorageFiles, folder: &OpenFolder, entry: &Id) {
    let Ok(replaced) = view(files, folder).await.map(|view| view.superseded(entry)) else {
        return;
    };
    let _ = drop_versions(
        files,
        folder,
        &replaced,
        &CancellationToken::new(),
        &mut |_| {},
    )
    .await;
}

/// Deletes `versions` in their order: each one's content object (unless a version that stays
/// points at it), then its sidecar. `progress` hears how many objects are gone.
pub async fn drop_versions(
    files: &StorageFiles,
    folder: &OpenFolder,
    versions: &[Version],
    cancel: &CancellationToken,
    progress: &mut (dyn FnMut(u64) + Send),
) -> Result<(), FilesError> {
    let sids: Vec<Id> = versions.iter().map(|version| version.sid).collect();
    let contents: Vec<Option<Id>> = {
        let view = view(files, folder).await?;
        versions
            .iter()
            .map(|version| {
                facts(version)
                    .map(|facts| facts.content)
                    .filter(|oid| !view.references(oid, &sids))
            })
            .collect()
    };
    let mut gone = Vec::new();
    let mut objects = 0;
    let mut result = Ok(());
    for (version, content) in versions.iter().zip(contents) {
        if cancel.is_cancelled() {
            result = Err(cancelled());
            break;
        }
        let mut keys: Vec<String> = content
            .map(|oid| folder.content_key(&oid))
            .into_iter()
            .collect();
        keys.push(folder.sidecar_key(&version.sid));
        if let Err(error) = delete_keys(files, &keys, &mut objects, progress).await {
            result = Err(error);
            break;
        }
        gone.push(version.sid);
    }
    view(files, folder).await?.remove(&gone);
    result
}

async fn delete_keys(
    files: &StorageFiles,
    keys: &[String],
    done: &mut u64,
    progress: &mut (dyn FnMut(u64) + Send),
) -> Result<(), FilesError> {
    for key in keys {
        files
            .store()
            .delete(files.access(), key, soon())
            .await
            .map_err(storage_error)?;
        *done += 1;
        progress(*done);
    }
    Ok(())
}

/// Deletes the encrypted folder at `at` (everything under `P`) or the entry at `at` with everything
/// below it.
pub async fn delete(
    files: &StorageFiles,
    at: &Located,
    cancel: &CancellationToken,
    progress: &mut (dyn FnMut(u64) + Send),
) -> Result<(), FilesError> {
    if at.rest.is_empty() {
        return delete_folder(files, &at.head.prefix, at.head.fid, cancel, progress).await;
    }
    let folder = open(files, at).await?;
    let (order, broken) = {
        let view = view(files, &folder).await?;
        let index = view.tree.find(&at.rest).ok_or_else(not_found)?;
        // Children before their parents, the entry itself last.
        let mut nodes: Vec<usize> = view
            .tree
            .below(index)
            .into_iter()
            .map(|(node, _)| node)
            .rev()
            .collect();
        nodes.push(index);
        let mut shown = Vec::new();
        let mut broken = Vec::new();
        for &node in &nodes {
            match &view.tree.node(node).item {
                Item::Version { version, .. } => shown.push(version.sid),
                Item::Broken { sid } => broken.push(*sid),
                Item::Root | Item::Recovered => {}
            }
        }
        let mut by_entry: HashMap<Id, Vec<Version>> = HashMap::new();
        for version in view.with_replaced(&shown) {
            by_entry
                .entry(version.sidecar.entry)
                .or_default()
                .push(version);
        }
        let mut order = Vec::new();
        for sid in &shown {
            let entry = view.version(sid).map(|version| version.sidecar.entry);
            if let Some(versions) = entry.and_then(|entry| by_entry.remove(&entry)) {
                order.extend(versions);
            }
        }
        (order, broken)
    };
    let mut objects = 0;
    let keys: Vec<String> = broken.iter().map(|sid| folder.sidecar_key(sid)).collect();
    delete_keys(files, &keys, &mut objects, progress).await?;
    let offset = objects;
    drop_versions(files, &folder, &order, cancel, &mut |n| {
        progress(offset + n)
    })
    .await
}

/// Deletes every object under the encrypted folder `prefix`, the head last, and forgets it.
pub async fn delete_folder(
    files: &StorageFiles,
    prefix: &str,
    fid: Option<Id>,
    cancel: &CancellationToken,
    progress: &mut (dyn FnMut(u64) + Send),
) -> Result<(), FilesError> {
    let head = format!("{prefix}h");
    let mut keys: Vec<String> = files
        .store()
        .list(files.access(), prefix, MAX_OBJECTS, soon())
        .await
        .map_err(storage_error)?
        .into_iter()
        .map(|object| object.key)
        .filter(|key| *key != head)
        .collect();
    keys.push(head);
    let mut done = 0;
    for key in &keys {
        if cancel.is_cancelled() {
            return Err(cancelled());
        }
        delete_keys(files, std::slice::from_ref(key), &mut done, progress).await?;
    }
    // Real folders (the device, WebDAV, SFTP) would stay behind empty.
    if files.access().location.is_tree() {
        files
            .store()
            .remove_tree(files.access(), prefix, soon())
            .await
            .map_err(storage_error)?;
    }
    forget_level(files, prefix);
    if let (Some(scope), Some(fid)) = (files.scope(), fid) {
        let (storage, folder_id) = (scope.storage_id.clone(), encode_id(&fid));
        scope
            .db()?
            .write(move |tx| cache::drop_folder(tx, &storage, &folder_id))
            .await
            .map_err(vault_error)?;
    }
    Ok(())
}

/// Renames the encrypted folder at `at`: its head with the new name, nothing else (spec edge case
/// "Den verschlüsselten Ordner selbst umbenennen").
async fn rename_folder(
    files: &StorageFiles,
    at: &Located,
    name: &str,
) -> Result<Entry, FilesError> {
    let folder = open(files, at).await?;
    let parent = match at.root.rfind('/') {
        Some(0) | None => "/".to_owned(),
        Some(slash) => at.root[..slash].to_owned(),
    };
    if at.head.display != name
        && files
            .list(&parent)
            .await?
            .iter()
            .any(|entry| entry.name == name)
    {
        return Err(FilesError::new(FilesErrorCode::Exists, "the name is taken"));
    }
    let key = format!("{}h", folder.prefix);
    let (store, access) = (files.store(), files.access());
    let head = store
        .get(access, &key, MAX_HEAD, soon())
        .await
        .map_err(storage_error)?;
    let renamed = rename_head(&head, folder.fk(), name).map_err(format_error)?;
    store
        .put(access, &key, renamed, soon())
        .await
        .map_err(storage_error)?;
    forget_level(files, &folder.prefix);
    let path = if parent == "/" {
        format!("/{name}")
    } else {
        format!("{parent}/{name}")
    };
    files.stat(&path).await
}

#[cfg(test)]
#[path = "manage_tests.rs"]
mod tests;
