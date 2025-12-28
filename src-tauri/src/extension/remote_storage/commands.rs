// src-tauri/src/extension/remote_storage/commands.rs
//!
//! Extension Remote Storage Commands
//!
//! Permission-checked wrappers around the internal storage API.
//! Extensions must have `filesync` permission with `backends` target to access storage backends.
//!

use crate::extension::error::ExtensionError;
use crate::extension::permissions::manager::PermissionManager;
use crate::extension::permissions::types::{FileSyncAction, FileSyncTarget};
use crate::extension::utils::get_extension_id_by_key_and_name;
use crate::extension::webview::helpers::emit_permission_prompt_if_needed;
use crate::remote_storage::types::{
    AddStorageBackendRequest, StorageBackendInfo, StorageDeleteRequest, StorageDownloadRequest,
    StorageListRequest, StorageObjectInfo, StorageUploadRequest, UpdateStorageBackendRequest,
};
use crate::remote_storage;
use crate::AppState;
use tauri::{AppHandle, State};

// ============================================================================
// Backend Management Commands (with permission checks)
// ============================================================================

/// List all storage backends (requires filesync:backends:read permission)
#[tauri::command(rename_all = "camelCase")]
pub async fn extension_remote_storage_list_backends(
    app_handle: AppHandle,
    public_key: String,
    name: String,
    state: State<'_, AppState>,
) -> Result<Vec<StorageBackendInfo>, ExtensionError> {
    let extension_id = get_extension_id_by_key_and_name(&state, &public_key, &name)?;

    // Check filesync permission for backends (read)
    let permission_result = PermissionManager::check_filesync_permission(
        &state,
        &extension_id,
        FileSyncAction::Read,
        FileSyncTarget::Backends,
    )
    .await;

    if let Err(ref e) = permission_result {
        emit_permission_prompt_if_needed(&app_handle, e);
    }
    permission_result?;

    // Delegate to internal remote storage command
    remote_storage::remote_storage_list_backends(state)
        .await
        .map_err(|e| ExtensionError::StorageError { source: e })
}

/// Add a new storage backend (requires filesync:backends:readWrite permission)
#[tauri::command(rename_all = "camelCase")]
pub async fn extension_remote_storage_add_backend(
    app_handle: AppHandle,
    public_key: String,
    name: String,
    request: AddStorageBackendRequest,
    state: State<'_, AppState>,
) -> Result<StorageBackendInfo, ExtensionError> {
    let extension_id = get_extension_id_by_key_and_name(&state, &public_key, &name)?;

    // Check filesync permission for backends (write)
    let permission_result = PermissionManager::check_filesync_permission(
        &state,
        &extension_id,
        FileSyncAction::ReadWrite,
        FileSyncTarget::Backends,
    )
    .await;

    if let Err(ref e) = permission_result {
        emit_permission_prompt_if_needed(&app_handle, e);
    }
    permission_result?;

    // Delegate to internal remote storage command
    remote_storage::remote_storage_add_backend(state, request)
        .await
        .map_err(|e| ExtensionError::StorageError { source: e })
}

/// Update a storage backend (requires filesync:backends:readWrite permission)
#[tauri::command(rename_all = "camelCase")]
pub async fn extension_remote_storage_update_backend(
    app_handle: AppHandle,
    public_key: String,
    name: String,
    request: UpdateStorageBackendRequest,
    state: State<'_, AppState>,
) -> Result<StorageBackendInfo, ExtensionError> {
    let extension_id = get_extension_id_by_key_and_name(&state, &public_key, &name)?;

    // Check filesync permission for backends (write)
    let permission_result = PermissionManager::check_filesync_permission(
        &state,
        &extension_id,
        FileSyncAction::ReadWrite,
        FileSyncTarget::Backends,
    )
    .await;

    if let Err(ref e) = permission_result {
        emit_permission_prompt_if_needed(&app_handle, e);
    }
    permission_result?;

    // Delegate to internal remote storage command
    remote_storage::remote_storage_update_backend(state, request)
        .await
        .map_err(|e| ExtensionError::StorageError { source: e })
}

/// Remove a storage backend (requires filesync:backends:readWrite permission)
#[tauri::command(rename_all = "camelCase")]
pub async fn extension_remote_storage_remove_backend(
    app_handle: AppHandle,
    public_key: String,
    name: String,
    backend_id: String,
    state: State<'_, AppState>,
) -> Result<(), ExtensionError> {
    let extension_id = get_extension_id_by_key_and_name(&state, &public_key, &name)?;

    // Check filesync permission for backends (write)
    let permission_result = PermissionManager::check_filesync_permission(
        &state,
        &extension_id,
        FileSyncAction::ReadWrite,
        FileSyncTarget::Backends,
    )
    .await;

    if let Err(ref e) = permission_result {
        emit_permission_prompt_if_needed(&app_handle, e);
    }
    permission_result?;

    // Delegate to internal remote storage command
    remote_storage::remote_storage_remove_backend(state, backend_id)
        .await
        .map_err(|e| ExtensionError::StorageError { source: e })
}

