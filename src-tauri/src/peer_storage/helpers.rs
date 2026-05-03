//! Path resolution, filesystem helpers, and UCAN utilities for peer storage.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::peer_storage::endpoint::SharedFolder;
use crate::peer_storage::protocol::{FileEntry, Response};

// ============================================================================
// Path resolution (with space-based access control)
// ============================================================================

/// Filter shares to only those the remote peer is allowed to access.
pub(super) fn filter_shares<'a>(
    shares: &'a HashMap<String, SharedFolder>,
    allowed_spaces: &HashSet<String>,
) -> HashMap<&'a String, &'a SharedFolder> {
    shares
        .iter()
        .filter(|(_, share)| allowed_spaces.contains(&share.space_id))
        .collect()
}

/// Find a share by name (or ID) and extract the sub-path within it.
pub(super) fn find_share_and_subpath<'a>(
    shares: &'a HashMap<String, SharedFolder>,
    allowed_spaces: &HashSet<String>,
    request_path: &str,
) -> Result<(&'a SharedFolder, String), Response> {
    let trimmed = request_path.trim_start_matches('/');
    let (share_name, sub_path) = trimmed.split_once('/').unwrap_or((trimmed, ""));

    let share = shares
        .values()
        .find(|s| s.name == share_name && allowed_spaces.contains(&s.space_id))
        .or_else(|| {
            shares
                .get(share_name)
                .filter(|s| allowed_spaces.contains(&s.space_id))
        })
        .ok_or_else(|| Response::Error {
            message: format!("Share not found: {share_name}"),
        })?;

    Ok((share, sub_path.to_string()))
}

/// Resolve a request path to a local filesystem path (desktop / standard paths).
pub(super) fn resolve_path_filtered(
    shares: &HashMap<String, SharedFolder>,
    allowed_spaces: &HashSet<String>,
    request_path: &str,
) -> Result<PathBuf, Response> {
    let (share, sub_path) = find_share_and_subpath(shares, allowed_spaces, request_path)?;

    crate::filesystem::reject_path_traversal(&sub_path)
        .map_err(|message| Response::Error { message })?;

    let full_path = PathBuf::from(&share.local_path).join(&sub_path);

    let canonical = full_path
        .canonicalize()
        .map_err(|_| Response::Error {
            message: "Path not found".to_string(),
        })?;
    let share_canonical =
        PathBuf::from(&share.local_path)
            .canonicalize()
            .map_err(|_| Response::Error {
                message: "Share path invalid".to_string(),
            })?;

    if !canonical.starts_with(&share_canonical) {
        return Err(Response::Error {
            message: "Access denied: path outside share".to_string(),
        });
    }

    Ok(canonical)
}

/// Resolve a request path for write operations.
/// Similar to `resolve_path_filtered` but doesn't require the path to exist yet
/// (needed for creating new files/directories).
pub(super) fn resolve_path_for_write(
    shares: &HashMap<String, SharedFolder>,
    allowed_spaces: &HashSet<String>,
    request_path: &str,
) -> Result<PathBuf, Response> {
    let (share, sub_path) = find_share_and_subpath(shares, allowed_spaces, request_path)?;

    crate::filesystem::check_relative_path(&sub_path)
        .map_err(|message| Response::Error { message })?;

    let full_path = PathBuf::from(&share.local_path).join(&sub_path);

    let share_canonical =
        PathBuf::from(&share.local_path)
            .canonicalize()
            .map_err(|_| Response::Error {
                message: "Share path invalid".to_string(),
            })?;

    let parent = full_path.parent().unwrap_or(&full_path);
    if parent.exists() {
        let parent_canonical = parent.canonicalize().map_err(|_| Response::Error {
            message: "Parent path invalid".to_string(),
        })?;
        if !parent_canonical.starts_with(&share_canonical) {
            return Err(Response::Error {
                message: "Access denied: path outside share".to_string(),
            });
        }
    }

    Ok(full_path)
}

/// Determine which space a request path targets, for UCAN capability checking.
pub(super) fn find_space_for_path(
    shares: &HashMap<String, SharedFolder>,
    allowed_spaces: &HashSet<String>,
    request_path: &str,
) -> Option<String> {
    if request_path.is_empty() || request_path == "/" {
        return None; // Root listing — no specific space
    }
    find_share_and_subpath(shares, allowed_spaces, request_path)
        .ok()
        .map(|(share, _)| share.space_id.clone())
}

// ============================================================================
// Filesystem helpers
// ============================================================================

pub(super) async fn read_dir_entries(dir: &Path) -> Result<Vec<FileEntry>, std::io::Error> {
    let mut entries = Vec::new();
    let mut read_dir = tokio::fs::read_dir(dir).await?;

    while let Some(entry) = read_dir.next_entry().await? {
        if let Ok(fe) = file_entry_from_dir_entry(&entry).await {
            entries.push(fe);
        }
    }

    entries.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then(a.name.cmp(&b.name)));
    Ok(entries)
}

async fn file_entry_from_dir_entry(
    entry: &tokio::fs::DirEntry,
) -> Result<FileEntry, std::io::Error> {
    let metadata = entry.metadata().await?;
    let modified = metadata
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs());

    Ok(FileEntry {
        name: entry.file_name().to_string_lossy().to_string(),
        size: metadata.len(),
        is_dir: metadata.is_dir(),
        modified,
    })
}

pub(super) fn file_entry_from_path(path: &Path) -> Result<FileEntry, std::io::Error> {
    let metadata = std::fs::metadata(path)?;
    let modified = metadata
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs());

    Ok(FileEntry {
        name: path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default(),
        size: metadata.len(),
        is_dir: metadata.is_dir(),
        modified,
    })
}

/// Recursively scan a directory and collect FileState entries for the manifest.
pub(super) fn scan_directory_recursive(
    dir: &Path,
    base: &Path,
) -> Result<Vec<crate::file_sync::types::FileState>, std::io::Error> {
    let mut entries = Vec::new();
    let read_dir = std::fs::read_dir(dir)?;

    for entry in read_dir {
        let entry = entry?;
        let metadata = entry.metadata()?;

        let relative = entry
            .path()
            .strip_prefix(base)
            .unwrap_or(&entry.path())
            .to_string_lossy()
            .to_string()
            .replace('\\', "/");

        let modified_at = metadata
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs())
            .unwrap_or(0);

        let size = if metadata.is_dir() { 0 } else { metadata.len() };
        let hash = if metadata.is_dir() {
            None
        } else {
            // Cached SHA-256 — same scheme as the sender side. The diff
            // engine treats files with matching hashes as equal regardless
            // of mtime, so transfers do not re-fire after every sync.
            crate::file_sync::hashing::cached_hash(&entry.path(), size, modified_at).ok()
        };

        entries.push(crate::file_sync::types::FileState {
            relative_path: relative,
            size,
            modified_at,
            is_directory: metadata.is_dir(),
            hash,
        });

        if metadata.is_dir() {
            entries.extend(scan_directory_recursive(&entry.path(), base)?);
        }
    }

    Ok(entries)
}
