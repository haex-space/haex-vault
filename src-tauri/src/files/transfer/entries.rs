//! The single entries a transfer on this device works with (spec 044 FR-018 to FR-024): their
//! names, removing one (into the trash on desktops, FR-023), links as links, and a free name for
//! "keep both".

use std::ffi::OsStr;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use super::local::Removal;
use crate::files::local::ops::io_error;
use crate::files::{FilesError, FilesErrorCode};

/// The last part of `path`.
pub(super) fn entry_name(path: &Path) -> Result<&OsStr, FilesError> {
    path.file_name()
        .ok_or_else(|| FilesError::invalid_path("path must name an entry"))
}

/// Removes an entry (a link itself, never its target).
pub(crate) fn remove(path: &Path, how: Removal) -> Result<(), FilesError> {
    let Ok(meta) = std::fs::symlink_metadata(path) else {
        return Ok(());
    };
    if how == Removal::Trash {
        return trash(path);
    }
    let removed = if meta.is_dir() {
        std::fs::remove_dir_all(path)
    } else {
        std::fs::remove_file(path)
    };
    removed.map_err(|error| io_error(error, path))
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
fn trash(path: &Path) -> Result<(), FilesError> {
    trash::delete(path).map_err(|error| {
        log::warn!(
            "files: moving {} to the trash failed: {error}",
            path.display()
        );
        FilesError::new(
            FilesErrorCode::NoAccess,
            "the system's trash refused the entry",
        )
    })
}

#[cfg(any(target_os = "android", target_os = "ios"))]
fn trash(_path: &Path) -> Result<(), FilesError> {
    Err(FilesError::new(
        FilesErrorCode::Unsupported,
        "no trash on this platform",
    ))
}

#[cfg(unix)]
pub(super) fn copy_link(source: &Path, target: &Path) -> Result<(), FilesError> {
    let points = std::fs::read_link(source).map_err(|error| io_error(error, source))?;
    std::os::unix::fs::symlink(points, target).map_err(|error| write_error(error, target))
}

/// Windows needs other rights for links; the target of a file link is copied as a file.
#[cfg(not(unix))]
pub(super) fn copy_link(source: &Path, target: &Path) -> Result<(), FilesError> {
    if source.is_file() {
        std::fs::copy(source, target)
            .map(|_| ())
            .map_err(|error| write_error(error, target))
    } else {
        Ok(())
    }
}

/// An I/O failure while writing, with a full disk named as such.
pub(super) fn write_error(error: std::io::Error, path: &Path) -> FilesError {
    if error.kind() == ErrorKind::StorageFull {
        return FilesError::new(FilesErrorCode::NoSpace, "the target is full");
    }
    io_error(error, path)
}

/// The first name `<stem> (n)<ext>` that is free in `folder`, counting from 2.
pub fn free_name(folder: &Path, name: &str) -> PathBuf {
    let (stem, ext) = match name.rfind('.') {
        Some(dot) if dot > 0 => name.split_at(dot),
        _ => (name, ""),
    };
    (2..)
        .map(|n| folder.join(format!("{stem} ({n}){ext}")))
        .find(|path| std::fs::symlink_metadata(path).is_err())
        .expect("some number is free")
}

#[cfg(test)]
#[path = "entries_tests.rs"]
mod tests;
