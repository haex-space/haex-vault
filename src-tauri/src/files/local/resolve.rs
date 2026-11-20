//! The real target of a path (spec 017 FR-047, spec 044 FR-033, research R3): every check runs on
//! it, never on the text a caller sent, so `..`, a symbolic link or another spelling cannot leave a
//! folder or reach one of haex-vault's own places.

use std::path::{Component, Path, PathBuf};

use crate::files::FilesError;

/// The canonical target of the absolute `path`. A target that does not exist yet (a file to
/// write, a folder to create) resolves through its nearest existing ancestor; the rest is appended
/// and may hold no `.` or `..`. A part that is there but does not resolve is a broken or looping
/// link: writing through it would reach a target nobody checked, so it is refused.
pub fn resolve(path: &Path) -> Result<PathBuf, FilesError> {
    if !path.is_absolute() {
        return Err(FilesError::invalid_path("path must be absolute"));
    }
    if let Ok(real) = std::fs::canonicalize(path) {
        return Ok(real);
    }
    let mut rest: Vec<&std::ffi::OsStr> = Vec::new();
    let mut ancestor = path;
    loop {
        if std::fs::symlink_metadata(ancestor).is_ok() {
            return Err(FilesError::invalid_path("path holds a broken link"));
        }
        let parent = ancestor
            .parent()
            .ok_or_else(|| FilesError::invalid_path("path has no existing ancestor"))?;
        match ancestor.components().next_back() {
            Some(Component::Normal(name)) => rest.push(name),
            _ => return Err(FilesError::invalid_path("path must not contain . or ..")),
        }
        if let Ok(real) = std::fs::canonicalize(parent) {
            let mut resolved = real;
            for name in rest.into_iter().rev() {
                resolved.push(name);
            }
            return Ok(resolved);
        }
        ancestor = parent;
    }
}

/// Where the entry `path` itself lives: its folder resolved, its own name kept. A link stays the
/// link, so renaming, moving or deleting it never reaches its target (spec 044 FR-017 to FR-023).
pub fn resolve_entry(path: &Path) -> Result<PathBuf, FilesError> {
    let name = match path.components().next_back() {
        Some(Component::Normal(name)) => name,
        _ => return Err(FilesError::invalid_path("path must name an entry")),
    };
    let parent = path
        .parent()
        .ok_or_else(|| FilesError::invalid_path("path must name an entry"))?;
    Ok(resolve(parent)?.join(name))
}

#[cfg(test)]
#[path = "resolve_tests.rs"]
mod tests;
