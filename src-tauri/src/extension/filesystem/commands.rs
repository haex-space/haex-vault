// src-tauri/src/extension/filesystem/commands.rs
//!
//! Extension Filesystem Commands
//!
//! Permission-checked wrappers around the internal filesystem API.
//! Extensions must have `fs` permission with appropriate path targets to access the filesystem.
//!

use crate::extension::error::ExtensionError;
use crate::extension::permissions::manager::PermissionManager;
use crate::extension::permissions::types::{Action, FsAction};
use crate::extension::utils::get_extension_id;
use crate::filesystem::{DirEntry, FileStat};
use crate::AppState;
use std::path::Path;
use tauri::State;

// ============================================================================
// Read Operations (require fs:read permission)
// ============================================================================

/// Read file contents as base64 (requires fs:read permission for path)
#[tauri::command(rename_all = "camelCase")]
pub async fn extension_filesystem_read_file(
    public_key: String,
    name: String,
    path: String,
    state: State<'_, AppState>,
) -> Result<String, ExtensionError> {
    let extension_id = get_extension_id(&state, &public_key, &name).await?;

    // Check fs permission for this path (read)
    PermissionManager::check_filesystem_permission(
        &state,
        &extension_id,
        Action::Filesystem(FsAction::Read),
        Path::new(&path),
    )
    .await?;

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
    public_key: String,
    name: String,
    path: String,
    state: State<'_, AppState>,
) -> Result<Vec<DirEntry>, ExtensionError> {
    let extension_id = get_extension_id(&state, &public_key, &name).await?;

    // Check fs permission for this path (read)
    PermissionManager::check_filesystem_permission(
        &state,
        &extension_id,
        Action::Filesystem(FsAction::Read),
        Path::new(&path),
    )
    .await?;

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
    public_key: String,
    name: String,
    path: String,
    state: State<'_, AppState>,
) -> Result<bool, ExtensionError> {
    let extension_id = get_extension_id(&state, &public_key, &name).await?;

    // Check fs permission for this path (read)
    PermissionManager::check_filesystem_permission(
        &state,
        &extension_id,
        Action::Filesystem(FsAction::Read),
        Path::new(&path),
    )
    .await?;

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
    public_key: String,
    name: String,
    path: String,
    state: State<'_, AppState>,
) -> Result<FileStat, ExtensionError> {
    let extension_id = get_extension_id(&state, &public_key, &name).await?;

    // Check fs permission for this path (read)
    PermissionManager::check_filesystem_permission(
        &state,
        &extension_id,
        Action::Filesystem(FsAction::Read),
        Path::new(&path),
    )
    .await?;

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
    public_key: String,
    name: String,
    path: String,
    data: String,
    state: State<'_, AppState>,
) -> Result<(), ExtensionError> {
    let extension_id = get_extension_id(&state, &public_key, &name).await?;

    // Check fs permission for this path (write)
    PermissionManager::check_filesystem_permission(
        &state,
        &extension_id,
        Action::Filesystem(FsAction::ReadWrite),
        Path::new(&path),
    )
    .await?;

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
    public_key: String,
    name: String,
    path: String,
    state: State<'_, AppState>,
) -> Result<(), ExtensionError> {
    let extension_id = get_extension_id(&state, &public_key, &name).await?;

    // Check fs permission for this path (write)
    PermissionManager::check_filesystem_permission(
        &state,
        &extension_id,
        Action::Filesystem(FsAction::ReadWrite),
        Path::new(&path),
    )
    .await?;

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
    public_key: String,
    name: String,
    path: String,
    recursive: Option<bool>,
    state: State<'_, AppState>,
) -> Result<(), ExtensionError> {
    let extension_id = get_extension_id(&state, &public_key, &name).await?;

    // Check fs permission for this path (write)
    PermissionManager::check_filesystem_permission(
        &state,
        &extension_id,
        Action::Filesystem(FsAction::ReadWrite),
        Path::new(&path),
    )
    .await?;

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
    public_key: String,
    name: String,
    from: String,
    to: String,
    state: State<'_, AppState>,
) -> Result<(), ExtensionError> {
    let extension_id = get_extension_id(&state, &public_key, &name).await?;

    // Check fs permission for source path (write - we're removing from here)
    PermissionManager::check_filesystem_permission(
        &state,
        &extension_id,
        Action::Filesystem(FsAction::ReadWrite),
        Path::new(&from),
    )
    .await?;

    // Check fs permission for destination path (write - we're creating here)
    PermissionManager::check_filesystem_permission(
        &state,
        &extension_id,
        Action::Filesystem(FsAction::ReadWrite),
        Path::new(&to),
    )
    .await?;

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
    public_key: String,
    name: String,
    from: String,
    to: String,
    state: State<'_, AppState>,
) -> Result<(), ExtensionError> {
    let extension_id = get_extension_id(&state, &public_key, &name).await?;

    // Check fs permission for source path (read)
    PermissionManager::check_filesystem_permission(
        &state,
        &extension_id,
        Action::Filesystem(FsAction::Read),
        Path::new(&from),
    )
    .await?;

    // Check fs permission for destination path (write)
    PermissionManager::check_filesystem_permission(
        &state,
        &extension_id,
        Action::Filesystem(FsAction::ReadWrite),
        Path::new(&to),
    )
    .await?;

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
    public_key: String,
    name: String,
    title: Option<String>,
    default_path: Option<String>,
    window: tauri::WebviewWindow,
    state: State<'_, AppState>,
    app_handle: tauri::AppHandle,
) -> Result<Option<String>, ExtensionError> {
    // Verify extension exists
    let _extension_id = get_extension_id(&state, &public_key, &name).await?;

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
    public_key: String,
    name: String,
    title: Option<String>,
    default_path: Option<String>,
    filters: Option<Vec<(String, Vec<String>)>>,
    multiple: Option<bool>,
    window: tauri::WebviewWindow,
    state: State<'_, AppState>,
    app_handle: tauri::AppHandle,
) -> Result<Option<Vec<String>>, ExtensionError> {
    // Verify extension exists
    let _extension_id = get_extension_id(&state, &public_key, &name).await?;

    // Delegate to internal filesystem command (no permission check - user explicitly selects)
    crate::filesystem::filesystem_select_file(window, title, default_path, filters, multiple, app_handle)
        .await
        .map_err(|e| ExtensionError::FilesystemError {
            reason: e.to_string(),
        })
}