/// Test a storage backend connection (requires filesync:backends:read permission)
#[tauri::command(rename_all = "camelCase")]
pub async fn extension_remote_storage_test_backend(
    app_handle: AppHandle,
    public_key: String,
    name: String,
    backend_id: String,
    state: State<'_, AppState>,
) -> Result<(), ExtensionError> {
    let extension_id = get_extension_id_by_key_and_name(&state, &public_key, &name)?;

    // Check filesync permission for backends (read is sufficient for testing)
    let permission_result = PermissionManager::check_filesync_permission(
        &state,
        &extension_id,
        FileSyncAction::Read,
        FileSyncTarget::Backends,
    )
    .await;

    if let Err(ref e) = permission_result {
        emit_permission_prompt_if_needed(&app_handle, e);
    }
    permission_result?;

    // Delegate to internal remote storage command
    remote_storage::remote_storage_test_backend(state, backend_id)
        .await
        .map_err(|e| ExtensionError::StorageError { source: e })
}

// ============================================================================
// Storage Operations Commands (with permission checks)
// ============================================================================

/// Upload data to a storage backend (requires filesync:backends:readWrite permission)
#[tauri::command(rename_all = "camelCase")]
pub async fn extension_remote_storage_upload(
    app_handle: AppHandle,
    public_key: String,
    name: String,
    request: StorageUploadRequest,
    state: State<'_, AppState>,
) -> Result<(), ExtensionError> {
    let extension_id = get_extension_id_by_key_and_name(&state, &public_key, &name)?;

    // Check filesync permission for backends (write)
    let permission_result = PermissionManager::check_filesync_permission(
        &state,
        &extension_id,
        FileSyncAction::ReadWrite,
        FileSyncTarget::Backends,
    )
    .await;

    if let Err(ref e) = permission_result {
        emit_permission_prompt_if_needed(&app_handle, e);
    }
    permission_result?;

    // Delegate to internal remote storage command
    remote_storage::remote_storage_upload(state, request)
        .await
        .map_err(|e| ExtensionError::StorageError { source: e })
}

/// Download data from a storage backend (requires filesync:backends:read permission)
#[tauri::command(rename_all = "camelCase")]
pub async fn extension_remote_storage_download(
    app_handle: AppHandle,
    public_key: String,
    name: String,
    request: StorageDownloadRequest,
    state: State<'_, AppState>,
) -> Result<String, ExtensionError> {
    let extension_id = get_extension_id_by_key_and_name(&state, &public_key, &name)?;

    // Check filesync permission for backends (read)
    let permission_result = PermissionManager::check_filesync_permission(
        &state,
        &extension_id,
        FileSyncAction::Read,
        FileSyncTarget::Backends,
    )
    .await;

    if let Err(ref e) = permission_result {
        emit_permission_prompt_if_needed(&app_handle, e);
    }
    permission_result?;

    // Delegate to internal remote storage command
    remote_storage::remote_storage_download(state, request)
        .await
        .map_err(|e| ExtensionError::StorageError { source: e })
}

/// Delete an object from a storage backend (requires filesync:backends:readWrite permission)
#[tauri::command(rename_all = "camelCase")]
pub async fn extension_remote_storage_delete(
    app_handle: AppHandle,
    public_key: String,
    name: String,
    request: StorageDeleteRequest,
    state: State<'_, AppState>,
) -> Result<(), ExtensionError> {
    let extension_id = get_extension_id_by_key_and_name(&state, &public_key, &name)?;

    // Check filesync permission for backends (write)
    let permission_result = PermissionManager::check_filesync_permission(
        &state,
        &extension_id,
        FileSyncAction::ReadWrite,
        FileSyncTarget::Backends,
    )
    .await;

    if let Err(ref e) = permission_result {
        emit_permission_prompt_if_needed(&app_handle, e);
    }
    permission_result?;

    // Delegate to internal remote storage command
    remote_storage::remote_storage_delete(state, request)
        .await
        .map_err(|e| ExtensionError::StorageError { source: e })
}

/// List objects in a storage backend (requires filesync:backends:read permission)
#[tauri::command(rename_all = "camelCase")]
pub async fn extension_remote_storage_list(
    app_handle: AppHandle,
    public_key: String,
    name: String,
    request: StorageListRequest,
    state: State<'_, AppState>,
) -> Result<Vec<StorageObjectInfo>, ExtensionError> {
    let extension_id = get_extension_id_by_key_and_name(&state, &public_key, &name)?;

    // Check filesync permission for backends (read)
    let permission_result = PermissionManager::check_filesync_permission(
        &state,
        &extension_id,
        FileSyncAction::Read,
        FileSyncTarget::Backends,
    )
    .await;

    if let Err(ref e) = permission_result {
        emit_permission_prompt_if_needed(&app_handle, e);
    }
    permission_result?;

    // Delegate to internal remote storage command
    remote_storage::remote_storage_list(state, request)
        .await
        .map_err(|e| ExtensionError::StorageError { source: e })
}
