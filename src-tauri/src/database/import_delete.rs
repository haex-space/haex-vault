use super::paths::VAULT_EXTENSION;
use super::*;

use crate::database::error::DatabaseError;
use std::fs;
use std::path::Path;
use tauri::{AppHandle, Emitter};
#[cfg(not(target_os = "android"))]
use trash;

/// Notify the frontend that the on-disk vault inventory changed (a vault was
/// imported, deleted, or moved to trash). Front-end stores listen for this and
/// re-run `list_vaults` to pick up the new state.
///
/// Best-effort: emission failures are logged but never block the mutating
/// command — the worst case is a stale UI that refreshes on the next manual
/// action or remount.
fn emit_vault_list_changed(app_handle: &AppHandle) {
    if let Err(e) = app_handle.emit("vault-list-changed", ()) {
        eprintln!("Failed to emit vault-list-changed: {e}");
    }
}

/// Imports a vault database file from an external location into the vaults directory.
/// Returns the new path of the imported vault.
/// Fails if a vault with the same name already exists.
#[tauri::command]
pub fn import_vault(
    app_handle: AppHandle,
    source_path: String,
    vault_name: Option<String>,
) -> Result<String, DatabaseError> {
    // Android: source comes as a Content URI JSON envelope (SAF). std::fs cannot
    // open content:// URIs, so we read through ContentResolver via android_fs.
    #[cfg(target_os = "android")]
    if source_path.starts_with('{') {
        return import_vault_from_content_uri(&app_handle, &source_path, vault_name);
    }

    let source = Path::new(&source_path);

    // Validate source file exists
    if !source.exists() {
        return Err(DatabaseError::IoError {
            path: source_path.clone(),
            reason: "Source file does not exist".to_string(),
        });
    }

    // Validate source file has .db extension
    if source.extension().and_then(|e| e.to_str()) != Some("db") {
        return Err(DatabaseError::ValidationError {
            reason: "Source file must have .db extension".to_string(),
        });
    }

    // Use provided vault_name or derive from file name
    let vault_name = match vault_name {
        Some(name) if !name.trim().is_empty() => name.trim().to_string(),
        _ => {
            let file_name = source.file_name().and_then(|n| n.to_str()).ok_or_else(|| {
                DatabaseError::ValidationError {
                    reason: "Could not extract file name from source path".to_string(),
                }
            })?;
            file_name.trim_end_matches(VAULT_EXTENSION).to_string()
        }
    };

    // Check if vault already exists
    let target_path = get_vault_path(&app_handle, &vault_name)?;
    if Path::new(&target_path).exists() {
        return Err(DatabaseError::VaultAlreadyExists {
            vault_name: vault_name.to_string(),
        });
    }

    // Copy the file to the vaults directory
    fs::copy(&source_path, &target_path).map_err(|e| DatabaseError::IoError {
        path: target_path.clone(),
        reason: format!("Failed to copy vault file: {e}"),
    })?;

    println!(
        "Vault '{}' successfully imported to '{}'",
        vault_name, target_path
    );

    emit_vault_list_changed(&app_handle);

    Ok(target_path)
}

