//! What WebDAV and SFTP share as servers with real folders (spec 050 research R7–R9): the names of
//! holzi's temporary files, the parent folder of a key, and a listing over many levels that reads
//! one folder at a time and shows folders like S3 shows its folder markers (`ordner/`, size 0).

use futures::stream::{self, StreamExt};
use uuid::Uuid;

use super::{ObjectInfo, StorageError};

/// The start of the name of a temporary file holzi writes before it moves it to its name.
pub(super) const PART_PREFIX: &str = ".holzi-part-";

/// How many folders a listing reads at once (spec 050 research R12).
const PARALLEL_READS: usize = 4;

/// One entry of one folder as a server lists it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct DirEntry {
    /// The name in its folder, without a `/`.
    pub name: String,
    pub is_dir: bool,
    pub size: u64,
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
/// `ordner/` of size 0; temporary files are left out. More than `max` entries is
/// [`StorageError::TooLarge`].
pub(super) async fn list<F, Fut>(
    prefix: &str,
    max: usize,
    read_dir: F,
) -> Result<Vec<ObjectInfo>, StorageError>
where
    F: Fn(String) -> Fut,
    Fut: std::future::Future<Output = Result<Vec<DirEntry>, StorageError>>,
{
    let mut found = Vec::new();
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
    Ok(found)
}
