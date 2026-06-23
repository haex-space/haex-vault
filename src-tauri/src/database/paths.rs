use crate::database::error::DatabaseError;
use std::fs;
use tauri::{path::BaseDirectory, AppHandle, Manager};

pub(super) const VAULT_EXTENSION: &str = ".db";
const VAULT_DIRECTORY: &str = "vaults";

/// Resolves a database name to the full vault path
pub(super) fn get_vault_path(
    app_handle: &AppHandle,
    vault_name: &str,
) -> Result<String, DatabaseError> {
    // Sicherstellen, dass der Name eine .db Endung hat
    let vault_file_name = if vault_name.ends_with(VAULT_EXTENSION) {
        vault_name.to_string()
    } else {
        format!("{vault_name}{VAULT_EXTENSION}")
    };

    let vault_directory = get_vaults_directory(app_handle)?;

    let vault_path = app_handle
        .path()
        .resolve(
            format!("{vault_directory}/{vault_file_name}"),
            BaseDirectory::AppLocalData,
        )
        .map_err(|e| DatabaseError::PathResolutionError {
            reason: format!("Failed to resolve vault path for '{vault_file_name}': {e}"),
        })?;

    // Sicherstellen, dass das vaults-Verzeichnis existiert
    if let Some(parent) = vault_path.parent() {
        fs::create_dir_all(parent).map_err(|e| DatabaseError::IoError {
            path: parent.display().to_string(),
            reason: format!("Failed to create vaults directory: {e}"),
        })?;
    }

    Ok(vault_path.to_string_lossy().to_string())
}

/// Returns the vaults directory path
#[tauri::command]
pub fn get_vaults_directory(app_handle: &AppHandle) -> Result<String, DatabaseError> {
    let vaults_dir = app_handle
        .path()
        .resolve(VAULT_DIRECTORY, BaseDirectory::AppLocalData)
        .map_err(|e| DatabaseError::PathResolutionError {
            reason: e.to_string(),
        })?;

    Ok(vaults_dir.to_string_lossy().to_string())
}
