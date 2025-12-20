// src-tauri/src/filesystem/commands.rs
//!
//! Generic filesystem commands
//!
//! These commands provide low-level filesystem access that can be used by
//! extensions and other parts of the application for local file operations.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use std::time::UNIX_EPOCH;
use tauri::State;
use thiserror::Error;
use ts_rs::TS;

use crate::AppState;

// ============================================================================
// Error Types
// ============================================================================

#[derive(Debug, Error)]
pub enum FsError {
    #[error("File not found: {path}")]
    NotFound { path: String },

    #[error("Permission denied: {path}")]
    PermissionDenied { path: String },

    #[error("I/O error: {reason}")]
    IoError { reason: String },

    #[error("Invalid path: {reason}")]
    InvalidPath { reason: String },

    #[error("Not a directory: {path}")]
    NotADirectory { path: String },

    #[error("Not a file: {path}")]
    NotAFile { path: String },
}

impl From<std::io::Error> for FsError {
    fn from(e: std::io::Error) -> Self {
        match e.kind() {
            std::io::ErrorKind::NotFound => FsError::NotFound {
                path: "unknown".to_string(),
            },
            std::io::ErrorKind::PermissionDenied => FsError::PermissionDenied {
                path: "unknown".to_string(),
            },
            _ => FsError::IoError {
                reason: e.to_string(),
            },
        }
    }
}

impl Serialize for FsError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

// ============================================================================
// Types
// ============================================================================

/// File/directory metadata
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct FileStat {
    /// File size in bytes
    pub size: u64,
    /// True if this is a file
    pub is_file: bool,
    /// True if this is a directory
    pub is_directory: bool,
    /// True if this is a symbolic link
    pub is_symlink: bool,
    /// Last modified time (Unix timestamp in milliseconds)
    pub modified: Option<u64>,
    /// Created time (Unix timestamp in milliseconds)
    pub created: Option<u64>,
    /// Whether the file is read-only
    pub readonly: bool,
}

/// Directory entry
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct DirEntry {
    /// Entry name (not full path)
    pub name: String,
    /// Full path
    pub path: String,
    /// True if this is a file
    pub is_file: bool,
    /// True if this is a directory
    pub is_directory: bool,
    /// File size in bytes (0 for directories)
    pub size: u64,
    /// Last modified time (Unix timestamp in milliseconds)
    pub modified: Option<u64>,
}

// ============================================================================
// Commands
// ============================================================================

/// Read file contents as base64
#[tauri::command]
pub async fn filesystem_read_file(
    _state: State<'_, AppState>,
    path: String,
) -> Result<String, FsError> {
    let path_ref = Path::new(&path);

    if !path_ref.exists() {
        return Err(FsError::NotFound { path });
    }

    if !path_ref.is_file() {
        return Err(FsError::NotAFile { path });
    }

    let data = fs::read(path_ref).map_err(|e| FsError::IoError {
        reason: format!("Failed to read '{}': {}", path, e),
    })?;

    // Return as base64
    use base64::{engine::general_purpose::STANDARD, Engine};
    Ok(STANDARD.encode(&data))
}

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

/// Read directory contents
#[tauri::command]
pub async fn filesystem_read_dir(
    _state: State<'_, AppState>,
    path: String,
) -> Result<Vec<DirEntry>, FsError> {
    let path_ref = Path::new(&path);

    if !path_ref.exists() {
        return Err(FsError::NotFound { path });
    }

    if !path_ref.is_dir() {
        return Err(FsError::NotADirectory { path });
    }

    let mut entries = Vec::new();

    for entry in fs::read_dir(path_ref).map_err(|e| FsError::IoError {
        reason: format!("Failed to read directory '{}': {}", path, e),
    })? {
        let entry = entry.map_err(|e| FsError::IoError {
            reason: format!("Failed to read entry: {}", e),
        })?;

        let metadata = entry.metadata().map_err(|e| FsError::IoError {
            reason: format!("Failed to read metadata: {}", e),
        })?;

        let modified = metadata
            .modified()
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_millis() as u64);

        entries.push(DirEntry {
            name: entry.file_name().to_string_lossy().to_string(),
            path: entry.path().to_string_lossy().to_string(),
            is_file: metadata.is_file(),
            is_directory: metadata.is_dir(),
            size: if metadata.is_file() { metadata.len() } else { 0 },
            modified,
        });
    }

    // Sort: directories first, then files, both alphabetically
    entries.sort_by(|a, b| {
        match (a.is_directory, b.is_directory) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
        }
    });

    Ok(entries)
}

