// src-tauri/src/extension/filesync/scanner.rs
//!
//! Local directory scanning for FileSync
//!

use crate::extension::filesync::error::FileSyncError;
use crate::extension::filesync::types::LocalFileInfo;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

// ============================================================================
// ID Generation
// ============================================================================

/// Generate an ID from the rule_id and relative path using SHA256
/// Including rule_id ensures uniqueness across different sync rules (same file can be in multiple rules)
/// Using relative path ensures the same logical file has the same ID across devices within a rule
pub fn generate_file_id(rule_id: &str, relative_path: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(rule_id.as_bytes());
    hasher.update(b":");
    hasher.update(relative_path.as_bytes());
    let result = hasher.finalize();
    hex::encode(result)
}

// ============================================================================
// Date/Time Formatting
// ============================================================================

/// Format a Unix timestamp as ISO 8601 string
pub fn format_unix_timestamp(secs: u64) -> String {
    let days_since_1970 = secs / 86400;
    let time_of_day = secs % 86400;
    let hours = time_of_day / 3600;
    let minutes = (time_of_day % 3600) / 60;
    let seconds = time_of_day % 60;

    // Calculate year, month, day
    let mut year = 1970u64;
    let mut remaining_days = days_since_1970;

    loop {
        let days_in_year = if is_leap_year(year) { 366 } else { 365 };
        if remaining_days < days_in_year {
            break;
        }
        remaining_days -= days_in_year;
        year += 1;
    }

    let is_leap = is_leap_year(year);
    let days_in_months: [u64; 12] = if is_leap {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };

    let mut month = 1u64;
    for days in days_in_months.iter() {
        if remaining_days < *days {
            break;
        }
        remaining_days -= days;
        month += 1;
    }

    let day = remaining_days + 1;

    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        year, month, day, hours, minutes, seconds
    )
}

fn is_leap_year(year: u64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0)
}

// ============================================================================
// Desktop Directory Scanning
// ============================================================================

/// Scan a local directory on desktop platforms
#[cfg(desktop)]
pub fn scan_local_directory_desktop(
    rule_id: &str,
    scan_path: &str,
    base_path: &str,
) -> Result<Vec<LocalFileInfo>, FileSyncError> {
    use std::fs;
    use std::time::UNIX_EPOCH;

    let path = Path::new(scan_path);
    if !path.exists() {
        return Err(FileSyncError::FilesystemError {
            reason: format!("Path does not exist: {}", scan_path),
        });
    }

    if !path.is_dir() {
        return Err(FileSyncError::FilesystemError {
            reason: format!("Path is not a directory: {}", scan_path),
        });
    }

    let entries = fs::read_dir(path).map_err(|e| FileSyncError::FilesystemError {
        reason: format!("Failed to read directory '{}': {}", scan_path, e),
    })?;

    let mut files = Vec::new();

    for entry in entries {
        let entry = entry.map_err(|e| FileSyncError::FilesystemError {
            reason: format!("Failed to read directory entry: {}", e),
        })?;

        let entry_path = entry.path();
        let full_path = entry_path.to_string_lossy().to_string();

        // Calculate relative path from base_path
        let relative_path = entry_path
            .strip_prefix(base_path)
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|_| {
                entry_path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default()
            });

        let metadata = entry.metadata().map_err(|e| FileSyncError::FilesystemError {
            reason: format!("Failed to read metadata for '{}': {}", full_path, e),
        })?;

        let is_directory = metadata.is_dir();
        let size = if is_directory { 0 } else { metadata.len() };

        let modified_at = metadata
            .modified()
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| format_unix_timestamp(d.as_secs()));

        let name = entry
            .file_name()
            .to_string_lossy()
            .to_string();

        // Detect MIME type from extension
        let mime_type = if is_directory {
            None
        } else {
            entry_path
                .extension()
                .and_then(|ext| ext.to_str())
                .map(|ext| mime_guess::from_ext(ext).first_or_octet_stream().to_string())
        };

        // Generate ID from rule_id + relative path (unique per rule, same across devices)
        let id = generate_file_id(rule_id, &relative_path);

        files.push(LocalFileInfo {
            id,
            name,
            path: full_path,
            relative_path,
            mime_type,
            size,
            is_directory,
            modified_at,
        });
    }

    // Sort: directories first, then by name
    files.sort_by(|a, b| {
        match (a.is_directory, b.is_directory) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
        }
    });

    Ok(files)
}

// ============================================================================
// Android Directory Scanning
// ============================================================================

/// Scan a local directory on Android using SAF
#[cfg(target_os = "android")]
pub fn scan_local_directory_android(
    app_handle: &tauri::AppHandle,
    rule_id: &str,
    scan_path: &str,
    base_path: &str,
) -> Result<Vec<LocalFileInfo>, FileSyncError> {
    use tauri_plugin_android_fs::AndroidFsExt;

    let api = app_handle.android_fs();

    let path_buf = PathBuf::from(scan_path);
    let dir_uri = tauri_plugin_android_fs::FileUri::from(&path_buf);

    let entries = api.read_dir(&dir_uri).map_err(|e| FileSyncError::FilesystemError {
        reason: format!("Failed to list directory '{}': {:?}", scan_path, e),
    })?;

    let mut files = Vec::new();

    for entry in entries {
        use tauri_plugin_android_fs::Entry;

        match entry {
            Entry::File { uri, name, len, mime_type, last_modified, .. } => {
                let full_path = format!("{:?}", uri);

                // Calculate relative path
                let relative_path = if full_path.starts_with(base_path) {
                    full_path[base_path.len()..].trim_start_matches('/').to_string()
                } else {
                    name.clone()
                };

                // Convert SystemTime to formatted string
                let modified_at = last_modified
                    .duration_since(UNIX_EPOCH)
                    .ok()
                    .map(|d| format_unix_timestamp(d.as_secs()));

                // Generate ID from rule_id + relative path (unique per rule, same across devices)
                let id = generate_file_id(rule_id, &relative_path);

                files.push(LocalFileInfo {
                    id,
                    name,
                    path: full_path,
                    relative_path,
                    mime_type: Some(mime_type),
                    size: len,
                    is_directory: false,
                    modified_at,
                });
            }
            Entry::Dir { uri, name, last_modified, .. } => {
                let full_path = format!("{:?}", uri);

                // Calculate relative path
                let relative_path = if full_path.starts_with(base_path) {
                    full_path[base_path.len()..].trim_start_matches('/').to_string()
                } else {
                    name.clone()
                };

                // Convert SystemTime to formatted string
                let modified_at = last_modified
                    .duration_since(UNIX_EPOCH)
                    .ok()
                    .map(|d| format_unix_timestamp(d.as_secs()));

                // Generate ID from rule_id + relative path (unique per rule, same across devices)
                let id = generate_file_id(rule_id, &relative_path);

                files.push(LocalFileInfo {
                    id,
                    name,
                    path: full_path,
                    relative_path,
                    mime_type: None,
                    size: 0,
                    is_directory: true,
                    modified_at,
                });
            }
        }
    }

    // Sort: directories first, then by name
    files.sort_by(|a, b| {
        match (a.is_directory, b.is_directory) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
        }
    });

    Ok(files)
}
