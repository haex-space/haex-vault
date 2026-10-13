//! What WebDAV and SFTP share as servers with real folders (spec 050 research R7–R9): the names of
//! holzi's temporary files, the parent folder of a key, listings of one level and of many levels
//! that show folders like S3 shows its folder markers (`ordner/`, size 0), temporary files a broken
//! upload left behind, and names that differ only in case.

use futures::stream::{self, StreamExt};
use uuid::Uuid;

use super::{DirListing, ObjectInfo, StorageError};
use crate::files::storage_time::parse_iso_ms;

/// The start of the name of a temporary file holzi writes before it moves it to its name.
pub(super) const PART_PREFIX: &str = ".holzi-part-";

/// How many folders a listing reads at once (spec 050 research R12).
const PARALLEL_READS: usize = 4;

/// How old a temporary file gets before a listing deletes it: an upload lasts a day at most
/// (research R7), so an older one was left behind by an upload that broke off.
const STALE_AFTER_MS: i64 = 24 * 60 * 60 * 1000;

/// One entry of one folder as a server lists it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct DirEntry {
    /// The name in its folder, without a `/`.
    pub name: String,
    pub is_dir: bool,
    pub size: u64,
    /// The time of the last change as a listing of S3 gives it (`2026-10-06T10:00:00.000Z`), or
    /// empty when the server sent none.
    pub modified: String,
    pub etag: String,
}

/// A new name for a temporary file.
pub(super) fn part_name() -> String {
    format!("{PART_PREFIX}{}", Uuid::new_v4())
}

/// Whether `name` (the last part of a key) is one of holzi's temporary files; no listing shows
/// them (FR-022).
pub(super) fn is_part(name: &str) -> bool {
    name.starts_with(PART_PREFIX)
}

/// Whether `entry` is a temporary file older than a day at `now_ms` (milliseconds since 1970). One
/// without a time stays: holzi cannot tell its age.
fn stale(entry: &DirEntry, now_ms: i64) -> bool {
    !entry.is_dir
        && is_part(&entry.name)
        && parse_iso_ms(&entry.modified).is_some_and(|at| now_ms - at > STALE_AFTER_MS)
}

/// A name among `names` that differs from `name` only in case (research R9). Some servers (Koofr,
/// a NAS on Windows) treat both as one, so writing `name` would hit the other file.
pub fn case_clash<'a>(names: impl IntoIterator<Item = &'a str>, name: &str) -> Option<&'a str> {
    let lower = name.to_lowercase();
    names
        .into_iter()
        .find(|other| *other != name && other.to_lowercase() == lower)
}

/// One level of the folder `prefix` (a key ending in `/`, or empty) from its `entries`, as S3
/// answers with `delimiter=/`: folders as prefixes, files as objects, temporary files left out.
/// Also returns the keys of temporary files older than a day, for the store to delete. More than
/// `max` entries is [`StorageError::TooLarge`].
pub(super) fn level(
    prefix: &str,
    entries: Vec<DirEntry>,
    max: usize,
    now_ms: i64,
) -> Result<(DirListing, Vec<String>), StorageError> {
    let mut listing = DirListing::default();
    let mut stale_parts = Vec::new();
    for entry in entries {
        if is_part(&entry.name) {
            if stale(&entry, now_ms) {
                stale_parts.push(format!("{prefix}{}", entry.name));
            }
            continue;
        }
        if entry.is_dir {
            listing.prefixes.push(format!("{prefix}{}/", entry.name));
        } else {
            listing.objects.push(ObjectInfo {
                key: format!("{prefix}{}", entry.name),
                size: entry.size,
                last_modified: entry.modified,
                etag: entry.etag,
            });
        }
    }
    if listing.objects.len() + listing.prefixes.len() > max {
        return Err(StorageError::TooLarge);
    }
    listing.prefixes.sort();
    listing.objects.sort_by(|a, b| a.key.cmp(&b.key));
    Ok((listing, stale_parts))
}

/// The folder of `key` with a `/` at the end, or empty for a key in the top folder. A folder key
/// (ending in `/`) has its parent as folder.
pub(super) fn parent_of(key: &str) -> &str {
    let trimmed = key.strip_suffix('/').unwrap_or(key);
    match trimmed.rfind('/') {
        Some(slash) => &key[..=slash],
        None => "",
    }
}

/// Every parent folder of `key` from the top, each with a `/` at the end: `a/`, `a/b/` for
/// `a/b/c.txt`.
pub(super) fn ancestors(key: &str) -> Vec<&str> {
    let parent = parent_of(key);
    parent
        .match_indices('/')
        .map(|(at, _)| &parent[..=at])
        .collect()
}

/// The objects under `prefix` as S3 would list them, read folder by folder from the folder of
/// `prefix` with `read_dir` (which gets a folder key, empty for the top). Folders are entries
/// `ordner/` of size 0; temporary files are left out, and those older than a day at `now_ms` come
/// back as keys for the store to delete. More than `max` entries is [`StorageError::TooLarge`].
pub(super) async fn list<F, Fut>(
    prefix: &str,
    max: usize,
    now_ms: i64,
    read_dir: F,
) -> Result<(Vec<ObjectInfo>, Vec<String>), StorageError>
where
    F: Fn(String) -> Fut,
    Fut: std::future::Future<Output = Result<Vec<DirEntry>, StorageError>>,
{
    let mut found = Vec::new();
    let mut stale_parts = Vec::new();
    let mut pending = vec![parent_of(prefix).to_owned()];
    while !pending.is_empty() {
        let batch: Vec<String> = pending.drain(..pending.len().min(PARALLEL_READS)).collect();
        let listings: Vec<(String, Result<Vec<DirEntry>, StorageError>)> = stream::iter(batch)
            .map(|dir| {
                let read = read_dir(dir.clone());
                async move { (dir, read.await) }
            })
            .buffer_unordered(PARALLEL_READS)
            .collect()
            .await;
        for (dir, entries) in listings {
            for entry in entries? {
                if is_part(&entry.name) {
                    if stale(&entry, now_ms) {
                        stale_parts.push(format!("{dir}{}", entry.name));
                    }
                    continue;
                }
                let key = if entry.is_dir {
                    format!("{dir}{}/", entry.name)
                } else {
                    format!("{dir}{}", entry.name)
                };
                // A folder is worth reading only when keys under it can start with the prefix.
                if entry.is_dir && (key.starts_with(prefix) || prefix.starts_with(&key)) {
                    pending.push(key.clone());
                }
                if key.starts_with(prefix) {
                    found.push(ObjectInfo {
                        key,
                        size: if entry.is_dir { 0 } else { entry.size },
                        last_modified: entry.modified,
                        etag: entry.etag,
                    });
                    if found.len() > max {
                        return Err(StorageError::TooLarge);
                    }
                }
            }
        }
    }
    found.sort_by(|a, b| a.key.cmp(&b.key));
    Ok((found, stale_parts))
}