/// Create a directory (and parent directories if needed)
#[tauri::command]
pub async fn filesystem_mkdir(
    _state: State<'_, AppState>,
    path: String,
) -> Result<(), FsError> {
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

/// Check if a path exists
#[tauri::command]
pub async fn filesystem_exists(
    _state: State<'_, AppState>,
    path: String,
) -> Result<bool, FsError> {
    Ok(Path::new(&path).exists())
}

/// Get file/directory metadata
#[tauri::command]
pub async fn filesystem_stat(
    _state: State<'_, AppState>,
    path: String,
) -> Result<FileStat, FsError> {
    let path_ref = Path::new(&path);

    if !path_ref.exists() {
        return Err(FsError::NotFound { path });
    }

    let metadata = fs::metadata(path_ref).map_err(|e| FsError::IoError {
        reason: format!("Failed to read metadata for '{}': {}", path, e),
    })?;

    let modified = metadata
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as u64);

    let created = metadata
        .created()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as u64);

    Ok(FileStat {
        size: metadata.len(),
        is_file: metadata.is_file(),
        is_directory: metadata.is_dir(),
        is_symlink: metadata.file_type().is_symlink(),
        modified,
        created,
        readonly: metadata.permissions().readonly(),
    })
}

/// Open a folder selection dialog
#[tauri::command]
pub async fn filesystem_select_folder(
    window: tauri::WebviewWindow,
    title: Option<String>,
    default_path: Option<String>,
) -> Result<Option<String>, FsError> {
    use tauri_plugin_dialog::DialogExt;

    let mut dialog = window.dialog().file();

    if let Some(t) = title {
        dialog = dialog.set_title(&t);
    }

    if let Some(path) = default_path {
        dialog = dialog.set_directory(&path);
    }

    let selected = dialog.blocking_pick_folder();

    Ok(selected.and_then(|p| p.as_path().map(|path| path.to_string_lossy().to_string())))
}

/// Open a file selection dialog
#[tauri::command]
pub async fn filesystem_select_file(
    window: tauri::WebviewWindow,
    title: Option<String>,
    default_path: Option<String>,
    filters: Option<Vec<(String, Vec<String>)>>,
    multiple: Option<bool>,
) -> Result<Option<Vec<String>>, FsError> {
    use tauri_plugin_dialog::DialogExt;

    let mut dialog = window.dialog().file();

    if let Some(t) = title {
        dialog = dialog.set_title(&t);
    }

    if let Some(path) = default_path {
        dialog = dialog.set_directory(&path);
    }

    if let Some(f) = filters {
        for (name, extensions) in f {
            let ext_refs: Vec<&str> = extensions.iter().map(|s| s.as_str()).collect();
            dialog = dialog.add_filter(&name, &ext_refs);
        }
    }

    if multiple.unwrap_or(false) {
        let selected = dialog.blocking_pick_files();
        Ok(selected.map(|paths| {
            paths
                .into_iter()
                .filter_map(|p| p.as_path().map(|path| path.to_string_lossy().to_string()))
                .collect()
        }))
    } else {
        let selected = dialog.blocking_pick_file();
        Ok(selected.and_then(|p| p.as_path().map(|path| vec![path.to_string_lossy().to_string()])))
    }
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
