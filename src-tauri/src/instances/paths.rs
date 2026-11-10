//! Path resolution + name validation for managed instance files.
//!
//! Contract: `tauri-commands.md`. All paths anchor at
//! `<AppLocalData>/vaults/<name>/` (`vault_paths`); the backend owns path
//! construction, the frontend never sends a managed path (FR-025).

use std::path::{Path, PathBuf};

use tauri::{AppHandle, Manager, Runtime};

use super::vault_paths::VaultPaths;
use crate::error::{HolziError, Result};

pub const PENDING_MARKER_EXTENSION: &str = "db.pending";

/// Instance-name regex per contract: alphanumeric + `_`/`-`, first char
/// must be alphanumeric, 1..=64 chars.
fn is_valid_name(name: &str) -> bool {
    let bytes = name.as_bytes();
    if bytes.is_empty() || bytes.len() > 64 {
        return false;
    }
    let is_alnum = |b: u8| b.is_ascii_alphanumeric();
    let is_valid_body = |b: u8| b.is_ascii_alphanumeric() || b == b'_' || b == b'-';
    is_alnum(bytes[0]) && bytes.iter().all(|&b| is_valid_body(b))
}

/// Validates an instance name per contract regex. Errors with
/// `InvalidName` if the name is empty, too long, or contains disallowed
/// characters.
pub fn validate_instance_name(name: &str) -> Result<()> {
    if is_valid_name(name) {
        Ok(())
    } else {
        Err(HolziError::InvalidName {
            reason: "must match ^[A-Za-z0-9][A-Za-z0-9_-]{0,63}$".to_string(),
        })
    }
}

/// Resolves `<AppLocalData>/vaults/<name>/vault.db`. Validates `name` first. Touches no disk: the
/// folder is made by whoever creates the vault.
pub fn get_instance_path<R: Runtime>(app: &AppHandle<R>, name: &str) -> Result<PathBuf> {
    validate_instance_name(name)?;
    Ok(vault_paths(app)?.database(name))
}

/// The paths of the vaults of this installation (spec 084).
pub fn vault_paths<R: Runtime>(app: &AppHandle<R>) -> Result<VaultPaths> {
    get_app_local_data(app).map(VaultPaths::new)
}

/// Sibling `vault.db.pending` marker path.
pub fn get_pending_marker_path(instance_path: &Path) -> PathBuf {
    let mut p = instance_path.to_path_buf();
    p.set_extension(PENDING_MARKER_EXTENSION);
    p
}

/// Resolves `<AppLocalData>/` — where the installation-id file lives.
pub fn get_app_local_data<R: Runtime>(app: &AppHandle<R>) -> Result<PathBuf> {
    app.path()
        .app_local_data_dir()
        .map_err(|e| HolziError::PathResolution {
            reason: format!("resolve app_local_data_dir: {e}"),
        })
}
