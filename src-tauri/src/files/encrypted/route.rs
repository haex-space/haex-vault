//! Which window paths go through an encrypted folder (research R8, R9). The window sees an
//! encrypted folder by its decrypted name: `/Privat/Unterlagen/Belege` goes through the prefix
//! `Privat/<fid>.hxef/` when the head there opens to "Unterlagen". A folder's listing loads the heads
//! of its `*.hxef/` prefixes ([`decorate`]) and keeps them for the session, so later paths through
//! it need no request; a path through a prefix no listing has seen yet loads that level once.

use std::collections::HashSet;
use std::sync::Arc;

use tokio::time::Instant;

use super::cache::{self, HeadRow};
use super::cleanup;
use super::format::head::HeadOutcome;
use super::format::{encode_id, is_folder_prefix};
use super::load::{load_folder, load_heads, read_head, LoadedHead};
use super::tree::conflict_name;
use super::vault_keys::HeldKeys;
use super::{state_error, EncryptedFolders, FolderHead, FolderView, OpenFolder, Scope};
use crate::files::storage_source::{parts, storage_error, StorageFiles};
use crate::files::{
    EncryptedInfo, EncryptedRole, EncryptedState, Entry, EntryKind, FilesError, FilesErrorCode,
};
use crate::remote_storage::DirListing;

/// The most entries of one level holzi lists to find the encrypted folders in it.
const MAX_ENTRIES: usize = 100_000;

/// Where a window path of a storage leads.
pub enum Route {
    /// Ordinary keys and prefixes, as in spec 044.
    Plain,
    /// The encrypted folder `head` (`rest` empty) or an entry below it.
    Encrypted(Located),
}

/// A path through an encrypted folder.
pub struct Located {
    pub head: Arc<FolderHead>,
    /// The window path of the encrypted folder.
    pub root: String,
    /// The names below it.
    pub rest: Vec<String>,
}

impl Located {
    /// The window path of a node at `rel` below the encrypted folder.
    pub fn path_of(&self, rel: &[String]) -> String {
        let mut path = self.root.clone();
        for part in rel {
            path.push('/');
            path.push_str(part);
        }
        path
    }
}

/// The error for a path through an encrypted folder without a scope.
pub fn in_encrypted() -> FilesError {
    FilesError::new(
        FilesErrorCode::InEncrypted,
        "the path goes through an encrypted folder",
    )
}

/// Where `path` leads. Without a scope a path through an encrypted folder's prefix is refused.
pub async fn route(files: &StorageFiles, path: &str) -> Result<Route, FilesError> {
    let parts = parts(path)?;
    let Some(scope) = files.scope() else {
        if parts.iter().any(|part| is_folder_prefix(part)) {
            return Err(in_encrypted());
        }
        return Ok(Route::Plain);
    };
    let mut prefix = String::new();
    for (at, part) in parts.iter().enumerate() {
        let heads = heads_of(files, scope, &prefix).await?;
        if let Some(head) = heads.iter().find(|head| head.display == *part) {
            return Ok(Route::Encrypted(Located {
                head: Arc::clone(head),
                root: format!("/{}", parts[..=at].join("/")),
                rest: parts[at + 1..]
                    .iter()
                    .map(|part| (*part).to_owned())
                    .collect(),
            }));
        }
        if is_folder_prefix(part) {
            // A raw prefix the window never saw: not a path of the window.
            return Err(FilesError::new(FilesErrorCode::NotFound, "not found"));
        }
        prefix.push_str(part);
        prefix.push('/');
    }
    Ok(Route::Plain)
}

