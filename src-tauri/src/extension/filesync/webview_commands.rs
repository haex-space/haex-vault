// src-tauri/src/extension/filesync/webview_commands.rs
//!
//! WebView Commands for FileSync
//!
//! These commands are for native WebView windows (not iframes).
//! They extract extension info from the WebviewWindow context,
//! which is secure because Tauri provides the window reference
//! and cannot be spoofed by extensions.
//!

use super::error::FileSyncError;
use super::types::*;
use crate::extension::webview::helpers::get_extension_info_from_window;
use crate::AppState;
use tauri::{AppHandle, State, WebviewWindow};

// ============================================================================
// Spaces Commands
// ============================================================================

#[tauri::command]
pub async fn webview_filesync_list_spaces(
    window: WebviewWindow,
    state: State<'_, AppState>,
) -> Result<Vec<FileSpace>, FileSyncError> {
    let info = get_extension_info_from_window(&window, &state)
        .map_err(|e| FileSyncError::Internal { reason: e.to_string() })?;

    super::commands::filesync_list_spaces(state, info.public_key, info.name).await
}

#[tauri::command]
pub async fn webview_filesync_create_space(
    window: WebviewWindow,
    app_handle: AppHandle,
    state: State<'_, AppState>,
    request: CreateSpaceRequest,
) -> Result<FileSpace, FileSyncError> {
    let info = get_extension_info_from_window(&window, &state)
        .map_err(|e| FileSyncError::Internal { reason: e.to_string() })?;

    super::commands::filesync_create_space(app_handle, state, info.public_key, info.name, request).await
}

#[tauri::command]
pub async fn webview_filesync_delete_space(
    window: WebviewWindow,
    app_handle: AppHandle,
    state: State<'_, AppState>,
    space_id: String,
) -> Result<(), FileSyncError> {
    let info = get_extension_info_from_window(&window, &state)
        .map_err(|e| FileSyncError::Internal { reason: e.to_string() })?;

    super::commands::filesync_delete_space(app_handle, state, info.public_key, info.name, space_id).await
}

// ============================================================================
// Files Commands
// ============================================================================

#[tauri::command]
pub async fn webview_filesync_list_files(
    window: WebviewWindow,
    state: State<'_, AppState>,
    request: ListFilesRequest,
) -> Result<Vec<FileInfo>, FileSyncError> {
    let info = get_extension_info_from_window(&window, &state)
        .map_err(|e| FileSyncError::Internal { reason: e.to_string() })?;

    super::commands::filesync_list_files(state, info.public_key, info.name, request).await
}

#[tauri::command]
pub async fn webview_filesync_get_file(
    window: WebviewWindow,
    state: State<'_, AppState>,
    file_id: String,
) -> Result<FileInfo, FileSyncError> {
    let info = get_extension_info_from_window(&window, &state)
        .map_err(|e| FileSyncError::Internal { reason: e.to_string() })?;

    super::commands::filesync_get_file(state, info.public_key, info.name, file_id).await
}

#[tauri::command]
pub async fn webview_filesync_upload_file(
    window: WebviewWindow,
    app_handle: AppHandle,
    state: State<'_, AppState>,
    request: UploadFileRequest,
) -> Result<FileInfo, FileSyncError> {
    let info = get_extension_info_from_window(&window, &state)
        .map_err(|e| FileSyncError::Internal { reason: e.to_string() })?;

    super::commands::filesync_upload_file(app_handle, state, info.public_key, info.name, request).await
}

#[tauri::command]
pub async fn webview_filesync_download_file(
    window: WebviewWindow,
    app_handle: AppHandle,
    state: State<'_, AppState>,
    request: DownloadFileRequest,
) -> Result<(), FileSyncError> {
    let info = get_extension_info_from_window(&window, &state)
        .map_err(|e| FileSyncError::Internal { reason: e.to_string() })?;

    super::commands::filesync_download_file(app_handle, state, info.public_key, info.name, request).await
}

#[tauri::command]
pub async fn webview_filesync_delete_file(
    window: WebviewWindow,
    state: State<'_, AppState>,
    file_id: String,
) -> Result<(), FileSyncError> {
    let info = get_extension_info_from_window(&window, &state)
        .map_err(|e| FileSyncError::Internal { reason: e.to_string() })?;

    super::commands::filesync_delete_file(state, info.public_key, info.name, file_id).await
}

// ============================================================================
// Backends Commands
// ============================================================================

#[tauri::command]
pub async fn webview_filesync_list_backends(
    window: WebviewWindow,
    state: State<'_, AppState>,
) -> Result<Vec<StorageBackendInfo>, FileSyncError> {
    let info = get_extension_info_from_window(&window, &state)
        .map_err(|e| FileSyncError::Internal { reason: e.to_string() })?;

    super::commands::filesync_list_backends(state, info.public_key, info.name).await
}

#[tauri::command]
pub async fn webview_filesync_add_backend(
    window: WebviewWindow,
    app_handle: AppHandle,
    state: State<'_, AppState>,
    request: AddBackendRequest,
) -> Result<StorageBackendInfo, FileSyncError> {
    let info = get_extension_info_from_window(&window, &state)
        .map_err(|e| FileSyncError::Internal { reason: e.to_string() })?;

    super::commands::filesync_add_backend(app_handle, state, info.public_key, info.name, request).await
}

