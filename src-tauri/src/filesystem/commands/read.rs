use std::fs;
use std::path::Path;
use std::time::UNIX_EPOCH;
use tauri::State;

use crate::AppState;

use super::{DirEntry, DirListing, FileStat, FsError};

/// Read file contents as base64
#[tauri::command]
pub async fn filesystem_read_file(
    _state: State<'_, AppState>,
    path: String,
    #[allow(unused_variables)] app_handle: tauri::AppHandle,
) -> Result<String, FsError> {
    // Android: handle Content URIs (JSON format from folder picker)
    #[cfg(target_os = "android")]
    if path.starts_with('{') {
        let handle = app_handle.clone();
        return tokio::task::spawn_blocking(move || read_file_android(&handle, &path))
            .await
            .unwrap_or_else(|e| {
                Err(FsError::IoError {
                    reason: e.to_string(),
                })
            });
    }

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

/// Android: Read file via Content URI using android_fs plugin
#[cfg(target_os = "android")]
fn read_file_android(app_handle: &tauri::AppHandle, path_json: &str) -> Result<String, FsError> {
    use tauri_plugin_android_fs::AndroidFsExt;

    let api = app_handle.android_fs();

    let uri = tauri_plugin_android_fs::FileUri::from_json_str(path_json).map_err(|e| {
        FsError::IoError {
            reason: format!("Invalid Content URI: {:?}", e),
        }
    })?;

    let bytes = api.read(&uri).map_err(|e| FsError::IoError {
        reason: format!("Failed to read Android file: {:?}", e),
    })?;

    use base64::{engine::general_purpose::STANDARD, Engine};
    Ok(STANDARD.encode(&bytes))
}

/// Read directory contents with optional pagination.
/// When offset/limit are provided, reads all entries, sorts them, and returns the slice.
/// Returns (entries, total_count) so the frontend knows if there are more.
#[tauri::command]
pub async fn filesystem_read_dir(
    _state: State<'_, AppState>,
    path: String,
    offset: Option<usize>,
    limit: Option<usize>,
    #[allow(unused_variables)] app_handle: tauri::AppHandle,
) -> Result<DirListing, FsError> {
    // Android: handle Content URIs (JSON format from folder picker).
    // Use spawn_blocking because android_fs JNI calls are synchronous and
    // would otherwise block the Tokio executor (especially for large folders).
    #[cfg(target_os = "android")]
    if path.starts_with('{') {
        let handle = app_handle.clone();
        return tokio::task::spawn_blocking(move || {
            read_dir_android(&handle, &path, offset, limit)
        })
        .await
        .unwrap_or_else(|e| {
            Err(FsError::IoError {
                reason: e.to_string(),
            })
        });
    }

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
            size: if metadata.is_file() {
                metadata.len()
            } else {
                0
            },
            modified,
        });
    }

    // Sort: directories first, then files, both alphabetically
    entries.sort_by(|a, b| match (a.is_directory, b.is_directory) {
        (true, false) => std::cmp::Ordering::Less,
        (false, true) => std::cmp::Ordering::Greater,
        _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
    });

    let total = entries.len();

    // Apply pagination if requested
    let entries = match (offset, limit) {
        (Some(off), Some(lim)) => entries.into_iter().skip(off).take(lim).collect(),
        (Some(off), None) => entries.into_iter().skip(off).collect(),
        (None, Some(lim)) => entries.into_iter().take(lim).collect(),
        (None, None) => entries,
    };

    Ok(DirListing { entries, total })
}

/// Android: Read directory via Content URI using android_fs plugin
#[cfg(target_os = "android")]
fn read_dir_android(
    app_handle: &tauri::AppHandle,
    path_json: &str,
    offset: Option<usize>,
    limit: Option<usize>,
) -> Result<DirListing, FsError> {
    use tauri_plugin_android_fs::AndroidFsExt;

    let api = app_handle.android_fs();

    // Parse the JSON Content URI
    let uri = tauri_plugin_android_fs::FileUri::from_json_str(path_json).map_err(|e| {
        FsError::IoError {
            reason: format!("Invalid Content URI: {:?}", e),
        }
    })?;

    let dir_entries = api.read_dir(&uri).map_err(|e| FsError::IoError {
        reason: format!("Failed to read Android directory: {:?}", e),
    })?;

    let mut entries: Vec<DirEntry> = dir_entries
        .into_iter()
        .filter_map(|entry: tauri_plugin_android_fs::Entry| {
            let name = entry.name().to_string();
            let is_dir = entry.is_dir();
            let modified = entry
                .last_modified()
                .duration_since(UNIX_EPOCH)
                .ok()
                .map(|d: std::time::Duration| d.as_millis() as u64);
            let size = entry.file_len().unwrap_or(0);
            let uri_json = entry.uri().to_json_string().ok()?;

            Some(DirEntry {
                name,
                path: uri_json,
                is_file: !is_dir,
                is_directory: is_dir,
                size,
                modified,
            })
        })
        .collect();

    entries.sort_by(|a, b| match (a.is_directory, b.is_directory) {
        (true, false) => std::cmp::Ordering::Less,
        (false, true) => std::cmp::Ordering::Greater,
        _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
    });

    let total = entries.len();

    let entries = match (offset, limit) {
        (Some(off), Some(lim)) => entries.into_iter().skip(off).take(lim).collect(),
        (Some(off), None) => entries.into_iter().skip(off).collect(),
        (None, Some(lim)) => entries.into_iter().take(lim).collect(),
        (None, None) => entries,
    };

    Ok(DirListing { entries, total })
}

/// Check if a path exists
#[tauri::command]
pub async fn filesystem_exists(_state: State<'_, AppState>, path: String) -> Result<bool, FsError> {
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

/// Get the display name of a file from its path.
/// Desktop: returns the basename of the path.
/// Android: resolves Content URIs (JSON envelope from android_fs) via ContentResolver.
#[tauri::command]
pub async fn filesystem_get_file_name(
    #[allow(unused_variables)] app_handle: tauri::AppHandle,
    path: String,
) -> Result<String, FsError> {
    #[cfg(target_os = "android")]
    if path.starts_with('{') {
        use tauri_plugin_android_fs::{AndroidFsExt, FileUri};

        let api = app_handle.android_fs();
        let uri = FileUri::from_json_str(&path).map_err(|e| FsError::IoError {
            reason: format!("Invalid Content URI: {:?}", e),
        })?;
        return api.get_name(&uri).map_err(|e| FsError::IoError {
            reason: format!("Failed to read file name from Content URI: {:?}", e),
        });
    }

    Path::new(&path)
        .file_name()
        .and_then(|n| n.to_str())
        .map(|s| s.to_string())
        .ok_or_else(|| FsError::InvalidPath {
            reason: format!("Could not extract file name from '{}'", path),
        })
}
