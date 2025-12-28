// src-tauri/src/extension/filesystem/commands.rs
//!
//! Extension Filesystem Commands
//!
//! Permission-checked wrappers around the internal filesystem API.
//! Extensions must have `fs` permission with appropriate path targets to access the filesystem.
//!
//! These commands work for both WebView and iframe extensions:
//! - WebView: extension_id is resolved from the window context
//! - iframe: extension_id is resolved from public_key/name parameters
//!           (verified by frontend via origin check)

use crate::extension::error::ExtensionError;
use crate::extension::permissions::manager::PermissionManager;
use crate::extension::permissions::types::{Action, FsAction};
use crate::extension::utils::resolve_extension_id;
use crate::extension::webview::helpers::emit_permission_prompt_if_needed;
use crate::filesystem::{DirEntry, FileStat};
use crate::AppState;
use std::path::Path;
use tauri::{AppHandle, State, WebviewWindow};

// ============================================================================
// Read Operations (require fs:read permission)
// ============================================================================

/// Read file contents as base64 (requires fs:read permission for path)
#[tauri::command(rename_all = "camelCase")]
pub async fn extension_filesystem_read_file(
    app_handle: AppHandle,
    window: WebviewWindow,
    state: State<'_, AppState>,
    path: String,
    // Optional parameters for iframe mode (verified by frontend via origin)
    public_key: Option<String>,
    name: Option<String>,
) -> Result<String, ExtensionError> {
    let extension_id = resolve_extension_id(&window, &state, public_key, name)?;

    // Check fs permission for this path (read)
    let permission_result = PermissionManager::check_filesystem_permission(
        &state,
        &extension_id,
        Action::Filesystem(FsAction::Read),
        Path::new(&path),
    )
    .await;

    if let Err(ref e) = permission_result {
        emit_permission_prompt_if_needed(&app_handle, e);
    }
    permission_result?;

    // Delegate to internal filesystem command
    crate::filesystem::filesystem_read_file(state, path)
        .await
        .map_err(|e| ExtensionError::FilesystemError {
            reason: e.to_string(),
        })
}

/// Read directory contents (requires fs:read permission for path)
#[tauri::command(rename_all = "camelCase")]
pub async fn extension_filesystem_read_dir(
    app_handle: AppHandle,
    window: WebviewWindow,
    state: State<'_, AppState>,
    path: String,
    // Optional parameters for iframe mode (verified by frontend via origin)
    public_key: Option<String>,
    name: Option<String>,
) -> Result<Vec<DirEntry>, ExtensionError> {
    let extension_id = resolve_extension_id(&window, &state, public_key, name)?;

    // Check fs permission for this path (read)
    let permission_result = PermissionManager::check_filesystem_permission(
        &state,
        &extension_id,
        Action::Filesystem(FsAction::Read),
        Path::new(&path),
    )
    .await;

    if let Err(ref e) = permission_result {
        emit_permission_prompt_if_needed(&app_handle, e);
    }
    permission_result?;

    // Delegate to internal filesystem command
    crate::filesystem::filesystem_read_dir(state, path)
        .await
        .map_err(|e| ExtensionError::FilesystemError {
            reason: e.to_string(),
        })
}

/// Check if a path exists (requires fs:read permission for path)
#[tauri::command(rename_all = "camelCase")]
pub async fn extension_filesystem_exists(
    app_handle: AppHandle,
    window: WebviewWindow,
    state: State<'_, AppState>,
    path: String,
    // Optional parameters for iframe mode (verified by frontend via origin)
    public_key: Option<String>,
    name: Option<String>,
) -> Result<bool, ExtensionError> {
    let extension_id = resolve_extension_id(&window, &state, public_key, name)?;

    // Check fs permission for this path (read)
    let permission_result = PermissionManager::check_filesystem_permission(
        &state,
        &extension_id,
        Action::Filesystem(FsAction::Read),
        Path::new(&path),
    )
    .await;

    if let Err(ref e) = permission_result {
        emit_permission_prompt_if_needed(&app_handle, e);
    }
    permission_result?;

    // Delegate to internal filesystem command
    crate::filesystem::filesystem_exists(state, path)
        .await
        .map_err(|e| ExtensionError::FilesystemError {
            reason: e.to_string(),
        })
}

