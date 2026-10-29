//! What a transfer across sources moves (spec 044 US5): the files and empty folders below each
//! source, with their place below the target, and the refusals before the start (a folder into
//! itself within one bucket, too little space for a download; FR-022).

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use tokio::runtime::Handle;
use tokio::time::Instant;
use walkdir::WalkDir;

use super::local::Totals;
use super::remote::CALL;
use super::remote_plan_encrypted::{
    encrypted_items, encrypted_prefix_of, nested_items, raw_items, relink_items,
};
use super::TransferOp;
use crate::files::encrypted::format::is_folder_prefix;
use crate::files::encrypted::route::{open, route, view, Located, Route};
use crate::files::encrypted::tree::Version;
use crate::files::encrypted::OpenFolder;
use crate::files::local::edit::check_name;
use crate::files::local::ops::io_error;
use crate::files::storage_source::{folder_prefix, object_key, storage_error, StorageFiles};
use crate::files::{FilesError, FilesErrorCode};

/// The most objects under a folder of a storage that one transfer takes.
const MAX_OBJECTS: usize = 100_000;

/// One end of a transfer.
#[derive(Clone)]
pub enum Side {
    /// This device; paths are resolved and checked.
    Device,
    Storage(Arc<StorageFiles>),
}

/// A transfer across sources.
#[derive(Clone)]
pub struct RemoteJob {
    pub op: TransferOp,
    pub from: Side,
    /// Device paths, or the window paths of a storage.
    pub sources: Vec<String>,
    pub to: Side,
    /// The folder they go into: a device path or a storage's window path.
    pub target: String,
    /// The caller knows the names at the target are free (a sync run just listed it), so a file
    /// sent to a storage is not looked for there first. A transfer the user started asks.
    pub known_free: bool,
}

/// What goes where.
#[derive(Debug, Clone)]
pub(super) struct Item {
    /// A device path or a key.
    pub(super) from: String,
    /// The parts of its place below the target folder.
    pub(super) rel: Vec<String>,
    pub(super) size: u64,
    /// An empty folder (device), a folder marker (storage) or a folder entry (encrypted folder):
    /// made at the target, never asked.
    pub(super) folder: bool,
    /// A file or folder of an encrypted folder (spec 048); `from` is then empty.
    pub(super) encrypted: Option<EncSource>,
}

/// What a node of an encrypted folder is, as a transfer's source.
#[derive(Clone)]
pub(super) enum EncNode {
    /// The encrypted folder itself.
    Root,
    /// The virtual folder of entries whose parent is gone.
    Recovered,
    Folder(Version),
    File(Version),
}

/// An entry of an encrypted folder as a transfer's source.
#[derive(Clone)]
pub(super) struct EncSource {
    pub(super) folder: Arc<OpenFolder>,
    pub(super) node: EncNode,
    /// Moves inside its own encrypted folder: written anew under its new parent (FR-018).
    pub(super) relink: bool,
}

impl EncSource {
    /// The version of a file or folder entry.
    pub(super) fn version(&self) -> Option<&Version> {
        match &self.node {
            EncNode::File(version) | EncNode::Folder(version) => Some(version),
            EncNode::Root | EncNode::Recovered => None,
        }
    }

    /// The version of a file.
    pub(super) fn file(&self) -> Option<&Version> {
        match &self.node {
            EncNode::File(version) => Some(version),
            _ => None,
        }
    }
}

impl std::fmt::Debug for EncSource {
    // Never a name of the folder (FR-027).
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("EncSource")
    }
}

/// An encrypted folder, or a folder in one, as a transfer's target (spec 048).
pub(super) struct EncTarget {
    pub(super) files: Arc<StorageFiles>,
    pub(super) folder: Arc<OpenFolder>,
    /// The target folder's names below the encrypted folder.
    pub(super) rest: Vec<String>,
}

/// The items of a job and its totals.
pub struct Plan {
    pub(super) items: Vec<Item>,
    /// The device folders a move empties at the end.
    pub(super) folders: Vec<PathBuf>,
    pub totals: Totals,
    /// Set when the target is in an encrypted folder.
    pub(super) target: Option<EncTarget>,
    /// The files that leave an encrypted folder for an ordinary folder of a storage; the window
    /// is asked once before the start (FR-019), then this is 0.
    pub(super) leaving: AtomicU64,
}

impl Plan {
    /// How many files leave an encrypted folder for an ordinary one and still need the window's
    /// yes (FR-019).
    pub fn leaving(&self) -> u64 {
        self.leaving.load(Ordering::SeqCst)
    }

