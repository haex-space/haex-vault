//! The read operations on the serving device (spec 056 FR-012, FR-032): a listing, the state of one
//! entry, a range of a file, and the shares of a contact. Device files go through
//! [`LocalStore`](crate::remote_storage::local::LocalStore), whose resolution keeps every key inside
//! its root (FR-006).

use std::time::Duration;

use tokio::io::AsyncRead;
use tokio::time::Instant;

use super::roots::Located;
use super::ServeContext;
use crate::files::peer::wire::{FailCode, WireEntry, WireKind, WireShare};
use crate::files::share::store;
use crate::files::storage_time::parse_iso_ms;
use crate::remote_storage::local::access_of;
use crate::remote_storage::local_fs::etag_of;
use crate::remote_storage::{ObjectStore, StorageError};

/// How long one operation may take before its answer starts.
const OP_LIMIT: Duration = Duration::from_secs(30);

fn deadline() -> Instant {
    Instant::now() + OP_LIMIT
}

/// A failure of the local store as the other side may see it, without any text (FR-042).
pub fn fail_of(error: StorageError) -> FailCode {
    match error {
        StorageError::NotFound => FailCode::NotFound,
        StorageError::AccessDenied | StorageError::MissingRight => FailCode::Refused,
        StorageError::TooLarge => FailCode::TooLarge,
        StorageError::Unsupported => FailCode::Unsupported,
        StorageError::Conflict => FailCode::Conflict,
        StorageError::Network
        | StorageError::TimedOut
        | StorageError::QuotaExceeded
        | StorageError::HostKeyChanged
        | StorageError::HostKeyUnknown
        | StorageError::Protocol => FailCode::SourceUnavailable,
    }
}

/// The shares this device serves to `vault`, with its level (the `Shares` request of a contact).
pub async fn shares(ctx: &ServeContext, vault: [u8; 32]) -> Result<Vec<WireShare>, FailCode> {
    let (device, vault) = (ctx.device.clone(), crate::sync::keys::hex(&vault));
    let rows = ctx
        .db
        .read(move |q| Ok(store::shares_for_vault(q, &device, &vault)?))
        .await
        .map_err(|error| {
            log::warn!("files: the shares of a contact could not be read: {error}");
            FailCode::Internal
        })?;
    Ok(rows
        .into_iter()
        .map(|(share, level)| WireShare {
            id: share.id,
            name: share.name,
            item_kind: share.item_kind,
            level,
        })
        .collect())
}

/// One level of the folder `at`, at most `max` entries; without entries in holzi's own places when
/// `hide_own`, and only the shared file of a file share.
pub async fn list(
    ctx: &ServeContext,
    at: &Located,
    max: usize,
    hide_own: bool,
) -> Result<Vec<WireEntry>, FailCode> {
    let prefix = at.folder_prefix();
    let listing = ctx
        .store
        .list_dir(&access_of(&at.root), &prefix, max, deadline())
        .await
        .map_err(fail_of)?;
    let files = listing.objects.into_iter().map(|object| WireEntry {
        name: object
            .key
            .strip_prefix(&prefix)
            .unwrap_or(&object.key)
            .to_owned(),
        kind: WireKind::File,
        size: object.size,
        modified_ms: parse_iso_ms(&object.last_modified),
        etag: object.etag,
    });
    let folders = listing.prefixes.into_iter().map(|folder| WireEntry {
        name: folder
            .strip_prefix(&prefix)
            .unwrap_or(&folder)
            .trim_end_matches('/')
            .to_owned(),
        kind: WireKind::Folder,
        size: 0,
        modified_ms: None,
        etag: String::new(),
    });
    let dir = at.path();
    Ok(folders
        .chain(files)
        .filter(|entry| at.only.as_deref().is_none_or(|only| entry.name == only))
        .filter(|entry| !(hide_own && ctx.own.contains_resolved(&dir.join(&entry.name))))
        .collect())
}

/// The state of the entry `at`.
pub async fn stat(ctx: &ServeContext, at: &Located) -> Result<WireEntry, FailCode> {
    // The store's resolution refuses a key that leaves the root through a link; only then is the
    // path read directly.
    ctx.store
        .head(&access_of(&at.root), &at.key, deadline())
        .await
        .map_err(fail_of)?;
    let path = at.path();
    let meta = tokio::task::spawn_blocking(move || std::fs::metadata(path))
        .await
        .map_err(|_| FailCode::Internal)?
        .map_err(|_| FailCode::NotFound)?;
    let name = at.key.rsplit('/').next().unwrap_or_default().to_owned();
    Ok(WireEntry {
        name,
        kind: if meta.is_dir() {
            WireKind::Folder
        } else {
            WireKind::File
        },
        size: if meta.is_dir() { 0 } else { meta.len() },
        modified_ms: meta
            .modified()
            .ok()
            .map(crate::passwords::clock::unix_millis),
        etag: etag_of(&meta),
    })
}

/// A reader of at most `len` bytes of the file `at` from `start`, the file's size, and how many
/// bytes the reader gives.
pub async fn open_read(
    ctx: &ServeContext,
    at: &Located,
    start: u64,
    len: u64,
) -> Result<(u64, u64, Box<dyn AsyncRead + Send + Unpin>), FailCode> {
    if at.key.is_empty() {
        return Err(FailCode::NotFound);
    }
    let access = access_of(&at.root);
    let head = ctx
        .store
        .head(&access, &at.key, deadline())
        .await
        .map_err(fail_of)?;
    let len = len.min(head.size.saturating_sub(start));
    let reader = ctx
        .store
        .get_range(&access, &at.key, start, len, deadline())
        .await
        .map_err(fail_of)?;
    Ok((head.size, len, reader))
}
