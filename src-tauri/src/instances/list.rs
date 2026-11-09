//! `list_instances` — directory scan under `<AppLocalData>/vaults/`.

use std::fs;
use std::time::UNIX_EPOCH;

use tauri::{AppHandle, State};

use crate::error::{HolziError, Result};
use crate::state::AppState;

use super::info::InstanceInfo;
use super::paths::{get_pending_marker_path, validate_instance_name, vault_paths};
use super::vault_paths::DATABASE_FILE;

/// Returns the vault currently held by this app process, if any.
///
/// This is intentionally app-scoped: it reports lifecycle state without opening or touching a
/// vault database, so a freshly reloaded frontend can return to the existing workspace.
#[tauri::command]
pub fn active_instance_name(state: State<'_, AppState>) -> Result<Option<String>> {
    state.active_name()
}

/// Scans `<AppLocalData>/vaults/` for vault folders (a folder with a `vault.db`). Hidden folders
/// (a vault being moved), folders whose name is no vault name and vaults whose creation is still
/// in progress are skipped. Returns entries sorted by mtime descending.
#[tauri::command]
pub async fn list_instances(app: AppHandle) -> Result<Vec<InstanceInfo>> {
    let dir = vault_paths(&app)?.vaults_dir();
    if !dir.exists() {
        return Ok(vec![]);
    }

    let mut out: Vec<InstanceInfo> = Vec::new();
    for entry in fs::read_dir(&dir).map_err(HolziError::from)? {
        let entry = entry.map_err(HolziError::from)?;
        let folder = entry.path();
        let Some(name) = folder.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if name.starts_with('.') || validate_instance_name(name).is_err() {
            continue;
        }
        let database = folder.join(DATABASE_FILE);
        if !database.is_file() {
            continue;
        }
        // Instance whose Genesis is still in progress: skip until cleanup.
        if get_pending_marker_path(&database).exists() {
            continue;
        }
        let metadata = fs::metadata(&database).map_err(HolziError::from)?;
        let last_access = metadata
            .modified()
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        out.push(InstanceInfo {
            name: name.to_string(),
            alias: None,
            last_access,
        });
    }

    out.sort_by_key(|instance| std::cmp::Reverse(instance.last_access));
    Ok(out)
}
