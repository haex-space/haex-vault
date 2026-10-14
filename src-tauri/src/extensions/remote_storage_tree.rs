//! The object methods of extensions on a server with real folders (WebDAV, SFTP; spec 050 US3,
//! research R9), so an extension gets the answers it gets on S3: keys a server cannot store are
//! refused, a new name that differs from one in its folder only in case is refused instead of
//! hitting the other file, listings show no folders, and deleting the last file of a folder
//! removes the folders it leaves empty, up to the extension's area and never above it.

use tokio::time::Instant;

use super::error::BridgeError;
use super::remote_storage::invalid;
use super::remote_storage_keys::fits_tree;
use crate::remote_storage::{case_clash, Access, ObjectStore, StorageError};

/// The most entries of one folder holzi reads to compare names.
const MAX_ENTRIES: usize = 100_000;

/// 3001 for an extension's key or prefix `text` a server with real folders cannot store; on S3
/// nothing changes.
pub(super) fn check_key(access: &Access, text: &str, what: &str) -> Result<(), BridgeError> {
    if access.location.is_tree() && !fits_tree(text) {
        return Err(invalid(&format!("invalid {what}")));
    }
    Ok(())
}

/// The folder of `key` (with `/` at the end, or empty) and its name.
fn split(key: &str) -> (&str, &str) {
    match key.rfind('/') {
        Some(slash) => (&key[..=slash], &key[slash + 1..]),
        None => ("", key),
    }
}

/// Whether writing the file `key` would hit another file: its folder holds a name that differs
/// only in case, and not the name itself.
pub(super) async fn clashes(
    store: &dyn ObjectStore,
    access: &Access,
    key: &str,
    deadline: Instant,
) -> Result<bool, StorageError> {
    let (folder, name) = split(key);
    let listing = match store.list_dir(access, folder, MAX_ENTRIES, deadline).await {
        Ok(listing) => listing,
        Err(StorageError::NotFound) => return Ok(false),
        Err(error) => return Err(error),
    };
    let names: Vec<&str> = listing
        .prefixes
        .iter()
        .map(|prefix| prefix[folder.len()..].trim_end_matches('/'))
        .chain(
            listing
                .objects
                .iter()
                .map(|object| &object.key[folder.len()..]),
        )
        .collect();
    Ok(!names.contains(&name) && case_clash(names, name).is_some())
}

/// After the file `key` went: removes the folders above it that are empty now, from the deepest,
/// below `area` (the extension's area, ending in `/`). It stops at the first folder that holds
/// something; a failure is only logged, the file is gone either way.
pub(super) async fn prune(
    store: &dyn ObjectStore,
    access: &Access,
    area: &str,
    key: &str,
    deadline: Instant,
) {
    let mut folder = split(key).0;
    while folder.len() > area.len() && folder.starts_with(area) {
        match store.list_dir(access, folder, 1, deadline).await {
            Ok(listing) if listing.objects.is_empty() && listing.prefixes.is_empty() => {}
            Ok(_) | Err(StorageError::TooLarge) => return,
            Err(error) => {
                log::warn!("remote storage of an extension: empty folders stay: {error:?}");
                return;
            }
        }
        if let Err(error) = store.delete(access, folder, deadline).await {
            log::warn!("remote storage of an extension: an empty folder stays: {error:?}");
            return;
        }
        folder = split(&folder[..folder.len() - 1]).0;
    }
}