/// Get file/directory metadata (requires fs:read permission for path)
#[tauri::command(rename_all = "camelCase")]
pub async fn extension_filesystem_stat(
    app_handle: AppHandle,
    window: WebviewWindow,
    state: State<'_, AppState>,
    path: String,
    // Optional parameters for iframe mode (verified by frontend via origin)
    public_key: Option<String>,
    name: Option<String>,
) -> Result<FileStat, ExtensionError> {
    let extension_id = resolve_extension_id(&window, &state, public_key, name)?;

    // Check fs permission for this path (read)
    let permission_result = PermissionManager::check_filesystem_permission(
        &state,
        &extension_id,
        Action::Filesystem(FsAction::Read),
        Path::new(&path),
    )
    .await;

    if let Err(ref e) = permission_result {
        emit_permission_prompt_if_needed(&app_handle, e);
    }
    permission_result?;

    // Delegate to internal filesystem command
    crate::filesystem::filesystem_stat(state, path)
        .await
        .map_err(|e| ExtensionError::FilesystemError {
            reason: e.to_string(),
        })
}

// ============================================================================
// Write Operations (require fs:readWrite permission)
// ============================================================================

/// Write file contents from base64 (requires fs:readWrite permission for path)
#[tauri::command(rename_all = "camelCase")]
pub async fn extension_filesystem_write_file(
    app_handle: AppHandle,
    window: WebviewWindow,
    state: State<'_, AppState>,
    path: String,
    data: String,
    // Optional parameters for iframe mode (verified by frontend via origin)
    public_key: Option<String>,
    name: Option<String>,
) -> Result<(), ExtensionError> {
    let extension_id = resolve_extension_id(&window, &state, public_key, name)?;

    // Check fs permission for this path (write)
    let permission_result = PermissionManager::check_filesystem_permission(
        &state,
        &extension_id,
        Action::Filesystem(FsAction::ReadWrite),
        Path::new(&path),
    )
    .await;

    if let Err(ref e) = permission_result {
        emit_permission_prompt_if_needed(&app_handle, e);
    }
    permission_result?;

    // Delegate to internal filesystem command
    crate::filesystem::filesystem_write_file(state, path, data)
        .await
        .map_err(|e| ExtensionError::FilesystemError {
            reason: e.to_string(),
        })
}

/// Create a directory (requires fs:readWrite permission for path)
#[tauri::command(rename_all = "camelCase")]
pub async fn extension_filesystem_mkdir(
    app_handle: AppHandle,
    window: WebviewWindow,
    state: State<'_, AppState>,
    path: String,
    // Optional parameters for iframe mode (verified by frontend via origin)
    public_key: Option<String>,
    name: Option<String>,
) -> Result<(), ExtensionError> {
    let extension_id = resolve_extension_id(&window, &state, public_key, name)?;

    // Check fs permission for this path (write)
    let permission_result = PermissionManager::check_filesystem_permission(
        &state,
        &extension_id,
        Action::Filesystem(FsAction::ReadWrite),
        Path::new(&path),
    )
    .await;

    if let Err(ref e) = permission_result {
        emit_permission_prompt_if_needed(&app_handle, e);
    }
    permission_result?;

    // Delegate to internal filesystem command
    crate::filesystem::filesystem_mkdir(state, path)
        .await
        .map_err(|e| ExtensionError::FilesystemError {
            reason: e.to_string(),
        })
}

/// Remove a file or directory (requires fs:readWrite permission for path)
#[tauri::command(rename_all = "camelCase")]
pub async fn extension_filesystem_remove(
    app_handle: AppHandle,
    window: WebviewWindow,
    state: State<'_, AppState>,
    path: String,
    recursive: Option<bool>,
    // Optional parameters for iframe mode (verified by frontend via origin)
    public_key: Option<String>,
    name: Option<String>,
) -> Result<(), ExtensionError> {
    let extension_id = resolve_extension_id(&window, &state, public_key, name)?;

    // Check fs permission for this path (write)
    let permission_result = PermissionManager::check_filesystem_permission(
        &state,
        &extension_id,
        Action::Filesystem(FsAction::ReadWrite),
        Path::new(&path),
    )
    .await;

    if let Err(ref e) = permission_result {
        emit_permission_prompt_if_needed(&app_handle, e);
    }
    permission_result?;

    // Delegate to internal filesystem command
    crate::filesystem::filesystem_remove(state, path, recursive)
        .await
        .map_err(|e| ExtensionError::FilesystemError {
            reason: e.to_string(),
        })
}

