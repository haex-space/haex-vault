use super::paths::VAULT_EXTENSION;
use super::*;

use crate::database::error::DatabaseError;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use std::time::UNIX_EPOCH;
use tauri::AppHandle;
use ts_rs::TS;

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct VaultInfo {
    name: String,
    last_access: u64,
    path: String,
}

/// Lists all vault databases in the vaults directory
#[tauri::command]
pub fn list_vaults(app_handle: AppHandle) -> Result<Vec<VaultInfo>, DatabaseError> {
    let vaults_dir_str = get_vaults_directory(&app_handle)?;
    let vaults_dir = Path::new(&vaults_dir_str);

    println!("Suche vaults in {}", vaults_dir.display());

    let mut vaults: Vec<VaultInfo> = vec![];

    if !vaults_dir.exists() {
        println!("Vaults-Verzeichnis existiert nicht, gebe leere Liste zurück.");
        return Ok(vec![]);
    }

    for entry in fs::read_dir(vaults_dir).map_err(|e| DatabaseError::IoError {
        path: "vaults directory".to_string(),
        reason: e.to_string(),
    })? {
        let entry = entry.map_err(|e| DatabaseError::IoError {
            path: "vaults directory entry".to_string(),
            reason: e.to_string(),
        })?;

        println!("Suche entry {}", entry.path().to_string_lossy());
        let path = entry.path();
        if path.is_file() {
            if let Some(filename) = path.file_name().and_then(|n| n.to_str()) {
                if filename.ends_with(VAULT_EXTENSION) {
                    // Entferne .db Endung für die Rückgabe
                    println!("Vault gefunden {filename}");

                    let metadata = fs::metadata(&path).map_err(|e| DatabaseError::IoError {
                        path: path.to_string_lossy().to_string(),
                        reason: format!("Metadaten konnten nicht gelesen werden: {e}"),
                    })?;

                    let last_access_timestamp = metadata
                        .accessed()
                        .map_err(|e| DatabaseError::IoError {
                            path: path.to_string_lossy().to_string(),
                            reason: format!("Zugriffszeit konnte nicht gelesen werden: {e}"),
                        })?
                        .duration_since(UNIX_EPOCH)
                        .unwrap_or_default() // Fallback für den seltenen Fall einer Zeit vor 1970
                        .as_secs();

                    let vault_name = filename.trim_end_matches(VAULT_EXTENSION).to_string();

                    vaults.push(VaultInfo {
                        name: vault_name,
                        last_access: last_access_timestamp,
                        path: path.to_string_lossy().to_string(),
                    });
                }
            }
        }
    }

    Ok(vaults)
}

/// Checks if a vault with the given name exists
#[tauri::command]
pub fn vault_exists(app_handle: AppHandle, vault_name: String) -> Result<bool, DatabaseError> {
    let vault_path = get_vault_path(&app_handle, &vault_name)?;
    Ok(Path::new(&vault_path).exists())
}