/// Imports a vault from an Android Content URI (SAF picker result).
/// Reads bytes through ContentResolver and writes them into the vaults directory.
#[cfg(target_os = "android")]
fn import_vault_from_content_uri(
    app_handle: &AppHandle,
    source_uri_json: &str,
    vault_name: Option<String>,
) -> Result<String, DatabaseError> {
    use tauri_plugin_android_fs::{AndroidFsExt, FsUri};

    let api = app_handle.android_fs();

    let uri = FsUri::from_json_str(source_uri_json).map_err(|e| DatabaseError::IoError {
        path: source_uri_json.to_string(),
        reason: format!("Invalid Content URI: {e:?}"),
    })?;

    let display_name = api
        .get_name(&uri)
        .map_err(|e| DatabaseError::ValidationError {
            reason: format!("Could not read file name from Content URI: {e:?}"),
        })?;

    // Match desktop import validation: source must be a .db file.
    if Path::new(&display_name)
        .extension()
        .and_then(|e| e.to_str())
        != Some("db")
    {
        return Err(DatabaseError::ValidationError {
            reason: "Source file must have .db extension".to_string(),
        });
    }

    // Derive vault name: prefer caller-provided, else fall back to URI display name.
    let resolved_name = match vault_name {
        Some(name) if !name.trim().is_empty() => name.trim().to_string(),
        _ => display_name.trim_end_matches(VAULT_EXTENSION).to_string(),
    };

    let target_path = get_vault_path(app_handle, &resolved_name)?;
    if Path::new(&target_path).exists() {
        return Err(DatabaseError::VaultAlreadyExists {
            vault_name: resolved_name,
        });
    }

    let bytes = api.read(&uri).map_err(|e| DatabaseError::IoError {
        path: source_uri_json.to_string(),
        reason: format!("Failed to read source via ContentResolver: {e:?}"),
    })?;

    fs::write(&target_path, &bytes).map_err(|e| DatabaseError::IoError {
        path: target_path.clone(),
        reason: format!("Failed to write vault file: {e}"),
    })?;

    println!(
        "Vault '{}' successfully imported from Content URI to '{}'",
        resolved_name, target_path
    );

    emit_vault_list_changed(app_handle);

    Ok(target_path)
}

/// Moves a vault database file to trash (or deletes permanently if trash is unavailable)
#[tauri::command]
pub fn move_vault_to_trash(
    app_handle: AppHandle,
    vault_name: String,
) -> Result<String, DatabaseError> {
    // On Android, trash is not available, so delete permanently
    #[cfg(target_os = "android")]
    {
        println!(
            "Android platform detected, permanently deleting vault '{}'",
            vault_name
        );
        return delete_vault(app_handle, vault_name);
    }

    // On non-Android platforms, try to use trash
    #[cfg(not(target_os = "android"))]
    {
        let vault_path = get_vault_path(&app_handle, &vault_name)?;
        let vault_shm_path = format!("{vault_path}-shm");
        let vault_wal_path = format!("{vault_path}-wal");

        if !Path::new(&vault_path).exists() {
            // Vault file already gone — not an error, just clean up references
            return Ok(format!("Vault '{vault_name}' already removed"));
        }

        // Try to move to trash first (works on desktop systems)
        let moved_to_trash = trash::delete(&vault_path).is_ok();

        if moved_to_trash {
            // Also try to move auxiliary files to trash (ignore errors as they might not exist)
            let _ = trash::delete(&vault_shm_path);
            let _ = trash::delete(&vault_wal_path);

            emit_vault_list_changed(&app_handle);

            Ok(format!("Vault '{vault_name}' successfully moved to trash"))
        } else {
            // Fallback: Permanent deletion if trash fails
            println!(
                "Trash not available, falling back to permanent deletion for vault '{vault_name}'"
            );
            delete_vault(app_handle, vault_name)
        }
    }
}

/// Deletes a vault database file permanently (bypasses trash)
#[tauri::command]
pub fn delete_vault(app_handle: AppHandle, vault_name: String) -> Result<String, DatabaseError> {
    let vault_path = get_vault_path(&app_handle, &vault_name)?;
    let vault_shm_path = format!("{vault_path}-shm");
    let vault_wal_path = format!("{vault_path}-wal");

    if !Path::new(&vault_path).exists() {
        // Vault file already gone — not an error, just clean up references
        return Ok(format!("Vault '{vault_name}' already removed"));
    }

    if Path::new(&vault_shm_path).exists() {
        fs::remove_file(&vault_shm_path).map_err(|e| DatabaseError::IoError {
            path: vault_shm_path.clone(),
            reason: format!("Failed to delete vault: {e}"),
        })?;
    }

    if Path::new(&vault_wal_path).exists() {
        fs::remove_file(&vault_wal_path).map_err(|e| DatabaseError::IoError {
            path: vault_wal_path.clone(),
            reason: format!("Failed to delete vault: {e}"),
        })?;
    }

    fs::remove_file(&vault_path).map_err(|e| DatabaseError::IoError {
        path: vault_path.clone(),
        reason: format!("Failed to delete vault: {e}"),
    })?;

    emit_vault_list_changed(&app_handle);

    Ok(format!("Vault '{vault_name}' successfully deleted"))
}
