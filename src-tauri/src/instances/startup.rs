//! Startup orphan cleanup: remove the folders of vaults that carry a `.pending` marker. Called
//! from `lib.rs::setup` before any command runs.
//!
//! Contract: `tauri-commands.md` — a crash between `create_instance`
//! step 3 (write marker + create DB) and step 7 (remove marker) leaves
//! a `.pending` file. That file is the signal that the DB is
//! half-initialised and must be removed.

use std::fs;
use std::path::Path;

use tauri::AppHandle;

use crate::error::{HolziError, Result};

use super::events::emit_instance_list_changed;
use super::paths::{get_pending_marker_path, vault_paths};
use super::vault_paths::DATABASE_FILE;
use crate::models::{import, paths as model_paths};

/// Tauri-facing entry. Resolves the vaults directory and delegates
/// to the pure `cleanup_orphans_in_dir` helper.
pub fn cleanup_orphans_on_startup(app: &AppHandle) -> Result<()> {
    let dir = vault_paths(app)?.vaults_dir();
    let removed = cleanup_orphans_in_dir(&dir)?;
    let models_root = model_paths::models_root(app)?;
    let removed_model_staging = import::cleanup_staging_in_dir(&models_root)?;
    if removed + removed_model_staging > 0 {
        emit_instance_list_changed(app, "startup-cleanup", None);
    }
    Ok(())
}

/// Pure filesystem worker: scans the vault folders in `vaults_dir` for a `vault.db.pending`
/// marker and takes each such vault back with its whole folder (`cleanup::take_back`); a folder
/// that holds nothing at all (a creation that stopped before its marker) goes too. Returns the
/// number of folders removed. A missing `vaults_dir` is not an error.
pub fn cleanup_orphans_in_dir(vaults_dir: &Path) -> Result<usize> {
    if !vaults_dir.exists() {
        return Ok(0);
    }
    let mut removed = 0usize;
    for entry in fs::read_dir(vaults_dir).map_err(HolziError::from)? {
        let entry = entry.map_err(HolziError::from)?;
        let folder = entry.path();
        let Some(name) = folder.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        // A vault being moved (`.moving-<name>`) is the business of `layout_move`.
        if name.starts_with('.') || !folder.is_dir() {
            continue;
        }
        let database = folder.join(DATABASE_FILE);
        if get_pending_marker_path(&database).exists() {
            // The vault's files first, with its sidecar files and temporary copies (spec 043),
            // then the folder. A deletion failure retains the marker so the file cannot be
            // published as a healthy instance.
            if super::cleanup::take_back(&database) {
                removed += 1;
            }
        } else if fs::remove_dir(&folder).is_ok() {
            // Only an empty folder goes; `remove_dir` refuses anything else.
            removed += 1;
        }
    }
    Ok(removed)
}
