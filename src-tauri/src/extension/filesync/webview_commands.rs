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
use serde::Serialize;
use tauri::{AppHandle, Emitter, State, WebviewWindow};

/// Event name for permission prompt required
const EVENT_PERMISSION_PROMPT_REQUIRED: &str = "extension:permission-prompt-required";

/// Payload for permission prompt event
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct PermissionPromptPayload {
    extension_id: String,
    extension_name: String,
    resource_type: String,
    action: String,
    target: String,
}

/// Helper to emit permission prompt event and return the error
fn emit_permission_prompt(
    app_handle: &AppHandle,
    error: &FileSyncError,
) {
    if let FileSyncError::PermissionPromptRequired {
        extension_id,
        extension_name,
        resource_type,
        action,
        target,
    } = error
    {
        let payload = PermissionPromptPayload {
            extension_id: extension_id.clone(),
            extension_name: extension_name.clone(),
            resource_type: resource_type.clone(),
            action: action.clone(),
            target: target.clone(),
        };
        let _ = app_handle.emit(EVENT_PERMISSION_PROMPT_REQUIRED, &payload);
    }
}

/// Wrapper to handle permission prompt errors
async fn with_permission_prompt<T, F, Fut>(
    app_handle: &AppHandle,
    f: F,
) -> Result<T, FileSyncError>
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = Result<T, FileSyncError>>,
{
    let result = f().await;
    if let Err(ref e) = result {
        emit_permission_prompt(app_handle, e);
    }
    result
}

// ============================================================================
// Spaces Commands
// ============================================================================

#[tauri::command]
pub async fn webview_filesync_list_spaces(
    window: WebviewWindow,
    app_handle: AppHandle,
    state: State<'_, AppState>,
) -> Result<Vec<FileSpace>, FileSyncError> {
    let info = get_extension_info_from_window(&window, &state)
        .map_err(|e| FileSyncError::Internal { reason: e.to_string() })?;

    with_permission_prompt(&app_handle, || async {
        super::commands::filesync_list_spaces(state, info.public_key, info.name).await
    }).await
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

    with_permission_prompt(&app_handle, || async {
        super::commands::filesync_create_space(app_handle.clone(), state, info.public_key, info.name, request).await
    }).await
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

    with_permission_prompt(&app_handle, || async {
        super::commands::filesync_delete_space(app_handle.clone(), state, info.public_key, info.name, space_id).await
    }).await
}

// ============================================================================
// Files Commands
// ============================================================================

#[tauri::command]
pub async fn webview_filesync_list_files(
    window: WebviewWindow,
    app_handle: AppHandle,
    state: State<'_, AppState>,
    request: ListFilesRequest,
) -> Result<Vec<FileInfo>, FileSyncError> {
    let info = get_extension_info_from_window(&window, &state)
        .map_err(|e| FileSyncError::Internal { reason: e.to_string() })?;

    with_permission_prompt(&app_handle, || async {
        super::commands::filesync_list_files(state, info.public_key, info.name, request).await
    }).await
}

#[tauri::command]
pub async fn webview_filesync_get_file(
    window: WebviewWindow,
    app_handle: AppHandle,
    state: State<'_, AppState>,
    file_id: String,
) -> Result<FileInfo, FileSyncError> {
    let info = get_extension_info_from_window(&window, &state)
        .map_err(|e| FileSyncError::Internal { reason: e.to_string() })?;

    with_permission_prompt(&app_handle, || async {
        super::commands::filesync_get_file(state, info.public_key, info.name, file_id).await
    }).await
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

    with_permission_prompt(&app_handle, || async {
        super::commands::filesync_upload_file(app_handle.clone(), state, info.public_key, info.name, request).await
    }).await
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

    with_permission_prompt(&app_handle, || async {
        super::commands::filesync_download_file(app_handle.clone(), state, info.public_key, info.name, request).await
    }).await
}

#[tauri::command]
pub async fn webview_filesync_delete_file(
    window: WebviewWindow,
    app_handle: AppHandle,
    state: State<'_, AppState>,
    file_id: String,
) -> Result<(), FileSyncError> {
    let info = get_extension_info_from_window(&window, &state)
        .map_err(|e| FileSyncError::Internal { reason: e.to_string() })?;

    with_permission_prompt(&app_handle, || async {
        super::commands::filesync_delete_file(state, info.public_key, info.name, file_id).await
    }).await
}

// ============================================================================
// Backends Commands
// ============================================================================

