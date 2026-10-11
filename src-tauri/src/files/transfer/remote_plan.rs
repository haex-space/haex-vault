//! What a transfer across sources moves (spec 044 US5): the files and empty folders below each
//! source, with their place below the target, and the refusals before the start (a folder into
//! itself within one bucket, too little space for a download; FR-022).

use std::path::{Path, PathBuf};
use std::sync::Arc;

use tokio::runtime::Handle;
use tokio::time::Instant;
use walkdir::WalkDir;

use super::local::Totals;
use super::remote::CALL;
use super::TransferOp;
use crate::files::encrypted::format::is_folder_prefix;
use crate::files::encrypted::ops::later;
use crate::files::encrypted::route::{open, route, view, Located, Route};
use crate::files::encrypted::tree::{facts, Item as TreeItem, Version};
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
    /// A file or folder of an encrypted folder (spec 048); `from` is then its window path.
    pub(super) encrypted: Option<EncSource>,
}

/// An entry of an encrypted folder as a transfer's source.
#[derive(Clone)]
pub(super) struct EncSource {
    pub(super) folder: Arc<OpenFolder>,
    /// The file's version; `None` for a folder.
    pub(super) version: Option<Version>,
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
}

fn soon() -> Instant {
    Instant::now() + CALL
}

/// Lists what a job moves, refuses a folder into itself within one bucket and too little space
/// for a download (FR-022).
pub fn prepare_remote(
    job: &RemoteJob,
    handle: &Handle,
    free_space: impl Fn(&Path) -> Option<u64>,
) -> Result<Plan, FilesError> {
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
                match handle.block_on(route(files, source))? {
                    Route::Plain => handle.block_on(storage_items(files, source, &mut items))?,
                    Route::Encrypted(_) if job.op == TransferOp::Move => return Err(later()),
                    Route::Encrypted(at) => {
                        handle.block_on(encrypted_items(files, &at, &mut items))?;
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
    let target = match &job.to {
        Side::Storage(files) => match handle.block_on(route(files, &job.target))? {
            Route::Encrypted(at) => Some(handle.block_on(encrypted_target(files, at))?),
            Route::Plain => None,
        },
        Side::Device => None,
    };
    if target.is_some() {
        for part in items.iter().flat_map(|item| &item.rel) {
            check_name(part)?;
        }
    } else if matches!(job.to, Side::Storage(_)) && items.iter().any(|i| i.encrypted.is_some()) {
        // Out of an encrypted folder into an ordinary one asks first (FR-019, spec 048 US5).
        return Err(later());
    }
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
    })
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

async fn storage_items(
    files: &StorageFiles,
    source: &str,
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
    let objects = files
        .store()
        .list(files.access(), &prefix, MAX_OBJECTS, soon())
        .await
        .map_err(storage_error)?;
    for object in objects {
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
    Ok(())
}

/// The files and folders of the encrypted folder (or entry in one) at `at`; a damaged file refuses
/// the transfer before it starts (FR-022).
async fn encrypted_items(
    files: &StorageFiles,
    at: &Located,
    items: &mut Vec<Item>,
) -> Result<(), FilesError> {
    let folder = open(files, at).await?;
    let view = view(files, &folder).await?;
    let top = view
        .tree
        .find(&at.rest)
        .ok_or_else(|| FilesError::new(FilesErrorCode::NotFound, "not found"))?;
    let name = at.rest.last().unwrap_or(&at.head.display).clone();
    let below = view.tree.below(top).into_iter().map(|(index, mut rel)| {
        rel.insert(0, name.clone());
        (index, rel)
    });
    for (index, rel) in std::iter::once((top, vec![name.clone()])).chain(below) {
        let (version, damaged) = match &view.tree.node(index).item {
            TreeItem::Version { version, damaged } => (Some((**version).clone()), *damaged),
            TreeItem::Broken { .. } => (None, true),
            TreeItem::Root | TreeItem::Recovered => (None, false),
        };
        if damaged {
            return Err(crate::files::encrypted::damaged());
        }
        let file = version.as_ref().and_then(facts).map(|facts| facts.size);
        items.push(Item {
            from: String::new(),
            rel,
            size: file.unwrap_or(0),
            folder: file.is_none(),
            encrypted: Some(EncSource {
                folder: Arc::clone(&folder),
                version: version.filter(|_| file.is_some()),
            }),
        });
    }
    Ok(())
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
