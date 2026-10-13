//! A storage on a server with real folders (WebDAV, SFTP; spec 050 US2, research R7–R9) in the file
//! browser. An entry is found in the listing of its folder, a file or a folder is renamed or
//! deleted with one call to the server, and a name that differs from one in its folder only in
//! case counts as taken: some servers treat both as one, and writing it would hit the other entry.

use tokio::time::Instant;
use uuid::Uuid;

use super::{
    file_entry, folder_entry, folder_prefix, object_key, parts, soon, storage_error, StorageFiles,
    COPY, MAX_ENTRIES,
};
use crate::files::storage_time::parse_iso_ms;
use crate::files::{Entry, EntryKind, FilesError, FilesErrorCode};
use crate::remote_storage::{case_clash, DirListing, StorageError};

/// The start of the name an entry has between the two steps of a rename that changes only case.
const RENAMING: &str = ".holzi-rename-";

/// The folder of `key` (a file, or a folder ending in `/`) as a prefix, and its name in it.
fn split_key(key: &str) -> (&str, &str) {
    let trimmed = key.strip_suffix('/').unwrap_or(key);
    match trimmed.rfind('/') {
        Some(slash) => (&key[..=slash], &trimmed[slash + 1..]),
        None => ("", trimmed),
    }
}

/// The names in one level of a folder, folders without their `/`.
fn names<'a>(listing: &'a DirListing, folder: &str) -> impl Iterator<Item = &'a str> {
    let strip = folder.len();
    listing
        .prefixes
        .iter()
        .map(move |prefix| prefix[strip..].trim_end_matches('/'))
        .chain(
            listing
                .objects
                .iter()
                .map(move |object| &object.key[strip..]),
        )
}

impl StorageFiles {
    async fn level(&self, folder: &str) -> Result<DirListing, StorageError> {
        self.store
            .list_dir(&self.access, folder, MAX_ENTRIES, soon())
            .await
    }

    /// Whether `key` (a file, or a folder ending in `/`) or a name that differs from it only in
    /// case lies in its folder. `except` does not count: the name of an entry that is renamed.
    pub(crate) async fn key_taken(
        &self,
        key: &str,
        except: Option<&str>,
    ) -> Result<bool, StorageError> {
        let (folder, name) = split_key(key);
        let listing = match self.level(folder).await {
            Ok(listing) => listing,
            Err(StorageError::NotFound) => return Ok(false),
            Err(error) => return Err(error),
        };
        let others: Vec<&str> = names(&listing, folder)
            .filter(|other| Some(*other) != except)
            .collect();
        Ok(others.contains(&name) || case_clash(others, name).is_some())
    }

    /// The entry at `path`, from the listing of its folder (a server may redirect a folder asked
    /// for without its `/`, which holzi does not follow).
    pub(super) async fn tree_stat(&self, path: &str) -> Result<Entry, FilesError> {
        let parts = parts(path)?;
        let Some((name, above)) = parts.split_last() else {
            return Ok(folder_entry(""));
        };
        let folder: String = above.iter().map(|part| format!("{part}/")).collect();
        let listing = self.level(&folder).await.map_err(storage_error)?;
        let prefix = format!("{folder}{name}/");
        if listing.prefixes.contains(&prefix) {
            return Ok(folder_entry(&prefix));
        }
        let key = format!("{folder}{name}");
        listing
            .objects
            .iter()
            .find(|object| object.key == key)
            .map(|object| {
                file_entry(
                    &object.key,
                    object.size,
                    parse_iso_ms(&object.last_modified),
                )
            })
            .ok_or_else(|| FilesError::new(FilesErrorCode::NotFound, "not found"))
    }

    /// Whether `path` names an entry already, or one whose name differs only in case.
    pub(super) async fn tree_taken(
        &self,
        path: &str,
        except: Option<&str>,
    ) -> Result<bool, FilesError> {
        let key = object_key(path)?;
        self.key_taken(&key, except).await.map_err(storage_error)
    }

    /// Renames `entry` at `path` to `target` in the same folder with one move on the server. A
    /// rename that changes only case goes through a passing name, as a server that treats both as
    /// one could otherwise overwrite the entry with itself.
    pub(super) async fn tree_rename(
        &self,
        entry: &Entry,
        path: &str,
        target: &str,
    ) -> Result<Entry, FilesError> {
        let folder = entry.kind == EntryKind::Dir;
        let key_of = |path: &str| {
            if folder {
                folder_prefix(path)
            } else {
                object_key(path)
            }
        };
        let (from, to) = (key_of(path)?, key_of(target)?);
        if path.to_lowercase() != target.to_lowercase() {
            self.move_tree(&from, &to).await?;
        } else {
            let (above, _) = split_key(&from);
            let mut passing = format!("{above}{RENAMING}{}", Uuid::new_v4());
            if folder {
                passing.push('/');
            }
            self.move_tree(&from, &passing).await?;
            if let Err(error) = self.move_tree(&passing, &to).await {
                return Err(FilesError::new(
                    error.code,
                    format!("the entry is left as {}", split_key(&passing).1),
                ));
            }
        }
        if folder {
            Ok(folder_entry(&to))
        } else {
            self.tree_stat(target).await
        }
    }

    async fn move_tree(&self, from: &str, to: &str) -> Result<(), FilesError> {
        self.store
            .rename_tree(&self.access, from, to, Instant::now() + COPY)
            .await
            .map_err(storage_error)
    }

    /// Deletes the entry `path` of `kind` with one call; `progress` hears one step.
    pub(super) async fn tree_delete(
        &self,
        path: &str,
        kind: EntryKind,
        progress: &mut (dyn FnMut(u64) + Send),
    ) -> Result<(), FilesError> {
        let result = if kind == EntryKind::File {
            let key = object_key(path)?;
            self.store.delete(&self.access, &key, soon()).await
        } else {
            let prefix = folder_prefix(path)?;
            if prefix.is_empty() {
                return Err(FilesError::invalid_path("the root of a storage stays"));
            }
            self.store
                .remove_tree(&self.access, &prefix, Instant::now() + COPY)
                .await
        };
        result.map_err(storage_error)?;
        progress(1);
        Ok(())
    }
}
