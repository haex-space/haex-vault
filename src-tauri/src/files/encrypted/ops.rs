//! What the window does in and with encrypted folders (spec 048 US1): create one (one head, nothing
//! else), list, stat, read the start of a file, and create a sub folder (one sidecar of kind
//! `folder`, no marker object); renaming and deleting are in [`super::manage`]. A damaged entry
//! answers `damaged` without data.

use std::sync::Arc;
use std::time::{Duration, SystemTime};

use tokio::time::Instant;

use super::format::head::seal_head;
use super::format::sidecar::{Sidecar, SidecarKind};
use super::format::{encode_id, FOLDER_SUFFIX};
use super::route::{
    self, in_encrypted, located_entry, open, route, vault_error, view, Located, Route,
};
use super::source::EncryptedFile;
use super::tree::{facts, Item, Version};
use super::vault_keys::{current, no_key};
use super::{damaged, new_id, state_error, FolderHead, OpenFolder};
use crate::files::local::edit::check_name;
use crate::files::storage_source::{folder_prefix, storage_error, StorageFiles};
use crate::files::{EncryptedState, Entry, FilesError, FilesErrorCode};

const CALL: Duration = Duration::from_secs(30);

fn soon() -> Instant {
    Instant::now() + CALL
}

fn not_found() -> FilesError {
    FilesError::new(FilesErrorCode::NotFound, "not found")
}

/// Milliseconds since 1970, as sidecars count.
pub fn now_ms() -> u64 {
    u64::try_from(crate::passwords::clock::unix_millis(SystemTime::now())).unwrap_or(0)
}

/// The entries of the encrypted folder or sub folder at `at`.
pub async fn list(files: &StorageFiles, at: &Located) -> Result<Vec<Entry>, FilesError> {
    let folder = open(files, at).await?;
    let view = view(files, &folder).await?;
    let index = view.tree.find(&at.rest).ok_or_else(not_found)?;
    if !view.tree.is_dir(index) {
        return Err(FilesError::invalid_path("not a folder"));
    }
    Ok(view
        .tree
        .children(index)
        .iter()
        .map(|&child| {
            let mut rel = at.rest.clone();
            rel.push(view.tree.node(child).name.clone());
            view.tree.entry(child, at.path_of(&rel))
        })
        .collect())
}

/// The entry at `at`: the encrypted folder itself, or an entry inside.
pub async fn stat(files: &StorageFiles, at: &Located) -> Result<Entry, FilesError> {
    if at.rest.is_empty() {
        let mut entry = located_entry(at);
        // The window asks for the folder itself when it enters it: its sidecars are read again (by
        // ETag), so what others wrote shows, with the count of foreign files. A folder that does
        // not open shows none.
        if let (Ok(folder), Some(info)) = (open(files, at).await, entry.encrypted.as_mut()) {
            if let Ok(loaded) = route::load_view(files, &folder).await {
                info.foreign = loaded.foreign;
                folder.reload(loaded).await;
            }
        }
        return Ok(entry);
    }
    let folder = open(files, at).await?;
    let view = view(files, &folder).await?;
    let index = view.tree.find(&at.rest).ok_or_else(not_found)?;
    Ok(view.tree.entry(index, at.path_of(&at.rest)))
}

/// The file version at `at`, or why there is none (`damaged` for a damaged entry).
pub async fn file_at(
    files: &StorageFiles,
    at: &Located,
) -> Result<(Arc<OpenFolder>, Version), FilesError> {
    let folder = open(files, at).await?;
    let version = {
        let view = view(files, &folder).await?;
        let index = view.tree.find(&at.rest).ok_or_else(not_found)?;
        match &view.tree.node(index).item {
            Item::Version { damaged: true, .. } | Item::Broken { .. } => return Err(damaged()),
            Item::Version { version, .. } if facts(version).is_some() => (**version).clone(),
            _ => return Err(FilesError::invalid_path("not a file")),
        }
    };
    Ok((folder, version))
}

/// The first `limit + 1` bytes of the file at `at` (the text viewer, through the block reader).
pub async fn read_start(
    files: &StorageFiles,
    at: &Located,
    limit: u64,
) -> Result<Vec<u8>, FilesError> {
    let (folder, version) = file_at(files, at).await?;
    let file = EncryptedFile::open(files, folder, &version).await?;
    let end = file.size().min(limit.saturating_add(1));
    file.read(0, end).await
}

