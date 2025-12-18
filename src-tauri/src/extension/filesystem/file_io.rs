// src-tauri/src/extension/filesystem/file_io.rs
//!
//! Platform-agnostic file I/O operations
//!
//! Provides unified file read/write operations that work across desktop and Android platforms.
//!

use std::path::{Path, PathBuf};
use thiserror::Error;

/// Errors that can occur during file I/O operations
#[derive(Debug, Error)]
pub enum FileIoError {
    #[error("Failed to read file '{path}': {reason}")]
    ReadError { path: String, reason: String },

    #[error("Failed to write file '{path}': {reason}")]
    WriteError { path: String, reason: String },

    #[error("Invalid file path '{path}': {reason}")]
    InvalidPath { path: String, reason: String },
}

// ============================================================================
// Path Helpers
// ============================================================================

/// Extract filename from path (handles both Unix paths and Android content URIs)
pub fn extract_filename(path: &str) -> String {
    if path.starts_with("content://") {
        path.rsplit('/').next().unwrap_or("unknown").to_string()
    } else {
        Path::new(path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string()
    }
}

/// Check if a path is an Android content URI
pub fn is_content_uri(path: &str) -> bool {
    path.starts_with("content://")
}

// ============================================================================
// Desktop File I/O
// ============================================================================

/// Read file bytes on desktop platforms
#[cfg(desktop)]
pub fn read_file_bytes(path: &str) -> Result<Vec<u8>, FileIoError> {
    std::fs::read(path).map_err(|e| FileIoError::ReadError {
        path: path.to_string(),
        reason: e.to_string(),
    })
}

/// Write file bytes on desktop platforms
#[cfg(desktop)]
pub fn write_file_bytes(path: &str, data: &[u8]) -> Result<(), FileIoError> {
    std::fs::write(path, data).map_err(|e| FileIoError::WriteError {
        path: path.to_string(),
        reason: e.to_string(),
    })
}

/// Check if a file exists on desktop platforms
#[cfg(desktop)]
pub fn file_exists(path: &str) -> bool {
    Path::new(path).exists()
}

/// Create parent directories if they don't exist on desktop platforms
#[cfg(desktop)]
pub fn create_parent_dirs(path: &str) -> Result<(), FileIoError> {
    if let Some(parent) = Path::new(path).parent() {
        if !parent.exists() {
            std::fs::create_dir_all(parent).map_err(|e| FileIoError::WriteError {
                path: path.to_string(),
                reason: format!("Failed to create parent directories: {}", e),
            })?;
        }
    }
    Ok(())
}

// ============================================================================
// Android File I/O
// ============================================================================

/// Read file bytes on Android using content URI
#[cfg(target_os = "android")]
pub fn read_file_bytes_android(
    app_handle: &tauri::AppHandle,
    path: &str,
) -> Result<Vec<u8>, FileIoError> {
    use tauri_plugin_android_fs::AndroidFsExt;

    let api = app_handle.android_fs();
    let path_buf = PathBuf::from(path);
    let file_uri = tauri_plugin_android_fs::FileUri::from(&path_buf);

    api.read(&file_uri).map_err(|e| FileIoError::ReadError {
        path: path.to_string(),
        reason: format!("{:?}", e),
    })
}

/// Write file bytes on Android using content URI
#[cfg(target_os = "android")]
pub fn write_file_bytes_android(
    app_handle: &tauri::AppHandle,
    path: &str,
    data: &[u8],
) -> Result<(), FileIoError> {
    use tauri_plugin_android_fs::AndroidFsExt;

    let api = app_handle.android_fs();
    let path_buf = PathBuf::from(path);
    let file_uri = tauri_plugin_android_fs::FileUri::from(&path_buf);

    api.write(&file_uri, data)
        .map_err(|e| FileIoError::WriteError {
            path: path.to_string(),
            reason: format!("{:?}", e),
        })
}

// ============================================================================
// Cross-platform helpers
// ============================================================================

/// Read file bytes - automatically handles desktop vs Android
#[cfg(desktop)]
pub fn read_file(path: &str) -> Result<Vec<u8>, FileIoError> {
    read_file_bytes(path)
}

/// Write file bytes - automatically handles desktop vs Android
#[cfg(desktop)]
pub fn write_file(path: &str, data: &[u8]) -> Result<(), FileIoError> {
    create_parent_dirs(path)?;
    write_file_bytes(path, data)
}

/// Read file bytes on Android
#[cfg(target_os = "android")]
pub fn read_file_with_handle(
    app_handle: &tauri::AppHandle,
    path: &str,
) -> Result<Vec<u8>, FileIoError> {
    read_file_bytes_android(app_handle, path)
}

/// Write file bytes on Android
#[cfg(target_os = "android")]
pub fn write_file_with_handle(
    app_handle: &tauri::AppHandle,
    path: &str,
    data: &[u8],
) -> Result<(), FileIoError> {
    write_file_bytes_android(app_handle, path, data)
}