#[tauri::command]
pub async fn webview_filesync_remove_backend(
    window: WebviewWindow,
    app_handle: AppHandle,
    state: State<'_, AppState>,
    backend_id: String,
) -> Result<(), FileSyncError> {
    let info = get_extension_info_from_window(&window, &state)
        .map_err(|e| FileSyncError::Internal { reason: e.to_string() })?;

    super::commands::filesync_remove_backend(app_handle, state, info.public_key, info.name, backend_id).await
}

#[tauri::command]
pub async fn webview_filesync_test_backend(
    window: WebviewWindow,
    state: State<'_, AppState>,
    backend_id: String,
) -> Result<(), FileSyncError> {
    let info = get_extension_info_from_window(&window, &state)
        .map_err(|e| FileSyncError::Internal { reason: e.to_string() })?;

    super::commands::filesync_test_backend(state, info.public_key, info.name, backend_id).await
}

// ============================================================================
// Sync Rules Commands
// ============================================================================

#[tauri::command]
pub async fn webview_filesync_list_sync_rules(
    window: WebviewWindow,
    state: State<'_, AppState>,
) -> Result<Vec<SyncRule>, FileSyncError> {
    let info = get_extension_info_from_window(&window, &state)
        .map_err(|e| FileSyncError::Internal { reason: e.to_string() })?;

    super::commands::filesync_list_sync_rules(state, info.public_key, info.name).await
}

#[tauri::command]
pub async fn webview_filesync_add_sync_rule(
    window: WebviewWindow,
    app_handle: AppHandle,
    state: State<'_, AppState>,
    request: AddSyncRuleRequest,
) -> Result<SyncRule, FileSyncError> {
    let info = get_extension_info_from_window(&window, &state)
        .map_err(|e| FileSyncError::Internal { reason: e.to_string() })?;

    super::commands::filesync_add_sync_rule(app_handle, state, info.public_key, info.name, request).await
}

#[tauri::command]
pub async fn webview_filesync_update_sync_rule(
    window: WebviewWindow,
    app_handle: AppHandle,
    state: State<'_, AppState>,
    request: UpdateSyncRuleRequest,
) -> Result<SyncRule, FileSyncError> {
    let info = get_extension_info_from_window(&window, &state)
        .map_err(|e| FileSyncError::Internal { reason: e.to_string() })?;

    super::commands::filesync_update_sync_rule(app_handle, state, info.public_key, info.name, request).await
}

#[tauri::command]
pub async fn webview_filesync_remove_sync_rule(
    window: WebviewWindow,
    app_handle: AppHandle,
    state: State<'_, AppState>,
    rule_id: String,
) -> Result<(), FileSyncError> {
    let info = get_extension_info_from_window(&window, &state)
        .map_err(|e| FileSyncError::Internal { reason: e.to_string() })?;

    super::commands::filesync_remove_sync_rule(app_handle, state, info.public_key, info.name, rule_id).await
}

// ============================================================================
// Sync Operations Commands
// ============================================================================

#[tauri::command]
pub async fn webview_filesync_get_sync_status(
    window: WebviewWindow,
    state: State<'_, AppState>,
) -> Result<SyncStatus, FileSyncError> {
    let info = get_extension_info_from_window(&window, &state)
        .map_err(|e| FileSyncError::Internal { reason: e.to_string() })?;

    super::commands::filesync_get_sync_status(state, info.public_key, info.name).await
}

#[tauri::command]
pub async fn webview_filesync_trigger_sync(
    window: WebviewWindow,
    state: State<'_, AppState>,
) -> Result<(), FileSyncError> {
    let info = get_extension_info_from_window(&window, &state)
        .map_err(|e| FileSyncError::Internal { reason: e.to_string() })?;

    super::commands::filesync_trigger_sync(state, info.public_key, info.name).await
}

#[tauri::command]
pub async fn webview_filesync_pause_sync(
    window: WebviewWindow,
    state: State<'_, AppState>,
) -> Result<(), FileSyncError> {
    let info = get_extension_info_from_window(&window, &state)
        .map_err(|e| FileSyncError::Internal { reason: e.to_string() })?;

    super::commands::filesync_pause_sync(state, info.public_key, info.name).await
}

#[tauri::command]
pub async fn webview_filesync_resume_sync(
    window: WebviewWindow,
    state: State<'_, AppState>,
) -> Result<(), FileSyncError> {
    let info = get_extension_info_from_window(&window, &state)
        .map_err(|e| FileSyncError::Internal { reason: e.to_string() })?;

    super::commands::filesync_resume_sync(state, info.public_key, info.name).await
}

// ============================================================================
// Conflict Resolution Commands
// ============================================================================

#[tauri::command]
pub async fn webview_filesync_resolve_conflict(
    window: WebviewWindow,
    state: State<'_, AppState>,
    request: ResolveConflictRequest,
) -> Result<(), FileSyncError> {
    let info = get_extension_info_from_window(&window, &state)
        .map_err(|e| FileSyncError::Internal { reason: e.to_string() })?;

    super::commands::filesync_resolve_conflict(state, info.public_key, info.name, request).await
}

// ============================================================================
// Local Directory Scanning Commands
// ============================================================================

#[tauri::command]
pub async fn webview_filesync_scan_local(
    window: WebviewWindow,
    app_handle: AppHandle,
    state: State<'_, AppState>,
    request: ScanLocalRequest,
) -> Result<Vec<LocalFileInfo>, FileSyncError> {
    let info = get_extension_info_from_window(&window, &state)
        .map_err(|e| FileSyncError::Internal { reason: e.to_string() })?;

    super::commands::filesync_scan_local(app_handle, state, info.public_key, info.name, request).await
}
