//! The building blocks of [`super::local::LocalStore`] (spec 048 research R15): a key as a path
//! under the root that neither `..` nor a link leaves, the entries of a folder as WebDAV and SFTP
//! list them, the marks of a file, and errors without a path (FR-027).

use std::fs::{self, Metadata};
use std::io::{self, Write};
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use super::tree::{is_part, part_name, DirEntry};
use super::StorageError;
use crate::passwords::clock;

/// The path of `key` under `root` (an absolute folder). A key with an empty, `.` or `..` part, a
/// leading `/`, a `\`, a NUL or (Windows) a `:` is refused, and so is a path whose existing part
/// leads out of `root` through a link. A key ending in `/` names the folder.
pub(crate) fn resolve(root: &Path, key: &str) -> Result<PathBuf, StorageError> {
    if !root.is_absolute() {
        return Err(StorageError::AccessDenied);
    }
    let trimmed = key.strip_suffix('/').unwrap_or(key);
    let parts: Vec<&str> = if trimmed.is_empty() {
        Vec::new()
    } else {
        trimmed.split('/').collect()
    };
    let bad = |part: &&str| {
        part.is_empty()
            || *part == "."
            || *part == ".."
            || part.contains(['\\', '\0'])
            || (cfg!(windows) && part.contains(':'))
    };
    if key.starts_with('/') || parts.iter().any(bad) {
        return Err(StorageError::AccessDenied);
    }
    let base = fs::canonicalize(root).map_err(error_of)?;
    let mut path = base.clone();
    let mut existing = true;
    for part in parts {
        path.push(part);
        if !existing {
            continue;
        }
        match fs::symlink_metadata(&path) {
            Ok(meta) if meta.file_type().is_symlink() => {
                path = fs::canonicalize(&path).map_err(|_| StorageError::AccessDenied)?;
                if !path.starts_with(&base) {
                    return Err(StorageError::AccessDenied);
                }
            }
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => existing = false,
            Err(error) => return Err(error_of(error)),
        }
    }
    // Only plain names were added, so this holds unless a link said otherwise above.
    debug_assert!(path.components().all(|c| c != Component::ParentDir));
    Ok(path)
}

/// The path of `key` for a call that changes something: never the root itself.
pub(super) fn resolve_below(root: &Path, key: &str) -> Result<PathBuf, StorageError> {
    let path = resolve(root, key)?;
    match fs::canonicalize(root) {
        Ok(base) if base == path => Err(StorageError::AccessDenied),
        _ => Ok(path),
    }
}

/// A local error as a [`StorageError`]; only its kind is logged, never the path.
pub(crate) fn error_of(error: io::Error) -> StorageError {
    match error.kind() {
        io::ErrorKind::NotFound | io::ErrorKind::NotADirectory => StorageError::NotFound,
        io::ErrorKind::PermissionDenied | io::ErrorKind::ReadOnlyFilesystem => {
            StorageError::AccessDenied
        }
        io::ErrorKind::StorageFull | io::ErrorKind::QuotaExceeded => StorageError::QuotaExceeded,
        kind => {
            log::warn!("local storage: {kind}");
            StorageError::Network
        }
    }
}

fn millis(time: io::Result<SystemTime>) -> i64 {
    time.map(clock::unix_millis).unwrap_or(0)
}

/// The time of `meta` as S3 lists it (`2026-10-13T09:41:45.123Z`).
pub(super) fn iso_of(meta: &Metadata) -> String {
    clock::format_millis(millis(meta.modified()))
}

/// The time of `meta` as an HTTP date, as a `HEAD` answers.
pub(super) fn http_date_of(meta: &Metadata) -> Option<String> {
    meta.modified().ok().map(httpdate::fmt_http_date)
}

/// A mark that changes with the content: size, time to the nanosecond and (Unix) the inode, which
/// every write through a temporary file renews. Windows has no stable file index, so there it is
/// size and time.
pub(crate) fn etag_of(meta: &Metadata) -> String {
    let nanos = meta
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |since| since.as_nanos());
    #[cfg(unix)]
    let inode = std::os::unix::fs::MetadataExt::ino(meta);
    #[cfg(not(unix))]
    let inode = 0u64;
    format!("\"{:x}-{nanos:x}-{inode:x}\"", meta.len())
}

/// The entries of the folder `dir`, without holzi's temporary files and without links (a link
/// could lead out of the root; holzi never writes one into an encrypted folder).
pub(super) fn read_entries(dir: &Path) -> Result<Vec<DirEntry>, StorageError> {
    let mut entries = Vec::new();
    for entry in fs::read_dir(dir).map_err(error_of)? {
        let entry = entry.map_err(error_of)?;
        let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        let meta = match entry.path().symlink_metadata() {
            Ok(meta) => meta,
            Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error_of(error)),
        };
        if meta.file_type().is_symlink() || is_part(&name) {
            continue;
        }
        entries.push(DirEntry {
            name,
            is_dir: meta.is_dir(),
            size: if meta.is_dir() { 0 } else { meta.len() },
            modified: iso_of(&meta),
            etag: etag_of(&meta),
        });
    }
    Ok(entries)
}

/// A new temporary file next to `path`.
pub(super) fn part_of(path: &Path) -> PathBuf {
    path.with_file_name(part_name())
}

/// Creates the folder of `path` with every missing parent.
pub(super) fn make_parent(path: &Path) -> Result<(), StorageError> {
    match path.parent() {
        Some(parent) => fs::create_dir_all(parent).map_err(error_of),
        None => Ok(()),
    }
}

/// Moves the finished temporary file `part` onto `path`; on failure the temporary file goes.
pub(super) fn settle(part: &Path, path: &Path) -> Result<(), StorageError> {
    let moved = fs::File::open(part)
        .and_then(|file| file.sync_all())
        .and_then(|()| fs::rename(part, path));
    if let Err(error) = moved {
        let _ = fs::remove_file(part);
        return Err(error_of(error));
    }
    Ok(())
}

/// Writes `body` to `path` through a temporary file (FR-047).
pub(super) fn write_whole(path: &Path, body: &[u8]) -> Result<(), StorageError> {
    make_parent(path)?;
    let part = part_of(path);
    if let Err(error) = fs::write(&part, body) {
        let _ = fs::remove_file(&part);
        return Err(error_of(error));
    }
    settle(&part, path)
}

/// Appends `body` to the temporary file of an upload.
pub(super) fn append(part: &Path, body: &[u8]) -> Result<(), StorageError> {
    fs::OpenOptions::new()
        .append(true)
        .open(part)
        .and_then(|mut file| file.write_all(body))
        .map_err(error_of)
}
