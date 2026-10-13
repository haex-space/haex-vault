//! Encrypted folders of a storage by their id, for callers that know a folder only by its id and
//! never by a path of the window (extensions, spec 048 US6, contracts/bridge.md): the readable
//! folders a storage holds, found by a walk through its ordinary folders, and the folder with a
//! given id, from the cache of heads when this device read it, else by the walk.
//!
//! A copy of an encrypted folder elsewhere in the same storage keeps its id (format "Kopieren beim
//! Anbieter"); both answer to it, and the one with the smallest prefix is the one reached.

use std::collections::{HashSet, VecDeque};
use std::sync::Arc;

use super::cache;
use super::format::{encode_id, Id};
use super::route::{in_encrypted, located_at, vault_error, Located};
use crate::files::access::is_device_storage;
use crate::files::storage_source::{folder_prefix, StorageFiles};
use crate::files::{EncryptedState, EntryKind, FilesError, FilesErrorCode};

/// The most ordinary folders one walk lists.
pub const MAX_FOLDERS: usize = 10_000;

/// The areas of extensions in a bucket (spec 038 FR-010): no encrypted folder is made there.
const AREAS: [&str; 2] = ["holzi-ext", "holzi-ext-dev"];

fn not_found() -> FilesError {
    FilesError::new(FilesErrorCode::NotFound, "not found")
}

fn readable(at: &Located) -> bool {
    at.head.state == EncryptedState::Readable && at.head.fid.is_some()
}

/// Every readable encrypted folder of the storage, one per id, by their decrypted names.
pub async fn readable_folders(files: &StorageFiles) -> Result<Vec<Located>, FilesError> {
    let mut found: Vec<Located> = walk(files).await?.into_iter().filter(readable).collect();
    found.sort_by(|a, b| a.head.prefix.cmp(&b.head.prefix));
    let mut seen = HashSet::new();
    found.retain(|at| seen.insert(at.head.fid));
    found.sort_by(|a, b| a.head.display.cmp(&b.head.display));
    Ok(found)
}

/// The readable encrypted folder `fid` of the storage, or `notFound`.
pub async fn find(files: &StorageFiles, fid: &Id) -> Result<Located, FilesError> {
    let scope = files.scope().ok_or_else(in_encrypted)?;
    let (storage, folder_id) = (scope.storage_id.clone(), encode_id(fid));
    let cached = scope
        .db()?
        .read(move |q| cache::heads_of_folder(q, &storage, &folder_id))
        .await
        .map_err(vault_error)?;
    for (object_key, _) in cached {
        let Some(prefix) = object_key.strip_suffix('h') else {
            continue;
        };
        if let Ok(at) = located_at(files, prefix).await {
            if readable(&at) && at.head.fid == Some(*fid) {
                return Ok(at);
            }
        }
    }
    // The device is never walked: it is the whole file system (spec 048 US7).
    if is_device_storage(&scope.storage_id) {
        return Err(not_found());
    }
    walk(files)
        .await?
        .into_iter()
        .filter(|at| readable(at) && at.head.fid == Some(*fid))
        .min_by(|a, b| a.head.prefix.cmp(&b.head.prefix))
        .ok_or_else(not_found)
}

/// The readable encrypted folders this vault's cache knows on the device `files` (spec 048 US7),
/// one per id, by their decrypted names; the device is never walked.
pub async fn known_on_device(files: &StorageFiles) -> Result<Vec<Located>, FilesError> {
    let scope = files.scope().ok_or_else(in_encrypted)?;
    let storage = scope.storage_id.clone();
    let cached = scope
        .db()?
        .read(move |q| cache::head_keys(q, &storage))
        .await
        .map_err(vault_error)?;
    let mut found = Vec::new();
    for object_key in cached {
        let Some(prefix) = object_key.strip_suffix('h') else {
            continue;
        };
        if let Ok(at) = located_at(files, prefix).await {
            if readable(&at) {
                found.push(at);
            }
        }
    }
    found.sort_by(|a, b| a.head.prefix.cmp(&b.head.prefix));
    let mut seen = HashSet::new();
    found.retain(|at| seen.insert(at.head.fid));
    found.sort_by(|a, b| a.head.display.cmp(&b.head.display));
    Ok(found)
}

/// Every encrypted folder below the ordinary folders of the storage, at most [`MAX_FOLDERS`]
/// listings; extension areas and the inside of encrypted folders are not walked. A level that
/// cannot be listed below the root is skipped.
async fn walk(files: &StorageFiles) -> Result<Vec<Located>, FilesError> {
    let scope = files.scope().ok_or_else(in_encrypted)?;
    let mut found = Vec::new();
    let mut folders = VecDeque::from(["/".to_owned()]);
    let mut listed = 0;
    while let Some(path) = folders.pop_front() {
        if listed >= MAX_FOLDERS {
            break;
        }
        listed += 1;
        let entries = match files.list(&path).await {
            Ok(entries) => entries,
            Err(error) if path == "/" => return Err(error),
            Err(_) => continue,
        };
        let heads = scope
            .folders
            .heads(&scope.storage_id, &folder_prefix(&path)?)
            .unwrap_or_default();
        for head in heads {
            let root = if path == "/" {
                format!("/{}", head.display)
            } else {
                format!("{path}/{}", head.display)
            };
            found.push(Located {
                head: Arc::clone(&head),
                root,
                rest: Vec::new(),
            });
        }
        for entry in entries {
            let area = path == "/" && AREAS.contains(&entry.name.as_str());
            if entry.kind == EntryKind::Dir && entry.encrypted.is_none() && !area {
                folders.push_back(entry.path);
            }
        }
    }
    Ok(found)
}

#[cfg(test)]
#[path = "by_id_tests.rs"]
mod tests;