/// Rename/move a file or directory (requires fs:readWrite permission for both paths)
#[tauri::command(rename_all = "camelCase")]
pub async fn extension_filesystem_rename(
    app_handle: AppHandle,
    window: WebviewWindow,
    state: State<'_, AppState>,
    from: String,
    to: String,
    // Optional parameters for iframe mode (verified by frontend via origin)
    public_key: Option<String>,
    name: Option<String>,
) -> Result<(), ExtensionError> {
    let extension_id = resolve_extension_id(&window, &state, public_key, name)?;

    // Check fs permission for source path (write - we're removing from here)
    let permission_result = PermissionManager::check_filesystem_permission(
        &state,
        &extension_id,
        Action::Filesystem(FsAction::ReadWrite),
        Path::new(&from),
    )
    .await;

    if let Err(ref e) = permission_result {
        emit_permission_prompt_if_needed(&app_handle, e);
    }
    permission_result?;

    // Check fs permission for destination path (write - we're creating here)
    let permission_result = PermissionManager::check_filesystem_permission(
        &state,
        &extension_id,
        Action::Filesystem(FsAction::ReadWrite),
        Path::new(&to),
    )
    .await;

    if let Err(ref e) = permission_result {
        emit_permission_prompt_if_needed(&app_handle, e);
    }
    permission_result?;

    // Delegate to internal filesystem command
    crate::filesystem::filesystem_rename(state, from, to)
        .await
        .map_err(|e| ExtensionError::FilesystemError {
            reason: e.to_string(),
        })
}

/// Copy a file (requires fs:read for source, fs:readWrite for destination)
#[tauri::command(rename_all = "camelCase")]
pub async fn extension_filesystem_copy(
    app_handle: AppHandle,
    window: WebviewWindow,
    state: State<'_, AppState>,
    from: String,
    to: String,
    // Optional parameters for iframe mode (verified by frontend via origin)
    public_key: Option<String>,
    name: Option<String>,
) -> Result<(), ExtensionError> {
    let extension_id = resolve_extension_id(&window, &state, public_key, name)?;

    // Check fs permission for source path (read)
    let permission_result = PermissionManager::check_filesystem_permission(
        &state,
        &extension_id,
        Action::Filesystem(FsAction::Read),
        Path::new(&from),
    )
    .await;

    if let Err(ref e) = permission_result {
        emit_permission_prompt_if_needed(&app_handle, e);
    }
    permission_result?;

    // Check fs permission for destination path (write)
    let permission_result = PermissionManager::check_filesystem_permission(
        &state,
        &extension_id,
        Action::Filesystem(FsAction::ReadWrite),
        Path::new(&to),
    )
    .await;

    if let Err(ref e) = permission_result {
        emit_permission_prompt_if_needed(&app_handle, e);
    }
    permission_result?;

    // Delegate to internal filesystem command
    crate::filesystem::filesystem_copy(state, from, to)
        .await
        .map_err(|e| ExtensionError::FilesystemError {
            reason: e.to_string(),
        })
}

// ============================================================================
// Dialog Operations (no path permission needed, user selects interactively)
// ============================================================================

/// Open a folder selection dialog
/// Note: No permission check needed as user explicitly selects the folder
#[tauri::command(rename_all = "camelCase")]
pub async fn extension_filesystem_select_folder(
    window: WebviewWindow,
    state: State<'_, AppState>,
    app_handle: AppHandle,
    title: Option<String>,
    default_path: Option<String>,
    // Optional parameters for iframe mode (verified by frontend via origin)
    public_key: Option<String>,
    name: Option<String>,
) -> Result<Option<String>, ExtensionError> {
    // Verify extension exists
    let _extension_id = resolve_extension_id(&window, &state, public_key, name)?;

    // Delegate to internal filesystem command (no permission check - user explicitly selects)
    crate::filesystem::filesystem_select_folder(window, title, default_path, app_handle)
        .await
        .map_err(|e| ExtensionError::FilesystemError {
            reason: e.to_string(),
        })
}