/// The encrypted folder at the object prefix `prefix` (`…/<fid>.hxef/`), as its parent's listing
/// names it; for a transfer that finds one inside an ordinary folder (spec 048 T044).
pub async fn located_at(files: &StorageFiles, prefix: &str) -> Result<Located, FilesError> {
    let scope = files.scope().ok_or_else(in_encrypted)?;
    let parent = parent_of(prefix);
    let head = heads_of(files, scope, parent)
        .await?
        .into_iter()
        .find(|head| head.prefix == prefix)
        .ok_or_else(|| FilesError::new(FilesErrorCode::NotFound, "not found"))?;
    let mut root: String = parent
        .split('/')
        .filter(|part| !part.is_empty())
        .map(|part| format!("/{part}"))
        .collect();
    root.push('/');
    root.push_str(&head.display);
    Ok(Located {
        head,
        root,
        rest: Vec::new(),
    })
}

/// Whether this session's listings show the folders of the file `path` of `storage` outside every
/// encrypted folder; `false` while a level was not listed yet.
pub fn plain_by_memory(folders: &EncryptedFolders, storage: &str, path: &str) -> bool {
    let Ok(parts) = parts(path) else {
        return false;
    };
    let mut prefix = String::new();
    for part in parts.iter().take(parts.len().saturating_sub(1)) {
        let Some(heads) = folders.heads(storage, &prefix) else {
            return false;
        };
        if is_folder_prefix(part) || heads.iter().any(|head| head.display == *part) {
            return false;
        }
        prefix.push_str(part);
        prefix.push('/');
    }
    true
}

/// The heads below `prefix`: from this session's listings, else listed now.
async fn heads_of(
    files: &StorageFiles,
    scope: &Scope,
    prefix: &str,
) -> Result<Vec<Arc<FolderHead>>, FilesError> {
    if let Some(heads) = scope.folders.heads(&scope.storage_id, prefix) {
        return Ok(heads);
    }
    let listing = files
        .store()
        .list_dir(
            files.access(),
            prefix,
            MAX_ENTRIES,
            Instant::now() + std::time::Duration::from_secs(30),
        )
        .await
        .map_err(storage_error)?;
    decorate(files, scope, prefix, &listing).await
}

/// Loads the heads of the encrypted folders in `listing` (of `prefix`), names them beside the
/// ordinary entries, keeps them for the session and updates the cache.
pub async fn decorate(
    files: &StorageFiles,
    scope: &Scope,
    prefix: &str,
    listing: &DirListing,
) -> Result<Vec<Arc<FolderHead>>, FilesError> {
    let encrypted: Vec<String> = listing
        .prefixes
        .iter()
        .filter(|sub| is_folder_prefix(raw_name(sub)))
        .cloned()
        .collect();
    if encrypted.is_empty() {
        scope
            .folders
            .set_heads(&scope.storage_id, prefix, Vec::new());
        return Ok(Vec::new());
    }
    let db = scope.db()?;
    let (storage, parent) = (scope.storage_id.clone(), prefix.to_owned());
    let (cached, held) = db
        .read(move |q| {
            Ok((
                cache::heads_under(q, &storage, &parent)?,
                HeldKeys::load(q)?,
            ))
        })
        .await
        .map_err(vault_error)?;
    let loaded = load_heads(
        files.store().as_ref(),
        files.access(),
        &encrypted,
        &cached,
        &held,
    )
    .await?;
    remember(scope, &db, prefix, &loaded, &cached).await?;
    let mut taken: HashSet<String> = listing
        .prefixes
        .iter()
        .filter(|sub| !is_folder_prefix(raw_name(sub)))
        .map(|sub| raw_name(sub).to_owned())
        .chain(
            listing
                .objects
                .iter()
                .map(|object| raw_name(&object.key).to_owned()),
        )
        .collect();
    let mut order: Vec<&LoadedHead> = loaded.iter().collect();
    order.sort_by(|a, b| {
        b.modified_ms
            .cmp(&a.modified_ms)
            .then(a.prefix.cmp(&b.prefix))
    });
    let mut heads = Vec::new();
    for head in order {
        let wanted = match (&head.state, &head.name) {
            (EncryptedState::Readable, Some(name)) => name.clone(),
            _ => raw_name(&head.prefix).to_owned(),
        };
        let mut display = wanted.clone();
        let mut n = 0;
        while taken.contains(&display) {
            display = conflict_name(&wanted, head.modified_ms.unwrap_or(0), false, n);
            n += 1;
        }
        taken.insert(display.clone());
        if let (Some(fk), Some(fid)) = (&head.fk, head.fid) {
            let opened = OpenFolder::new(head.prefix.clone(), fid, head.etag.clone(), fk);
            scope.folders.keep(&scope.storage_id, Arc::new(opened));
        }
        heads.push(Arc::new(FolderHead {
            prefix: head.prefix.clone(),
            fid: head.fid,
            state: head.state,
            conflict: n > 0,
            display,
            modified_ms: head.modified_ms,
            etag: head.etag.clone(),
        }));
    }
    scope
        .folders
        .set_heads(&scope.storage_id, prefix, heads.clone());
    Ok(heads)
}

