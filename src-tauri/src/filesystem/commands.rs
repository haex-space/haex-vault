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

    #[allow(dead_code)]
    #[error("Dialog cancelled by user")]
    DialogCancelled,
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
    #[allow(unused_variables)] window: tauri::WebviewWindow,
    #[allow(unused_variables)] title: Option<String>,
    #[allow(unused_variables)] default_path: Option<String>,
    #[allow(unused_variables)] app_handle: tauri::AppHandle,
) -> Result<Option<String>, FsError> {
    #[cfg(not(target_os = "android"))]
    {
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

    #[cfg(target_os = "android")]
    {
        use tauri_plugin_android_fs::AndroidFsExt;

        let api = app_handle.android_fs();
        let picker = api.file_picker();

        let selected = picker.pick_dir(None, false).map_err(|e| FsError::IoError {
            reason: format!("Android folder picker error: {:?}", e),
        })?;

        match selected {
            Some(uri) => {
                let uri_json = uri.to_json_string().map_err(|e| FsError::IoError {
                    reason: format!("Failed to serialize URI: {:?}", e),
                })?;
                Ok(Some(uri_json))
            }
            None => Ok(None),
        }
    }
}

/// Open a file selection dialog
#[tauri::command]
pub async fn filesystem_select_file(
    #[allow(unused_variables)] window: tauri::WebviewWindow,
    #[allow(unused_variables)] title: Option<String>,
    #[allow(unused_variables)] default_path: Option<String>,
    #[allow(unused_variables)] filters: Option<Vec<(String, Vec<String>)>>,
    #[allow(unused_variables)] multiple: Option<bool>,
    #[allow(unused_variables)] app_handle: tauri::AppHandle,
) -> Result<Option<Vec<String>>, FsError> {
    #[cfg(not(target_os = "android"))]
    {
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

    #[cfg(target_os = "android")]
    {
        use tauri_plugin_android_fs::AndroidFsExt;

        let api = app_handle.android_fs();
        let picker = api.file_picker();

        // Convert extension filters to MIME types for Android
        let mime_types: Vec<String> = filters
            .as_ref()
            .map(|f| {
                f.iter()
                    .flat_map(|(_, extensions)| {
                        extensions.iter().map(|ext| {
                            match ext.to_lowercase().as_str() {
                                "jpg" | "jpeg" => "image/jpeg".to_string(),
                                "png" => "image/png".to_string(),
                                "gif" => "image/gif".to_string(),
                                "webp" => "image/webp".to_string(),
                                "svg" => "image/svg+xml".to_string(),
                                "pdf" => "application/pdf".to_string(),
                                "txt" => "text/plain".to_string(),
                                "json" => "application/json".to_string(),
                                "xml" => "application/xml".to_string(),
                                "zip" => "application/zip".to_string(),
                                "mp3" => "audio/mpeg".to_string(),
                                "mp4" => "video/mp4".to_string(),
                                "doc" => "application/msword".to_string(),
                                "docx" => "application/vnd.openxmlformats-officedocument.wordprocessingml.document".to_string(),
                                "xls" => "application/vnd.ms-excel".to_string(),
                                "xlsx" => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet".to_string(),
                                _ => "*/*".to_string(),
                            }
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();

        let mime_refs: Vec<&str> = if mime_types.is_empty() {
            vec!["*/*"]
        } else {
            mime_types.iter().map(|s| s.as_str()).collect()
        };

        if multiple.unwrap_or(false) {
            let selected = picker.pick_files(None, &mime_refs, false).map_err(|e| FsError::IoError {
                reason: format!("Android file picker error: {:?}", e),
            })?;

            if selected.is_empty() {
                Ok(None)
            } else {
                let uris: Result<Vec<String>, FsError> = selected
                    .into_iter()
                    .map(|uri| {
                        uri.to_json_string().map_err(|e| FsError::IoError {
                            reason: format!("Failed to serialize URI: {:?}", e),
                        })
                    })
                    .collect();
                Ok(Some(uris?))
            }
        } else {
            let selected = picker.pick_file(None, &mime_refs, false).map_err(|e| FsError::IoError {
                reason: format!("Android file picker error: {:?}", e),
            })?;

            match selected {
                Some(uri) => {
                    let uri_json = uri.to_json_string().map_err(|e| FsError::IoError {
                        reason: format!("Failed to serialize URI: {:?}", e),
                    })?;
                    Ok(Some(vec![uri_json]))
                }
                None => Ok(None),
            }
        }
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