/// Open a file selection dialog
/// Note: No permission check needed as user explicitly selects the file
#[tauri::command(rename_all = "camelCase")]
pub async fn extension_filesystem_select_file(
    window: WebviewWindow,
    state: State<'_, AppState>,
    app_handle: AppHandle,
    title: Option<String>,
    default_path: Option<String>,
    filters: Option<Vec<(String, Vec<String>)>>,
    multiple: Option<bool>,
    // Optional parameters for iframe mode (verified by frontend via origin)
    public_key: Option<String>,
    name: Option<String>,
) -> Result<Option<Vec<String>>, ExtensionError> {
    // Verify extension exists
    let _extension_id = resolve_extension_id(&window, &state, public_key, name)?;

    // Delegate to internal filesystem command (no permission check - user explicitly selects)
    crate::filesystem::filesystem_select_file(window, title, default_path, filters, multiple, app_handle)
        .await
        .map_err(|e| ExtensionError::FilesystemError {
            reason: e.to_string(),
        })
}

// ============================================================================
// File Watcher Operations (require fs:read permission)
// ============================================================================

/// Start watching a directory for changes (requires fs:read permission)
/// Emits "filesync:file-changed" events when files change
#[tauri::command(rename_all = "camelCase")]
pub async fn extension_filesystem_watch(
    app_handle: AppHandle,
    window: WebviewWindow,
    state: State<'_, AppState>,
    rule_id: String,
    path: String,
    // Optional parameters for iframe mode (verified by frontend via origin)
    public_key: Option<String>,
    name: Option<String>,
) -> Result<(), ExtensionError> {
    let extension_id = resolve_extension_id(&window, &state, public_key, name)?;

    // Check fs permission for this path (read - we're watching for changes)
    let permission_result = PermissionManager::check_filesystem_permission(
        &state,
        &extension_id,
        Action::Filesystem(FsAction::Read),
        Path::new(&path),
    )
    .await;

    if let Err(ref e) = permission_result {
        emit_permission_prompt_if_needed(&app_handle, e);
    }
    permission_result?;

    // Start watching the directory
    #[cfg(desktop)]
    state
        .file_watcher
        .watch(app_handle, rule_id, path)
        .map_err(|e| ExtensionError::FilesystemError { reason: e })?;

    Ok(())
}

/// Stop watching a directory
#[tauri::command(rename_all = "camelCase")]
pub async fn extension_filesystem_unwatch(
    window: WebviewWindow,
    state: State<'_, AppState>,
    rule_id: String,
    // Optional parameters for iframe mode (verified by frontend via origin)
    public_key: Option<String>,
    name: Option<String>,
) -> Result<(), ExtensionError> {
    // Verify extension exists
    let _extension_id = resolve_extension_id(&window, &state, public_key, name)?;

    // Stop watching
    #[cfg(desktop)]
    state
        .file_watcher
        .unwatch(&rule_id)
        .map_err(|e| ExtensionError::FilesystemError { reason: e })?;

    Ok(())
}

/// Check if a directory is being watched
#[tauri::command(rename_all = "camelCase")]
pub async fn extension_filesystem_is_watching(
    window: WebviewWindow,
    state: State<'_, AppState>,
    rule_id: String,
    // Optional parameters for iframe mode (verified by frontend via origin)
    public_key: Option<String>,
    name: Option<String>,
) -> Result<bool, ExtensionError> {
    // Verify extension exists
    let _extension_id = resolve_extension_id(&window, &state, public_key, name)?;

    #[cfg(desktop)]
    {
        Ok(state.file_watcher.is_watching(&rule_id))
    }

    #[cfg(not(desktop))]
    {
        Ok(false)
    }
}
