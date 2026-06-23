//! Extension installation and removal commands.

use crate::{
    extension::{
        core::{EditablePermissions, ExtensionManifest},
        error::ExtensionError,
    },
    AppState,
};
use tauri::{AppHandle, State};

/// Register extension metadata in database (UPSERT - handles sync case).
/// Takes manifest data directly - call preview_extension first to get the manifest.
/// Returns the extension ID.
#[tauri::command]
pub fn register_extension_in_database(
    manifest: ExtensionManifest,
    custom_permissions: EditablePermissions,
    state: State<'_, AppState>,
) -> Result<String, ExtensionError> {
    state
        .extension_manager
        .register_extension_in_database(&manifest, &custom_permissions, &state)
}

/// Install extension files to local filesystem.
/// Use this after register_extension_in_database or when extension
/// already exists in DB (e.g., from sync).
/// Returns the extension ID.
#[tauri::command]
pub async fn install_extension_files(
    app_handle: AppHandle,
    file_bytes: Vec<u8>,
    extension_id: String,
    state: State<'_, AppState>,
) -> Result<String, ExtensionError> {
    state
        .extension_manager
        .install_extension_files_from_bytes(&app_handle, file_bytes, &extension_id, &state)
        .await
}

/// Full installation: Register in DB + Install files.
/// Convenience function that calls both steps.
#[tauri::command]
pub async fn install_extension_with_permissions(
    app_handle: AppHandle,
    file_bytes: Vec<u8>,
    custom_permissions: EditablePermissions,
    state: State<'_, AppState>,
) -> Result<String, ExtensionError> {
    state
        .extension_manager
        .install_extension_with_permissions_internal(
            app_handle,
            file_bytes,
            custom_permissions,
            &state,
        )
        .await
}

#[tauri::command]
pub async fn remove_extension(
    app_handle: AppHandle,
    public_key: String,
    name: String,
    version: String,
    delete_data: Option<bool>,
    state: State<'_, AppState>,
) -> Result<(), ExtensionError> {
    state
        .extension_manager
        .remove_extension_internal(
            &app_handle,
            &public_key,
            &name,
            &version,
            delete_data.unwrap_or(false),
            &state,
        )
        .await
}
