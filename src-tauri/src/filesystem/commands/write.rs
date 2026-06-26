use std::fs;
use std::path::Path;
use tauri::State;

use crate::AppState;

use super::FsError;

/// Write file contents from base64
#[tauri::command]
pub async fn filesystem_write_file(
    _state: State<'_, AppState>,
    path: String,
    data: String,
) -> Result<(), FsError> {
    let path_ref = Path::new(&path);

    // Create parent directories if needed
    if let Some(parent) = path_ref.parent() {
        if !parent.exists() {
            fs::create_dir_all(parent).map_err(|e| FsError::IoError {
                reason: format!("Failed to create parent directories: {}", e),
            })?;
        }
    }

    // Decode base64
    use base64::{engine::general_purpose::STANDARD, Engine};
    let bytes = STANDARD.decode(&data).map_err(|e| FsError::InvalidPath {
        reason: format!("Invalid base64 data: {}", e),
    })?;

    fs::write(path_ref, &bytes).map_err(|e| FsError::IoError {
        reason: format!("Failed to write '{}': {}", path, e),
    })?;

    Ok(())
}

/// Create a directory (and parent directories if needed)
#[tauri::command]
pub async fn filesystem_mkdir(_state: State<'_, AppState>, path: String) -> Result<(), FsError> {
    fs::create_dir_all(&path).map_err(|e| FsError::IoError {
        reason: format!("Failed to create directory '{}': {}", path, e),
    })?;

    Ok(())
}

/// Remove a file or directory
#[tauri::command]
pub async fn filesystem_remove(
    _state: State<'_, AppState>,
    path: String,
    recursive: Option<bool>,
) -> Result<(), FsError> {
    let path_ref = Path::new(&path);

    if !path_ref.exists() {
        return Err(FsError::NotFound { path });
    }

    if path_ref.is_dir() {
        if recursive.unwrap_or(false) {
            fs::remove_dir_all(path_ref).map_err(|e| FsError::IoError {
                reason: format!("Failed to remove directory '{}': {}", path, e),
            })?;
        } else {
            fs::remove_dir(path_ref).map_err(|e| FsError::IoError {
                reason: format!("Failed to remove directory '{}': {}", path, e),
            })?;
        }
    } else {
        fs::remove_file(path_ref).map_err(|e| FsError::IoError {
            reason: format!("Failed to remove file '{}': {}", path, e),
        })?;
    }

    Ok(())
}

/// Rename/move a file or directory
#[tauri::command]
pub async fn filesystem_rename(
    _state: State<'_, AppState>,
    from: String,
    to: String,
) -> Result<(), FsError> {
    let from_path = Path::new(&from);

    if !from_path.exists() {
        return Err(FsError::NotFound { path: from });
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

    fs::rename(&from, &to).map_err(|e| FsError::IoError {
        reason: format!("Failed to rename '{}' to '{}': {}", from, to, e),
    })?;

    Ok(())
}
