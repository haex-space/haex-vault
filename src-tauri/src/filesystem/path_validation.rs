// src-tauri/src/filesystem/path_validation.rs
//!
//! Path validation utilities shared across modules.
//!
//! Centralizes path traversal checks so that `file_sync`, `peer_storage`,
//! `extension`, and any future consumer use identical security logic.

use std::path::{Component, Path};

/// Reject unsafe path components: null bytes, `..`, absolute paths, prefixes.
///
/// Does **not** reject empty paths — that is a business-logic concern, not a
/// security one.  Use [`check_relative_path`] when an empty path is also invalid.
pub fn reject_path_traversal(path: &str) -> Result<(), String> {
    if path.contains('\0') {
        return Err(format!("Path contains null byte: {path}"));
    }
    let p = Path::new(path);
    if p.is_absolute() {
        return Err(format!("Path must be relative: {path}"));
    }
    for component in p.components() {
        match component {
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(format!("Path traversal denied: {path}"));
            }
            _ => {}
        }
    }
    Ok(())
}

/// Validate that `path` is a non-empty, safe relative path.
///
/// Combines the empty-check with [`reject_path_traversal`].
pub fn check_relative_path(path: &str) -> Result<(), String> {
    if path.is_empty() {
        return Err("Path must not be empty".to_string());
    }
    reject_path_traversal(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_simple_relative_paths() {
        assert!(check_relative_path("file.txt").is_ok());
        assert!(check_relative_path("dir/file.txt").is_ok());
        assert!(check_relative_path("a/b/c/d.txt").is_ok());
        assert!(check_relative_path(".hidden").is_ok());
        assert!(check_relative_path("dir/.hidden/file").is_ok());
    }

    #[test]
    fn rejects_empty() {
        assert!(check_relative_path("").is_err());
    }

    #[test]
    fn rejects_null_bytes() {
        assert!(check_relative_path("file\0.txt").is_err());
    }

    #[test]
    fn rejects_absolute_paths() {
        assert!(check_relative_path("/etc/passwd").is_err());
    }

    #[test]
    fn rejects_parent_traversal() {
        assert!(check_relative_path("..").is_err());
        assert!(check_relative_path("../secret").is_err());
        assert!(check_relative_path("dir/../../etc").is_err());
        assert!(check_relative_path("a/b/../../../outside").is_err());
    }

    #[test]
    fn traversal_check_allows_empty() {
        assert!(reject_path_traversal("").is_ok());
    }

    #[test]
    fn traversal_check_rejects_dotdot() {
        assert!(reject_path_traversal("..").is_err());
        assert!(reject_path_traversal("a/../b").is_err());
    }
}