/// Creates the sub folder `name` in the folder at `at`: one sidecar of kind `folder`.
pub async fn create_folder(
    files: &StorageFiles,
    at: &Located,
    name: &str,
) -> Result<Entry, FilesError> {
    check_name(name)?;
    let folder = open(files, at).await?;
    let parent = {
        let view = view(files, &folder).await?;
        let index = view.tree.find(&at.rest).ok_or_else(not_found)?;
        let parent = view
            .tree
            .parent_id(index)
            .ok_or_else(|| FilesError::invalid_path("nothing can be created here"))?;
        if child_named(&view.tree, index, name) {
            return Err(FilesError::new(FilesErrorCode::Exists, "the name is taken"));
        }
        parent
    };
    let version = Version {
        sid: new_id(),
        sidecar: Sidecar {
            entry: new_id(),
            parent,
            name: name.to_owned(),
            revision: 1,
            base: None,
            written: now_ms(),
            kind: SidecarKind::Folder,
        },
    };
    write_version(files, &folder, version, None).await?;
    let mut rel = at.rest.clone();
    rel.push(name.to_owned());
    stat(
        files,
        &Located {
            head: Arc::clone(&at.head),
            root: at.root.clone(),
            rest: rel,
        },
    )
    .await
}

/// Whether node `index` has a child named `name`.
pub fn child_named(tree: &super::tree::Tree, index: usize, name: &str) -> bool {
    tree.children(index)
        .iter()
        .any(|&child| tree.node(child).name == name)
}

/// Writes the sidecar of `version` (its content object is complete already, FR-016) and shows it.
pub async fn write_version(
    files: &StorageFiles,
    folder: &OpenFolder,
    version: Version,
    content: Option<(super::format::Id, u64)>,
) -> Result<(), FilesError> {
    let body = folder.seal(&version.sid, &version.sidecar)?;
    files
        .store()
        .put(
            files.access(),
            &folder.sidecar_key(&version.sid),
            body,
            soon(),
        )
        .await
        .map_err(storage_error)?;
    view(files, folder).await?.add(folder.fid, version, content);
    Ok(())
}

/// Creates the encrypted folder `name` in the ordinary folder `path` (FR-001, FR-002): one head with
/// one recipient, the current content key generation; nothing else in the bucket.
pub async fn create_encrypted_folder(
    files: &StorageFiles,
    path: &str,
    name: &str,
) -> Result<Entry, FilesError> {
    check_name(name)?;
    let scope = files.scope().ok_or_else(in_encrypted)?;
    if let Route::Encrypted(_) = route(files, path).await? {
        return Err(in_encrypted());
    }
    let parent = folder_prefix(path)?;
    if ["holzi-ext/", "holzi-ext-dev/"]
        .iter()
        .any(|area| parent.starts_with(area))
    {
        return Err(FilesError::new(
            FilesErrorCode::InExtensionArea,
            "not in the area of an extension",
        ));
    }
    // The listing names the encrypted folders beside the ordinary entries (spec: same name twice).
    if files
        .list(path)
        .await?
        .iter()
        .any(|entry| entry.name == name)
    {
        return Err(FilesError::new(FilesErrorCode::Exists, "the name is taken"));
    }
    let key = scope
        .db()?
        .read(|q| current(q))
        .await
        .map_err(vault_error)?
        .ok_or_else(no_key)?;
    let fid = new_id();
    let fk = crate::sync::keys::random_bytes::<32>();
    let head = seal_head(&fid, &fk, name, &key.key_id, &key.key).map_err(super::format_error)?;
    let prefix = format!("{parent}{}{FOLDER_SUFFIX}/", encode_id(&fid));
    files
        .store()
        .put(files.access(), &format!("{prefix}h"), head, soon())
        .await
        .map_err(storage_error)?;
    let folder = Arc::new(OpenFolder::new(prefix.clone(), fid, None, &fk));
    folder.reload(super::load::LoadedFolder::default()).await;
    scope.folders.keep(&scope.storage_id, folder);
    let head = FolderHead {
        prefix,
        fid: Some(fid),
        state: EncryptedState::Readable,
        display: name.to_owned(),
        conflict: false,
        modified_ms: crate::passwords::clock::unix_millis(SystemTime::now()).into(),
        etag: None,
    };
    let mut heads = scope
        .folders
        .heads(&scope.storage_id, &parent)
        .unwrap_or_default();
    heads.push(Arc::new(head.clone()));
    scope.folders.set_heads(&scope.storage_id, &parent, heads);
    Ok(route::folder_entry(&head, path))
}

/// Loads the sidecars of the encrypted folder at `path` again (another device wrote).
pub async fn refresh(files: &StorageFiles, path: &str) -> Result<(), FilesError> {
    let Route::Encrypted(at) = route(files, path).await? else {
        return Err(FilesError::invalid_path("not an encrypted folder"));
    };
    if at.head.state != EncryptedState::Readable {
        return Err(state_error(at.head.state));
    }
    let folder = open(files, &at).await?;
    let loaded = route::load_view(files, &folder).await?;
    folder.reload(loaded).await;
    Ok(())
}

#[cfg(test)]
#[path = "ops_tests.rs"]
mod tests;
