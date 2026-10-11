//! Listing a folder and describing one entry on this device (spec 044 FR-003, FR-037, research R3).
//! Paths come in resolved ([`crate::files::local::resolve`]); entries keep the path as listed, so a
//! link shows where it sits, and its kind is that of its target (a broken link is a file).

use std::fs::Metadata;
use std::path::Path;
use std::time::UNIX_EPOCH;

use crate::files::kind::mime_for;
use crate::files::local::OwnPlaces;
use crate::files::{Entry, EntryKind, FilesError, FilesErrorCode};

/// Every entry of the folder `dir`, unsorted (the window sorts).
pub fn list(dir: &Path, own: &OwnPlaces) -> Result<Vec<Entry>, FilesError> {
    let read = std::fs::read_dir(dir).map_err(|error| io_error(error, dir))?;
    Ok(read
        .filter_map(Result::ok)
        .filter_map(|item| describe(&item.path(), own))
        .collect())
}

/// The entry at `path`.
pub fn stat(path: &Path, own: &OwnPlaces) -> Result<Entry, FilesError> {
    std::fs::symlink_metadata(path).map_err(|error| io_error(error, path))?;
    describe(path, own).ok_or_else(|| FilesError::new(FilesErrorCode::NotFound, "entry not found"))
}

fn describe(path: &Path, own: &OwnPlaces) -> Option<Entry> {
    let name = path.file_name()?.to_string_lossy().into_owned();
    let link = std::fs::symlink_metadata(path).ok()?;
    let symlink = link.file_type().is_symlink();
    // A link has the kind of its target; a broken one stays a file.
    let meta = if symlink {
        std::fs::metadata(path).unwrap_or(link)
    } else {
        link
    };
    let kind = if meta.is_dir() {
        EntryKind::Dir
    } else {
        EntryKind::File
    };
    Some(Entry {
        hidden: is_hidden(&name, &meta),
        size: (kind == EntryKind::File).then_some(meta.len()),
        modified_ms: modified_ms(&meta),
        mime: (kind == EntryKind::File)
            .then(|| mime_for(&name).map(str::to_owned))
            .flatten(),
        no_access: kind == EntryKind::Dir && !can_enter(path),
        holzi_owned: own.contains(path),
        encrypted: None,
        conflict: false,
        path: path.to_string_lossy().into_owned(),
        name,
        kind,
        symlink,
    })
}

fn modified_ms(meta: &Metadata) -> Option<i64> {
    let since = meta.modified().ok()?.duration_since(UNIX_EPOCH).ok()?;
    i64::try_from(since.as_millis()).ok()
}

pub(crate) fn is_hidden(name: &str, _meta: &Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_HIDDEN: u32 = 0x2;
        if _meta.file_attributes() & FILE_ATTRIBUTE_HIDDEN != 0 {
            return true;
        }
    }
    name.starts_with('.')
}

/// Whether this process may list and enter the folder `path`.
fn can_enter(path: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        let Ok(c_path) = std::ffi::CString::new(path.as_os_str().as_bytes()) else {
            return false;
        };
        // SAFETY: `c_path` is a valid, NUL-terminated string that outlives the call.
        unsafe { libc::access(c_path.as_ptr(), libc::R_OK | libc::X_OK) == 0 }
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        true
    }
}

/// The error for an I/O failure on `path`.
pub fn io_error(error: std::io::Error, path: &Path) -> FilesError {
    use std::io::ErrorKind;
    match error.kind() {
        ErrorKind::NotFound => FilesError::new(FilesErrorCode::NotFound, "not found"),
        ErrorKind::PermissionDenied => {
            FilesError::new(FilesErrorCode::NoAccess, "the system denies access")
        }
        ErrorKind::NotADirectory => FilesError::invalid_path("not a folder"),
        _ => {
            // The path can be a download out of an encrypted folder (spec 048 FR-027).
            log::warn!(
                "files: {} failed: {}",
                crate::files::encrypted::redacted::Redacted(&path.to_string_lossy()),
                error.kind()
            );
            FilesError::new(FilesErrorCode::NotFound, "not readable")
        }
    }
}

#[cfg(test)]
#[path = "ops_tests.rs"]
mod tests;