#[tauri::command]
pub async fn webview_filesync_list_backends(
    window: WebviewWindow,
    app_handle: AppHandle,
    state: State<'_, AppState>,
) -> Result<Vec<StorageBackendInfo>, FileSyncError> {
    let info = get_extension_info_from_window(&window, &state)
        .map_err(|e| FileSyncError::Internal { reason: e.to_string() })?;

    with_permission_prompt(&app_handle, || async {
        super::commands::filesync_list_backends(state, info.public_key, info.name).await
    }).await
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

    with_permission_prompt(&app_handle, || async {
        super::commands::filesync_add_backend(app_handle.clone(), state, info.public_key, info.name, request).await
    }).await
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

    with_permission_prompt(&app_handle, || async {
        super::commands::filesync_remove_backend(app_handle.clone(), state, info.public_key, info.name, backend_id).await
    }).await
}

#[tauri::command]
pub async fn webview_filesync_test_backend(
    window: WebviewWindow,
    app_handle: AppHandle,
    state: State<'_, AppState>,
    backend_id: String,
) -> Result<(), FileSyncError> {
    let info = get_extension_info_from_window(&window, &state)
        .map_err(|e| FileSyncError::Internal { reason: e.to_string() })?;

    with_permission_prompt(&app_handle, || async {
        super::commands::filesync_test_backend(state, info.public_key, info.name, backend_id).await
    }).await
}

// ============================================================================
// Sync Rules Commands
// ============================================================================

#[tauri::command]
pub async fn webview_filesync_list_sync_rules(
    window: WebviewWindow,
    app_handle: AppHandle,
    state: State<'_, AppState>,
) -> Result<Vec<SyncRule>, FileSyncError> {
    let info = get_extension_info_from_window(&window, &state)
        .map_err(|e| FileSyncError::Internal { reason: e.to_string() })?;

    with_permission_prompt(&app_handle, || async {
        super::commands::filesync_list_sync_rules(state, info.public_key, info.name).await
    }).await
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

    with_permission_prompt(&app_handle, || async {
        super::commands::filesync_add_sync_rule(app_handle.clone(), state, info.public_key, info.name, request).await
    }).await
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

    with_permission_prompt(&app_handle, || async {
        super::commands::filesync_update_sync_rule(app_handle.clone(), state, info.public_key, info.name, request).await
    }).await
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

    with_permission_prompt(&app_handle, || async {
        super::commands::filesync_remove_sync_rule(app_handle.clone(), state, info.public_key, info.name, rule_id).await
    }).await
}

// ============================================================================
// Sync Operations Commands
// ============================================================================

#[tauri::command]
pub async fn webview_filesync_get_sync_status(
    window: WebviewWindow,
    app_handle: AppHandle,
    state: State<'_, AppState>,
) -> Result<SyncStatus, FileSyncError> {
    let info = get_extension_info_from_window(&window, &state)
        .map_err(|e| FileSyncError::Internal { reason: e.to_string() })?;

    with_permission_prompt(&app_handle, || async {
        super::commands::filesync_get_sync_status(state, info.public_key, info.name).await
    }).await
}

#[tauri::command]
pub async fn webview_filesync_trigger_sync(
    window: WebviewWindow,
    app_handle: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), FileSyncError> {
    let info = get_extension_info_from_window(&window, &state)
        .map_err(|e| FileSyncError::Internal { reason: e.to_string() })?;

    with_permission_prompt(&app_handle, || async {
        super::commands::filesync_trigger_sync(state, info.public_key, info.name).await
    }).await
}

#[tauri::command]
pub async fn webview_filesync_pause_sync(
    window: WebviewWindow,
    app_handle: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), FileSyncError> {
    let info = get_extension_info_from_window(&window, &state)
        .map_err(|e| FileSyncError::Internal { reason: e.to_string() })?;

    with_permission_prompt(&app_handle, || async {
        super::commands::filesync_pause_sync(state, info.public_key, info.name).await
    }).await
}

#[tauri::command]
pub async fn webview_filesync_resume_sync(
    window: WebviewWindow,
    app_handle: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), FileSyncError> {
    let info = get_extension_info_from_window(&window, &state)
        .map_err(|e| FileSyncError::Internal { reason: e.to_string() })?;

    with_permission_prompt(&app_handle, || async {
        super::commands::filesync_resume_sync(state, info.public_key, info.name).await
    }).await
}

// ============================================================================
// Conflict Resolution Commands
// ============================================================================

#[tauri::command]
pub async fn webview_filesync_resolve_conflict(
    window: WebviewWindow,
    app_handle: AppHandle,
    state: State<'_, AppState>,
    request: ResolveConflictRequest,
) -> Result<(), FileSyncError> {
    let info = get_extension_info_from_window(&window, &state)
        .map_err(|e| FileSyncError::Internal { reason: e.to_string() })?;

    with_permission_prompt(&app_handle, || async {
        super::commands::filesync_resolve_conflict(state, info.public_key, info.name, request).await
    }).await
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

    with_permission_prompt(&app_handle, || async {
        super::commands::filesync_scan_local(app_handle.clone(), state, info.public_key, info.name, request).await
    }).await
}
