//! Read-only extension query commands.

use crate::{
    extension::{
        core::{ExtensionInfoResponse, ExtensionPreview},
        error::ExtensionError,
    },
    AppState,
};
use tauri::{AppHandle, State};

#[tauri::command]
pub fn get_extension_info(
    public_key: String,
    name: String,
    state: State<AppState>,
) -> Result<ExtensionInfoResponse, ExtensionError> {
    let extension = state
        .extension_manager
        .get_extension_by_public_key_and_name(&public_key, &name)?
        .ok_or_else(|| ExtensionError::NotFound {
            public_key: public_key.clone(),
            name: name.clone(),
        })?;

    ExtensionInfoResponse::from_extension(&extension)
}

#[tauri::command]
pub async fn get_all_extensions(
    app_handle: AppHandle,
    state: State<'_, AppState>,
) -> Result<Vec<ExtensionInfoResponse>, String> {
    state
        .extension_manager
        .load_installed_extensions(&app_handle, &state)
        .await
        .map_err(|e| format!("Failed to load extensions: {e:?}"))?;

    let mut extensions = Vec::new();

    {
        let available_exts = state
            .extension_manager
            .available_extensions
            .lock()
            .map_err(|e| ExtensionError::MutexPoisoned {
                reason: e.to_string(),
            })?;
        for ext in available_exts.values() {
            extensions.push(ExtensionInfoResponse::from_extension(ext)?);
        }
    }

    Ok(extensions)
}

#[tauri::command]
pub async fn preview_extension(
    app_handle: AppHandle,
    state: State<'_, AppState>,
    file_bytes: Vec<u8>,
) -> Result<ExtensionPreview, ExtensionError> {
    state
        .extension_manager
        .preview_extension_internal(&app_handle, file_bytes)
        .await
}

#[tauri::command]
pub fn is_extension_installed(
    public_key: String,
    name: String,
    extension_version: String,
    state: State<'_, AppState>,
) -> Result<bool, ExtensionError> {
    if let Some(ext) = state
        .extension_manager
        .get_extension_by_public_key_and_name(&public_key, &name)?
    {
        Ok(ext.manifest.version == extension_version)
    } else {
        Ok(false)
    }
}
