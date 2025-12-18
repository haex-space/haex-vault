// src-tauri/src/extension/filesync/file_io.rs
//!
//! FileSync-specific file I/O wrappers
//!
//! Re-exports and wraps filesystem::file_io functions with FileSyncError.
//!

use crate::extension::filesync::error::FileSyncError;
use crate::extension::filesystem::file_io::{self, FileIoError};

// Re-export path helpers directly
pub use crate::extension::filesystem::file_io::{extract_filename, is_content_uri};

// ============================================================================
// Error Conversion
// ============================================================================

impl From<FileIoError> for FileSyncError {
    fn from(e: FileIoError) -> Self {
        FileSyncError::FilesystemError {
            reason: e.to_string(),
        }
    }
}

// ============================================================================
// Desktop File I/O (wrapped for FileSyncError)
// ============================================================================

/// Read file bytes on desktop platforms
#[cfg(desktop)]
pub fn read_file_bytes(path: &str) -> Result<Vec<u8>, FileSyncError> {
    file_io::read_file_bytes(path).map_err(Into::into)
}

/// Write file bytes on desktop platforms
#[cfg(desktop)]
pub fn write_file_bytes(path: &str, data: &[u8]) -> Result<(), FileSyncError> {
    file_io::write_file(path, data).map_err(Into::into)
}

// ============================================================================
// Android File I/O (wrapped for FileSyncError)
// ============================================================================

/// Read file bytes on Android using content URI
#[cfg(target_os = "android")]
pub fn read_file_bytes_android(
    app_handle: &tauri::AppHandle,
    path: &str,
) -> Result<Vec<u8>, FileSyncError> {
    file_io::read_file_bytes_android(app_handle, path).map_err(Into::into)
}

/// Write file bytes on Android using content URI
#[cfg(target_os = "android")]
pub fn write_file_bytes_android(
    app_handle: &tauri::AppHandle,
    path: &str,
    data: &[u8],
) -> Result<(), FileSyncError> {
    file_io::write_file_bytes_android(app_handle, path, data).map_err(Into::into)
}
