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
    /// An empty folder (device) or a folder marker (storage): made at the target, never asked.
    pub(super) folder: bool,
}

/// The items of a job and its totals.
pub struct Plan {
    pub(super) items: Vec<Item>,
    /// The device folders a move empties at the end.
    pub(super) folders: Vec<PathBuf>,
    pub totals: Totals,
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
                handle.block_on(storage_items(files, source, &mut items))?;
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
                });
            }
        } else if file_type.is_file() {
            let size = entry.metadata().map(|meta| meta.len()).unwrap_or(0);
            items.push(Item {
                from: entry.path().to_string_lossy().into_owned(),
                rel,
                size,
                folder: false,
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
        });
    }
    Ok(())
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
