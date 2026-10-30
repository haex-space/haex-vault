//! `extension_install_preview` and `extension_install` (US1).

use tauri::{AppHandle, State};

use super::emit_changed;
use crate::error::{HolziError, Result};
use crate::extensions::registry::install::InstallPreview;
use crate::extensions::registry::install_source::{
    install_source, preview_source, BundleSource, ExtensionInstallArgs,
};
use crate::extensions::registry::list::{list, ExtensionSummary};
use crate::marketplace::MarketplaceState;
use crate::passwords::clock::unix_millis;
use crate::state::AppState;
use crate::state_utils::active_database;
use crate::storage::wm_session_commands::current_device_uuid;

fn join_error(error: tauri::Error) -> HolziError {
    HolziError::ExtensionInstall {
        reason: format!("install task: {error}"),
    }
}

/// Reads the bundle of `source` (a file chosen in the file dialog or a bundle downloaded from a
/// marketplace) and checks it; writes nothing. A marketplace bundle names its origin (FR-011).
#[tauri::command]
pub async fn extension_install_preview(
    app: AppHandle,
    state: State<'_, AppState>,
    market: State<'_, MarketplaceState>,
    source: BundleSource,
) -> Result<InstallPreview> {
    let db = active_database(&state)?;
    let pending = std::sync::Arc::clone(&market.pending);
    tauri::async_runtime::spawn_blocking(move || preview_source(&db, &app, &pending, &source))
        .await
        .map_err(join_error)?
}

/// Checks the bundle again and installs it for the vault (FR-005). A marketplace bundle leaves
/// the pending slot after a successful install only; a refused one can be tried again.
#[tauri::command]
pub async fn extension_install(
    app: AppHandle,
    state: State<'_, AppState>,
    market: State<'_, MarketplaceState>,
    args: ExtensionInstallArgs,
) -> Result<ExtensionSummary> {
    let db = active_database(&state)?;
    let device = current_device_uuid(&app, &db)?;
    let opener = app.clone();
    let pending = std::sync::Arc::clone(&market.pending);
    let id = tauri::async_runtime::spawn_blocking(move || {
        let now = unix_millis(std::time::SystemTime::now());
        let installed = install_source(&db, &opener, &pending, args, device, now)?;
        let id = installed.ids.extension_id.to_string();
        let summary = db
            .read_blocking(move |q| list(q, device).map_err(Into::into))?
            .into_iter()
            .find(|s| s.id == id)
            .ok_or(HolziError::ExtensionNotFound)?;
        Ok::<_, HolziError>(summary)
    })
    .await
    .map_err(join_error)??;
    emit_changed(&app, vec![id.id.clone()]);
    Ok(id)
}
