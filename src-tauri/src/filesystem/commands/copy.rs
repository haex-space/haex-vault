use std::fs;
use std::path::Path;
use tauri::State;

use crate::AppState;

use super::FsError;

/// Recursively copy a directory
#[tauri::command]
pub async fn filesystem_copy_dir(
    _state: State<'_, AppState>,
    from: String,
    to: String,
) -> Result<(), FsError> {
    let from_path = Path::new(&from);
    let to_path = Path::new(&to);

    if !from_path.exists() {
        return Err(FsError::NotFound { path: from });
    }
    if !from_path.is_dir() {
        return Err(FsError::NotADirectory { path: from });
    }

    // Reject copies where the destination is the source or nested inside it —
    // `copy_dir_recursive` creates `dst` first then iterates `src`, so the
    // newly created destination would be re-traversed until the path explodes.
    if destination_is_inside_source(from_path, to_path) {
        return Err(FsError::InvalidPath {
            reason: format!(
                "Destination '{}' is the source or nested inside source '{}'",
                to, from
            ),
        });
    }

    copy_dir_recursive(from_path, to_path).map_err(|e| FsError::IoError {
        reason: format!("Failed to copy directory '{}' to '{}': {}", from, to, e),
    })?;

    Ok(())
}

/// Returns true when `to` resolves to `from` or to a descendant of `from`.
/// Resolution falls back to lexical comparison if canonicalization fails
/// (which is safe: a missing-but-clearly-nested raw path also gets rejected).
fn destination_is_inside_source(from: &Path, to: &Path) -> bool {
    let from_canon = fs::canonicalize(from).unwrap_or_else(|_| from.to_path_buf());

    // The destination may not exist yet; canonicalize the closest existing
    // ancestor and re-attach the unresolved tail.
    let mut ancestor = to;
    let mut tail = std::path::PathBuf::new();
    let canon_ancestor = loop {
        if let Ok(c) = fs::canonicalize(ancestor) {
            break c;
        }
        match (ancestor.file_name(), ancestor.parent()) {
            (Some(name), Some(parent)) if parent != ancestor => {
                tail = Path::new(name).join(&tail);
                ancestor = parent;
            }
            _ => return to.starts_with(from),
        }
    };
    let projected_to = canon_ancestor.join(&tail);
    projected_to == from_canon || projected_to.starts_with(&from_canon)
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> std::io::Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let dest_path = dst.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir_recursive(&entry.path(), &dest_path)?;
        } else {
            fs::copy(entry.path(), &dest_path)?;
        }
    }
    Ok(())
}

/// Copy a file
#[tauri::command]
pub async fn filesystem_copy(
    _state: State<'_, AppState>,
    from: String,
    to: String,
) -> Result<(), FsError> {
    let from_path = Path::new(&from);

    if !from_path.exists() {
        return Err(FsError::NotFound { path: from });
    }

    if !from_path.is_file() {
        return Err(FsError::NotAFile { path: from });
    }

    // Create parent directories for destination if needed
    let to_path = Path::new(&to);
    if let Some(parent) = to_path.parent() {
        if !parent.exists() {
            fs::create_dir_all(parent).map_err(|e| FsError::IoError {
                reason: format!("Failed to create parent directories: {}", e),
            })?;
        }
    }

    fs::copy(&from, &to).map_err(|e| FsError::IoError {
        reason: format!("Failed to copy '{}' to '{}': {}", from, to, e),
    })?;

    Ok(())
}
