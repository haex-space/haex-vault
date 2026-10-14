//! Reading encrypted folders from a storage (research R6, R8). Heads of the `*.hxef/` prefixes in a
//! listing load in parallel: one `HEAD` each for the ETag, a `GET` only when the cache does not know
//! that ETag. A folder loads whole when it is opened: one listing of its prefix (sidecars and
//! content objects), then only the sidecars whose ETag is new or missing in the cache, at most
//! [`AT_ONCE`] at a time. Nothing here touches the vault database: callers pass the cached rows in
//! and write back what changed.

use std::collections::HashMap;
use std::time::Duration;

use futures::{StreamExt, TryStreamExt};
use tokio::time::Instant;
use zeroize::Zeroizing;

use super::cache::{EntryRow, HeadRow};
use super::format::head::{open_head, HeadOutcome};
use super::format::keys::FolderKeys;
use super::format::sidecar::open_sidecar;
use super::format::{decode_id, folder_id, Id};
use super::vault_keys::HeldKeys;
use crate::files::storage_source::{parse_http_date_ms, parse_iso_ms, storage_error};
use crate::files::{EncryptedState, FilesError};
use crate::remote_storage::{Access, ObjectStore, StorageError};

/// The most requests for heads or sidecars in flight at once.
pub const AT_ONCE: usize = 32;

/// The largest head holzi reads (one recipient is 91 bytes; spaces add a few).
pub const MAX_HEAD: usize = 64 * 1024;

/// The largest sidecar holzi reads (a name is at most 255 bytes).
const MAX_SIDECAR: usize = 64 * 1024;

/// The most objects under one encrypted folder: sidecars and content objects of 100 000 entries.
const MAX_OBJECTS: usize = 250_000;

const CALL: Duration = Duration::from_secs(30);

fn soon() -> Instant {
    Instant::now() + CALL
}

/// One encrypted folder as its parent's listing found it.
pub struct LoadedHead {
    /// `P`, ending in `/`.
    pub prefix: String,
    pub fid: Option<Id>,
    pub state: EncryptedState,
    pub name: Option<String>,
    pub key_id: Option<Id>,
    pub etag: Option<String>,
    /// The head's modification time at the provider.
    pub modified_ms: Option<i64>,
    /// The folder key, when the head was read now and opened.
    pub fk: Option<Zeroizing<[u8; 32]>>,
    /// Read now (not from the cache): the cache learns it.
    pub fresh: bool,
}

impl LoadedHead {
    fn without_head(prefix: &str, fid: Option<Id>, state: EncryptedState) -> Self {
        Self {
            prefix: prefix.to_owned(),
            fid,
            state,
            name: None,
            key_id: None,
            etag: None,
            modified_ms: None,
            fk: None,
            fresh: false,
        }
    }
}

/// The heads of the encrypted folders `prefixes`, sorted by prefix.
pub async fn load_heads(
    store: &dyn ObjectStore,
    access: &Access,
    prefixes: &[String],
    cached: &[HeadRow],
    held: &HeldKeys,
) -> Result<Vec<LoadedHead>, FilesError> {
    let mut heads: Vec<LoadedHead> = futures::stream::iter(prefixes.iter().cloned())
        .map(|prefix| async move { load_head(store, access, &prefix, cached, held).await })
        .buffer_unordered(AT_ONCE)
        .try_collect()
        .await?;
    heads.sort_by(|a, b| a.prefix.cmp(&b.prefix));
    Ok(heads)
}

async fn load_head(
    store: &dyn ObjectStore,
    access: &Access,
    prefix: &str,
    cached: &[HeadRow],
    held: &HeldKeys,
) -> Result<LoadedHead, FilesError> {
    let name = prefix
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or("");
    let Some(fid) = folder_id(name) else {
        return Ok(LoadedHead::without_head(
            prefix,
            None,
            EncryptedState::Damaged,
        ));
    };
    let key = format!("{prefix}h");
    let head = match store.head(access, &key, soon()).await {
        Ok(head) => head,
        Err(StorageError::NotFound) => {
            return Ok(LoadedHead::without_head(
                prefix,
                Some(fid),
                EncryptedState::Damaged,
            ))
        }
        Err(error) => return Err(storage_error(error)),
    };
    let modified_ms = head.last_modified.as_deref().and_then(parse_http_date_ms);
    // Another vault's head is read again: this device may have received that generation since.
    let known = cached.iter().find(|row| {
        row.object_key == key
            && Some(&row.etag) == head.etag.as_ref()
            && row.state != EncryptedState::OtherVault
    });
    if let Some(row) = known {
        return Ok(LoadedHead {
            prefix: prefix.to_owned(),
            fid: Some(fid),
            state: row.state,
            name: row.name.clone(),
            key_id: row.key_id,
            etag: head.etag,
            modified_ms,
            fk: None,
            fresh: false,
        });
    }
    let bytes = match store.get(access, &key, MAX_HEAD, soon()).await {
        Ok(bytes) => bytes,
        Err(StorageError::NotFound | StorageError::TooLarge) => {
            return Ok(LoadedHead::without_head(
                prefix,
                Some(fid),
                EncryptedState::Damaged,
            ))
        }
        Err(error) => return Err(storage_error(error)),
    };
    let mut loaded = LoadedHead {
        prefix: prefix.to_owned(),
        fid: Some(fid),
        state: EncryptedState::Damaged,
        name: None,
        key_id: None,
        etag: head.etag,
        modified_ms,
        fk: None,
        fresh: true,
    };
    match open_head(&bytes, &fid, |key_id| held.by_key_id(key_id)) {
        HeadOutcome::Readable(opened) => {
            loaded.state = EncryptedState::Readable;
            loaded.name = Some(opened.name);
            loaded.key_id = Some(opened.key_id);
            loaded.fk = Some(opened.fk);
        }
        HeadOutcome::OtherVault => loaded.state = EncryptedState::OtherVault,
        HeadOutcome::NewerFormat => loaded.state = EncryptedState::NewerFormat,
        HeadOutcome::Damaged => {}
    }
    Ok(loaded)
}