/// Writes the heads read now to the cache and drops those of folders that are gone.
async fn remember(
    scope: &Scope,
    db: &crate::vault_gate::VaultDb,
    prefix: &str,
    loaded: &[LoadedHead],
    cached: &[HeadRow],
) -> Result<(), FilesError> {
    let rows: Vec<HeadRow> = loaded
        .iter()
        .filter(|head| head.fresh)
        .filter_map(|head| {
            Some(HeadRow {
                object_key: format!("{}h", head.prefix),
                etag: head.etag.clone()?,
                folder_id: encode_id(&head.fid?),
                name: head.name.clone(),
                state: head.state,
                key_id: head.key_id,
                open_ack: false,
            })
        })
        .collect();
    let present: HashSet<String> = loaded
        .iter()
        .map(|head| format!("{}h", head.prefix))
        .collect();
    let gone: Vec<String> = cached
        .iter()
        .map(|row| row.object_key.clone())
        .filter(|key| !present.contains(key) && key.starts_with(prefix))
        .collect();
    if rows.is_empty() && gone.is_empty() {
        return Ok(());
    }
    let storage = scope.storage_id.clone();
    let now = crate::passwords::clock::unix_millis(std::time::SystemTime::now());
    db.write(move |tx| {
        for row in &rows {
            cache::put_head(tx, &storage, row, now)?;
        }
        cache::drop_heads(tx, &storage, &gone)
    })
    .await
    .map_err(vault_error)
}

/// Forgets the heads of the level that holds the encrypted folder `prefix` (it was renamed, moved
/// or deleted), so the next path through that level lists it again.
pub fn forget_level(files: &StorageFiles, prefix: &str) {
    if let Some(scope) = files.scope() {
        scope
            .folders
            .forget_heads(&scope.storage_id, parent_of(prefix));
    }
}

/// The prefix of the level that holds the prefix `prefix` (`""` at the root).
pub fn parent_of(prefix: &str) -> &str {
    let trimmed = prefix.trim_end_matches('/');
    trimmed.rfind('/').map_or("", |slash| &prefix[..=slash])
}

/// The last segment of a key or prefix.
fn raw_name(key: &str) -> &str {
    key.trim_end_matches('/').rsplit('/').next().unwrap_or("")
}

/// A vault failure, without its text (it may quote SQL with a name).
pub fn vault_error(error: impl std::fmt::Display) -> FilesError {
    let _ = error;
    log::warn!("files: the cache of encrypted folders could not be read or written");
    FilesError::new(FilesErrorCode::Unsupported, "the vault could not be read")
}

/// The entry of the encrypted folder `head` in the folder `parent` (a window path).
pub fn folder_entry(head: &FolderHead, parent: &str) -> Entry {
    let path = if parent == "/" {
        format!("/{}", head.display)
    } else {
        format!("{parent}/{}", head.display)
    };
    head_entry(head, path)
}

fn head_entry(head: &FolderHead, path: String) -> Entry {
    Entry {
        hidden: head.display.starts_with('.'),
        name: head.display.clone(),
        path,
        kind: EntryKind::Dir,
        size: None,
        modified_ms: head.modified_ms,
        mime: None,
        symlink: false,
        no_access: false,
        holzi_owned: false,
        encrypted: Some(EncryptedInfo {
            role: EncryptedRole::Folder,
            state: head.state,
            foreign: 0,
        }),
        conflict: head.conflict,
    }
}

