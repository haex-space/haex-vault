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

    copy_dir_recursive(from_path, to_path).map_err(|e| FsError::IoError {
        reason: format!("Failed to copy directory '{}' to '{}': {}", from, to, e),
    })?;

    Ok(())
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