/// The head of the folder `prefix` with `fid`, read and opened now.
pub async fn read_head(
    store: &dyn ObjectStore,
    access: &Access,
    prefix: &str,
    fid: &Id,
    held: &HeldKeys,
) -> Result<HeadOutcome, FilesError> {
    match store
        .get(access, &format!("{prefix}h"), MAX_HEAD, soon())
        .await
    {
        Ok(bytes) => Ok(open_head(&bytes, fid, |key_id| held.by_key_id(key_id))),
        Err(StorageError::NotFound | StorageError::TooLarge) => Ok(HeadOutcome::Damaged),
        Err(error) => Err(storage_error(error)),
    }
}

/// What one load of a folder found.
#[derive(Debug, Default)]
pub struct LoadedFolder {
    /// Every sidecar that opened: from the cache or read now.
    pub versions: Vec<EntryRow>,
    /// The sidecars read now, for the cache.
    pub fetched: Vec<EntryRow>,
    /// Cached sidecars that are gone from the bucket.
    pub vanished: Vec<Id>,
    /// Sidecars that did not open.
    pub broken: Vec<Id>,
    /// The content objects by `oid` with their length.
    pub contents: HashMap<Id, u64>,
    /// When the provider last changed each sidecar and content object, ms (for the cleanup).
    pub times: HashMap<Id, i64>,
    /// Files under the folder that are not `h`, `m/<id>` or `c/<id>` (research R19): never entries,
    /// never cleaned up, removed only with the whole folder.
    pub foreign: u32,
}

/// Loads the folder `prefix` (`P`) with id `fid`; `cached` are the rows the cache holds for it.
pub async fn load_folder(
    store: &dyn ObjectStore,
    access: &Access,
    prefix: &str,
    fid: &Id,
    keys: &FolderKeys,
    cached: Vec<EntryRow>,
) -> Result<LoadedFolder, FilesError> {
    let objects = store
        .list(access, prefix, MAX_OBJECTS, soon())
        .await
        .map_err(storage_error)?;
    let mut loaded = LoadedFolder::default();
    let mut listed: Vec<(Id, String)> = Vec::new();
    for object in objects {
        let rest = &object.key[prefix.len()..];
        let time = parse_iso_ms(&object.last_modified);
        if let Some(sid) = rest.strip_prefix("m/").and_then(decode_id) {
            loaded.times.extend(time.map(|time| (sid, time)));
            listed.push((sid, object.etag));
        } else if let Some(oid) = rest.strip_prefix("c/").and_then(decode_id) {
            loaded.times.extend(time.map(|time| (oid, time)));
            loaded.contents.insert(oid, object.size);
        } else if rest != "h" && !rest.is_empty() && !rest.ends_with('/') {
            // Folder markers are no files; holzi's temporary files never show in a listing.
            loaded.foreign = loaded.foreign.saturating_add(1);
        }
    }
    let mut cached: HashMap<Id, EntryRow> = cached.into_iter().map(|row| (row.sid, row)).collect();
    let mut wanted = Vec::new();
    for (sid, etag) in listed {
        match cached.remove(&sid) {
            Some(row) if row.etag == etag => loaded.versions.push(row),
            _ => wanted.push((sid, etag)),
        }
    }
    loaded.vanished = cached.into_keys().collect();
    let read: Vec<(Id, String, Option<Option<super::format::sidecar::Sidecar>>)> =
        futures::stream::iter(wanted)
            .map(|(sid, etag)| async move {
                let key = format!("{prefix}m/{}", super::format::encode_id(&sid));
                match store.get(access, &key, MAX_SIDECAR, soon()).await {
                    Ok(bytes) => Ok((sid, etag, Some(open_sidecar(keys, fid, &sid, &bytes).ok()))),
                    // Deleted since the listing: superseded by another device.
                    Err(StorageError::NotFound) => Ok((sid, etag, None)),
                    Err(StorageError::TooLarge) => Ok((sid, etag, Some(None))),
                    Err(error) => Err(storage_error(error)),
                }
            })
            .buffer_unordered(AT_ONCE)
            .try_collect()
            .await?;
    for (sid, etag, sidecar) in read {
        match sidecar {
            Some(Some(sidecar)) => {
                let row = EntryRow { sid, etag, sidecar };
                loaded.versions.push(row.clone());
                loaded.fetched.push(row);
            }
            Some(None) => loaded.broken.push(sid),
            None => {}
        }
    }
    Ok(loaded)
}

#[cfg(test)]
#[path = "load_tests.rs"]
mod tests;