/// The entry of the encrypted folder at `at`.
pub fn located_entry(at: &Located) -> Entry {
    head_entry(&at.head, at.root.clone())
}

/// The opened folder at `at`, or why it does not open (`noKey`, its state).
pub async fn open(files: &StorageFiles, at: &Located) -> Result<Arc<OpenFolder>, FilesError> {
    let scope = files.scope().ok_or_else(in_encrypted)?;
    let head = &at.head;
    let (Some(fid), EncryptedState::Readable) = (head.fid, head.state) else {
        return Err(state_error(head.state));
    };
    if let Some(open) = scope
        .folders
        .opened(&scope.storage_id, &head.prefix, head.etag.as_deref())
    {
        return Ok(open);
    }
    let db = scope.db()?;
    let held = db.read(|q| HeldKeys::load(q)).await.map_err(vault_error)?;
    if held.is_empty() {
        return Err(super::vault_keys::no_key());
    }
    match read_head(
        files.store().as_ref(),
        files.access(),
        &head.prefix,
        &fid,
        &held,
    )
    .await?
    {
        HeadOutcome::Readable(opened) => {
            let folder = Arc::new(OpenFolder::new(
                head.prefix.clone(),
                fid,
                head.etag.clone(),
                &opened.fk,
            ));
            scope.folders.keep(&scope.storage_id, Arc::clone(&folder));
            Ok(folder)
        }
        HeadOutcome::OtherVault => Err(state_error(EncryptedState::OtherVault)),
        HeadOutcome::NewerFormat => Err(state_error(EncryptedState::NewerFormat)),
        HeadOutcome::Damaged => Err(state_error(EncryptedState::Damaged)),
    }
}

/// The view of `folder`, loaded with the cache when this session has not loaded it yet.
pub async fn view<'a>(
    files: &StorageFiles,
    folder: &'a OpenFolder,
) -> Result<tokio::sync::MappedMutexGuard<'a, FolderView>, FilesError> {
    folder.view(|| load_view(files, folder)).await
}

/// Loads `folder` from the bucket with the cache, and updates the cache.
pub async fn load_view(
    files: &StorageFiles,
    folder: &OpenFolder,
) -> Result<super::load::LoadedFolder, FilesError> {
    let scope = files.scope().ok_or_else(in_encrypted)?;
    let db = scope.db()?;
    let (storage, folder_id) = (scope.storage_id.clone(), encode_id(&folder.fid));
    let (s, f) = (storage.clone(), folder_id.clone());
    let cached = db
        .read(move |q| cache::entries(q, &s, &f))
        .await
        .map_err(vault_error)?;
    let loaded = load_folder(
        files.store().as_ref(),
        files.access(),
        &folder.prefix,
        &folder.fid,
        folder.keys(),
        cached,
    )
    .await?;
    if !loaded.fetched.is_empty() || !loaded.vanished.is_empty() {
        let (fetched, vanished) = (loaded.fetched.clone(), loaded.vanished.clone());
        db.write(move |tx| {
            cache::put_entries(tx, &storage, &folder_id, &fetched)?;
            cache::drop_entries(tx, &storage, &folder_id, &vanished)
        })
        .await
        .map_err(vault_error)?;
    }
    let leftovers = cleanup::leftovers(&loaded, cleanup::now_ms());
    if !leftovers.is_empty() {
        let (store, access) = (Arc::clone(files.store()), files.access().clone());
        let prefix = folder.prefix.clone();
        (scope.background)(Box::pin(async move {
            cleanup::clean(store.as_ref(), &access, &prefix, &leftovers).await;
        }));
    }
    Ok(loaded)
}

#[cfg(test)]
#[path = "route_tests.rs"]
mod tests;