    /// The window said yes; a retry does not ask again.
    pub fn confirm_leaving(&self) {
        self.leaving.store(0, Ordering::SeqCst);
    }
}

pub(super) fn soon() -> Instant {
    Instant::now() + CALL
}

/// Where a transfer's items go.
enum To<'a> {
    Device,
    Plain(&'a StorageFiles),
    Encrypted(&'a EncTarget),
}

/// Lists what a job moves, refuses a folder into itself within one bucket and too little space
/// for a download (FR-022).
pub fn prepare_remote(
    job: &RemoteJob,
    handle: &Handle,
    free_space: impl Fn(&Path) -> Option<u64>,
) -> Result<Plan, FilesError> {
    let target = match &job.to {
        Side::Storage(files) => match handle.block_on(route(files, &job.target))? {
            Route::Encrypted(at) => Some(handle.block_on(encrypted_target(files, at))?),
            Route::Plain => None,
        },
        Side::Device => None,
    };
    let to = match (&job.to, &target) {
        (_, Some(target)) => To::Encrypted(target),
        (Side::Storage(files), None) => To::Plain(files),
        (Side::Device, None) => To::Device,
    };
    let mut items = Vec::new();
    let mut folders = Vec::new();
    for source in &job.sources {
        match &job.from {
            Side::Device => device_items(Path::new(source), &mut items, &mut folders)?,
            Side::Storage(files) => {
                if let Side::Storage(target) = &job.to {
                    let same = files.access().location == target.access().location;
                    let source_prefix = folder_prefix(source)?;
                    if same && folder_prefix(&job.target)?.starts_with(&source_prefix) {
                        return Err(FilesError::new(
                            FilesErrorCode::IntoItself,
                            "a folder cannot go into itself",
                        ));
                    }
                }
                let decrypts = !matches!(to, To::Plain(_));
                match handle.block_on(route(files, source))? {
                    Route::Plain => {
                        handle.block_on(storage_items(files, source, decrypts, &mut items))?;
                    }
                    Route::Encrypted(at) => {
                        handle.block_on(encrypted_source(files, &at, job, &to, &mut items))?
                    }
                }
            }
        }
    }
    // A device name of an encrypted folder's form would show as a damaged one (spec 048).
    let uploads = matches!((&job.from, &job.to), (Side::Device, Side::Storage(_)));
    if uploads
        && items
            .iter()
            .any(|item| item.rel.iter().any(|part| is_folder_prefix(part)))
    {
        return Err(FilesError::new(
            FilesErrorCode::InvalidName,
            "this name is kept for encrypted folders",
        ));
    }
    if target.is_some() {
        for part in items.iter().flat_map(|item| &item.rel) {
            check_name(part)?;
        }
    }
    // Out of an encrypted folder into an ordinary one of a storage asks first (FR-019).
    let leaving = match to {
        To::Plain(_) => items
            .iter()
            .filter(|item| !item.folder && item.encrypted.is_some())
            .count() as u64,
        _ => 0,
    };
    let totals = Totals {
        items: items.iter().filter(|item| !item.folder).count() as u64,
        bytes: items.iter().map(|item| item.size).sum(),
    };
    if matches!(job.to, Side::Device) {
        if let Some(free) = free_space(Path::new(&job.target)) {
            if totals.bytes > free {
                return Err(FilesError::new(
                    FilesErrorCode::NoSpace,
                    "not enough free space at the target",
                ));
            }
        }
    }
    Ok(Plan {
        items,
        folders,
        totals,
        target,
        leaving: AtomicU64::new(leaving),
    })
}

/// The items of the encrypted folder, or entry in one, at `at`: written anew inside its own
/// folder for a move there, its objects as they are for the folder itself into an ordinary folder
/// of a storage, else decrypted.
async fn encrypted_source(
    files: &StorageFiles,
    at: &Located,
    job: &RemoteJob,
    to: &To<'_>,
    items: &mut Vec<Item>,
) -> Result<(), FilesError> {
    match to {
        To::Plain(target) if at.rest.is_empty() => {
            raw_items(files, at, target, &job.target, items).await
        }
        To::Encrypted(target)
            if job.op == TransferOp::Move
                && !at.rest.is_empty()
                && target.folder.prefix == at.head.prefix
                && target.files.access().location == files.access().location =>
        {
            relink_items(files, at, target, items).await
        }
        _ => encrypted_items(files, at, &[], items).await,
    }
}

fn device_items(
    source: &Path,
    items: &mut Vec<Item>,
    folders: &mut Vec<PathBuf>,
) -> Result<(), FilesError> {
    let base = source.parent().unwrap_or(source);
    let meta = std::fs::symlink_metadata(source).map_err(|error| io_error(error, source))?;
    if meta.is_dir() {
        folders.push(source.to_path_buf());
    }
    for entry in WalkDir::new(source).follow_links(false).sort_by_file_name() {
        let entry = entry.map_err(|_| FilesError::new(FilesErrorCode::NoAccess, "not readable"))?;
        let rel: Vec<String> = entry
            .path()
            .strip_prefix(base)
            .unwrap_or(entry.path())
            .components()
            .map(|part| part.as_os_str().to_string_lossy().into_owned())
            .collect();
        let file_type = entry.file_type();
        if file_type.is_dir() {
            let empty = std::fs::read_dir(entry.path()).is_ok_and(|mut read| read.next().is_none());
            if empty {
                items.push(Item {
                    from: entry.path().to_string_lossy().into_owned(),
                    rel,
                    size: 0,
                    folder: true,
                    encrypted: None,
                });
            }
        } else if file_type.is_file() {
            let size = entry.metadata().map(|meta| meta.len()).unwrap_or(0);
            items.push(Item {
                from: entry.path().to_string_lossy().into_owned(),
                rel,
                size,
                folder: false,
                encrypted: None,
            });
        }
        // Links stay behind: a storage has none.
    }
    Ok(())
}

/// The objects of the file or folder `source`. With `decrypts` (the target is the device or an
/// encrypted folder) an encrypted folder inside comes decrypted instead of as its objects.
async fn storage_items(
    files: &StorageFiles,
    source: &str,
    decrypts: bool,
    items: &mut Vec<Item>,
) -> Result<(), FilesError> {
    let entry = files.stat(source).await?;
    let name = entry.name.clone();
    if entry.kind == crate::files::EntryKind::File {
        items.push(Item {
            from: object_key(source)?,
            rel: vec![name],
            size: entry.size.unwrap_or(0),
            folder: false,
            encrypted: None,
        });
        return Ok(());
    }
    let prefix = folder_prefix(source)?;
    let mut objects = files
        .store()
        .list(files.access(), &prefix, MAX_OBJECTS, soon())
        .await
        .map_err(storage_error)?;
    // The head of an encrypted folder inside after its other objects (format: the head last).
    objects.sort_by_key(|object| object.key.ends_with(".hxef/h"));
    let mut nested: Vec<String> = Vec::new();
    for object in objects {
        if decrypts {
            if let Some(inner) = encrypted_prefix_of(&prefix, &object.key) {
                if !nested.contains(&inner) {
                    nested.push(inner);
                }
                continue;
            }
        }
        let mut rel = vec![name.clone()];
        let rest = object.key[prefix.len()..].trim_end_matches('/');
        if !rest.is_empty() {
            rel.extend(rest.split('/').map(str::to_owned));
        }
        items.push(Item {
            folder: object.key.ends_with('/'),
            from: object.key,
            rel,
            size: object.size,
            encrypted: None,
        });
    }
    nested_items(files, &prefix, &name, &nested, items).await
}

/// The encrypted folder at `at` as a target: it opens and is a folder.
async fn encrypted_target(files: &Arc<StorageFiles>, at: Located) -> Result<EncTarget, FilesError> {
    let folder = open(files, &at).await?;
    let view = view(files, &folder).await?;
    match view.tree.find(&at.rest) {
        Some(index) if view.tree.parent_id(index).is_some() => {}
        Some(_) => return Err(FilesError::invalid_path("not a folder")),
        None => return Err(FilesError::new(FilesErrorCode::NotFound, "not found")),
    }
    drop(view);
    Ok(EncTarget {
        files: Arc::clone(files),
        folder,
        rest: at.rest,
    })
}

/// Removes `folder` and the folders below it that are empty now.
pub(super) fn remove_empty(folder: &Path) {
    if let Ok(read) = std::fs::read_dir(folder) {
        for child in read.flatten() {
            if child.file_type().is_ok_and(|kind| kind.is_dir()) {
                remove_empty(&child.path());
            }
        }
    }
    let _ = std::fs::remove_dir(folder);
}

/// Reads from `read` until `limit` bytes or the end.
pub(super) fn fill(
    read: &mut dyn FnMut(&mut [u8]) -> std::io::Result<usize>,
    limit: usize,
) -> std::io::Result<Vec<u8>> {
    let mut body = vec![0u8; limit];
    let mut filled = 0;
    while filled < limit {
        let got = read(&mut body[filled..])?;
        if got == 0 {
            break;
        }
        filled += got;
    }
    body.truncate(filled);
    Ok(body)
}
