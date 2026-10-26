//! Creating folders and renaming entries on this device (spec 044 FR-017, FR-037). Paths come in
//! resolved: a folder through [`crate::files::local::resolve`], an entry through
//! [`crate::files::local::resolve_entry`], so a link is renamed itself, not its target. Nothing in
//! holzi's own places changes, whoever asks.

use std::path::Path;

use crate::files::local::ops::{io_error, stat};
use crate::files::local::OwnPlaces;
use crate::files::{Entry, FilesError, FilesErrorCode};

/// The longest name most file systems take, in bytes.
const MAX_NAME: usize = 255;

/// Characters Windows refuses in a name; refused everywhere, as a folder made here may later be
/// copied to a Windows drive or a storage.
pub(crate) const FORBIDDEN: &[char] = &['/', '\\', ':', '*', '?', '"', '<', '>', '|', '\0'];

/// Whether `name` may name an entry.
pub fn check_name(name: &str) -> Result<(), FilesError> {
    let invalid = |message: &str| Err(FilesError::new(FilesErrorCode::InvalidName, message));
    if name.trim().is_empty() || name == "." || name == ".." {
        return invalid("the name is empty");
    }
    if name.len() > MAX_NAME {
        return invalid("the name is too long");
    }
    if name.contains(FORBIDDEN) || name.chars().any(char::is_control) {
        return invalid("the name holds a character that is not allowed");
    }
    if name.ends_with('.') || name.ends_with(' ') {
        return invalid("the name must not end with a dot or a space");
    }
    Ok(())
}

/// Refuses `path` when it is in holzi's own places or holds one (a folder that holds holzi's data
/// does not move away under it).
fn refuse_own(path: &Path, own: &OwnPlaces) -> Result<(), FilesError> {
    if own.touches(path, true) {
        return Err(FilesError::new(
            FilesErrorCode::HolziOwned,
            "holzi's own data is read-only",
        ));
    }
    Ok(())
}

fn exists(path: &Path) -> bool {
    std::fs::symlink_metadata(path).is_ok()
}

/// Creates the folder `name` in `parent`.
pub fn create_folder(parent: &Path, name: &str, own: &OwnPlaces) -> Result<Entry, FilesError> {
    check_name(name)?;
    let target = parent.join(name);
    refuse_own(&target, own)?;
    if exists(&target) {
        return Err(FilesError::new(FilesErrorCode::Exists, "the name is taken"));
    }
    std::fs::create_dir(&target).map_err(|error| match error.kind() {
        std::io::ErrorKind::AlreadyExists => {
            FilesError::new(FilesErrorCode::Exists, "the name is taken")
        }
        _ => io_error(error, &target),
    })?;
    stat(&target, own)
}

/// Renames the entry `path` to `new_name` in the same folder. A name that is taken is refused,
/// unless it is the entry itself (a change of case on a file system that ignores case).
pub fn rename(path: &Path, new_name: &str, own: &OwnPlaces) -> Result<Entry, FilesError> {
    check_name(new_name)?;
    let parent = path
        .parent()
        .ok_or_else(|| FilesError::invalid_path("path must name an entry"))?;
    let target = parent.join(new_name);
    refuse_own(path, own)?;
    refuse_own(&target, own)?;
    if target == path {
        return stat(path, own);
    }
    if exists(&target) && !same_entry(path, &target) {
        return Err(FilesError::new(FilesErrorCode::Exists, "the name is taken"));
    }
    std::fs::rename(path, &target).map_err(|error| io_error(error, path))?;
    stat(&target, own)
}

/// Whether two spellings name the same entry (case-insensitive file systems).
fn same_entry(a: &Path, b: &Path) -> bool {
    match (std::fs::symlink_metadata(a), std::fs::symlink_metadata(b)) {
        #[cfg(unix)]
        (Ok(a), Ok(b)) => {
            use std::os::unix::fs::MetadataExt;
            a.dev() == b.dev() && a.ino() == b.ino()
        }
        #[cfg(not(unix))]
        (Ok(_), Ok(_)) => a
            .to_string_lossy()
            .eq_ignore_ascii_case(&b.to_string_lossy()),
        _ => false,
    }
}

#[cfg(test)]
#[path = "edit_tests.rs"]
mod tests;
