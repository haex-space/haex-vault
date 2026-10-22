//! `extension_list` and `extension_icon` (US1), `extension_remove` (US4, R11), and enabling,
//! deleting kept data and the limits (US7, T091, T092).

use tauri::{AppHandle, State};
use uuid::Uuid;

use crate::error::{HolziError, Result};
use crate::extensions::logs::{self, LogEntry, LogQuery};
use crate::extensions::registry::limits::{self, ExtensionLimits, ExtensionLimitsView};
use crate::extensions::registry::list::{icon_data_url, list, with_dev_projects, ExtensionSummary};
use crate::extensions::registry::remove::{purge_kept_data, remove, set_enabled};
use crate::passwords::clock::unix_millis;
use crate::state::AppState;
use crate::state_utils::active_database;
use crate::storage::wm_session_commands::current_device_uuid;
use crate::vault_gate::VaultDb;

fn parse_extension_id(text: &str) -> Result<Uuid> {
    Uuid::parse_str(text).map_err(|_| HolziError::ExtensionNotFound)
}

/// Every extension of the vault, ordered by title.
#[tauri::command]
pub async fn extension_list(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Vec<ExtensionSummary>> {
    let db = active_database(&state)?;
    let device = current_device_uuid(&app, &db)?;
    let (listed, dev) = db
        .read(move |q| {
            let listed = list(q, device)?;
            Ok((listed, crate::extensions::dev::registrations(q, device)?))
        })
        .await?;
    tauri::async_runtime::spawn_blocking(move || with_dev_projects(listed, &dev))
        .await
        .map_err(|e| HolziError::InvalidInput {
            reason: format!("extension task: {e}"),
        })
}

/// The icon of the effective bundle as a `data:` URL.
#[tauri::command]
pub async fn extension_icon(
    state: State<'_, AppState>,
    extension_id: String,
) -> Result<Option<String>> {
    let id = parse_extension_id(&extension_id)?;
    let db = active_database(&state)?;
    db.read(move |q| icon_data_url(q, id).map_err(Into::into))
        .await
}

/// Removes an extension from the vault; with `delete_data` every device also drops its tables.
/// Each device clears up when the removal reaches it (research R11).
#[tauri::command]
pub async fn extension_remove(
    app: AppHandle,
    state: State<'_, AppState>,
    extension_id: String,
    delete_data: bool,
) -> Result<()> {
    let host = state.extensions();
    change(&app, &state, extension_id, move |db, id, now| {
        remove(db, id, delete_data, now)?;
        // An install of the same key and name gets the same id again: no decision carries over.
        host.permissions.forget_extension(id);
        host.fs.watches.end_all(id);
        host.notifications.close_all(id);
        host.mail_watches.end_all(id);
        host.shells.end_all(id);
        host.peers.end_all(id);
        Ok(())
    })
    .await
}

/// Runs a blocking registry write and tells holzi's window the extension changed.
async fn change(
    app: &AppHandle,
    state: &State<'_, AppState>,
    extension_id: String,
    write: impl FnOnce(&VaultDb, Uuid, i64) -> Result<()> + Send + 'static,
) -> Result<()> {
    let id = parse_extension_id(&extension_id)?;
    let db = active_database(state)?;
    tauri::async_runtime::spawn_blocking(move || {
        write(&db, id, unix_millis(std::time::SystemTime::now()))
    })
    .await
    .map_err(|e| HolziError::InvalidInput {
        reason: format!("extension task: {e}"),
    })??;
    super::emit_changed(app, vec![extension_id]);
    Ok(())
}

/// Enables or disables an extension on every device (FR-007, FR-039).
#[tauri::command]
pub async fn extension_set_enabled(
    app: AppHandle,
    state: State<'_, AppState>,
    extension_id: String,
    enabled: bool,
) -> Result<()> {
    let host = state.extensions();
    change(&app, &state, extension_id, move |db, id, now| {
        set_enabled(db, id, enabled, now)?;
        if !enabled {
            host.fs.watches.end_all(id);
            host.notifications.close_all(id);
            host.mail_watches.end_all(id);
            host.shells.end_all(id);
            host.peers.end_all(id);
        }
        Ok(())
    })
    .await
}

/// Deletes the kept data of a removed extension on every device (FR-008).
#[tauri::command]
pub async fn extension_purge_kept_data(
    app: AppHandle,
    state: State<'_, AppState>,
    extension_id: String,
) -> Result<()> {
    change(&app, &state, extension_id, purge_kept_data).await
}

/// The limits of an installed extension (FR-031).
#[tauri::command]
pub async fn extension_limits_get(
    state: State<'_, AppState>,
    extension_id: String,
) -> Result<ExtensionLimitsView> {
    let id = parse_extension_id(&extension_id)?;
    let db = active_database(&state)?;
    tauri::async_runtime::spawn_blocking(move || limits::get(&db, id))
        .await
        .map_err(|e| HolziError::InvalidInput {
            reason: format!("extension task: {e}"),
        })?
}

/// Stores the limits of an installed extension for every device (FR-031).
#[tauri::command]
pub async fn extension_limits_set(
    state: State<'_, AppState>,
    extension_id: String,
    limits: ExtensionLimits,
) -> Result<()> {
    let id = parse_extension_id(&extension_id)?;
    let db = active_database(&state)?;
    tauri::async_runtime::spawn_blocking(move || limits::set(&db, id, limits))
        .await
        .map_err(|e| HolziError::InvalidInput {
            reason: format!("extension task: {e}"),
        })?
}

/// The log of an extension on this device, newest first, for the settings (T086).
#[tauri::command]
pub async fn extension_logs_read(
    app: AppHandle,
    state: State<'_, AppState>,
    extension_id: String,
    level: Option<String>,
    limit: i64,
    before: Option<i64>,
) -> Result<Vec<LogEntry>> {
    let id = parse_extension_id(&extension_id)?;
    let db = active_database(&state)?;
    let device = current_device_uuid(&app, &db)?;
    let query = LogQuery {
        level,
        limit,
        offset: 0,
        before,
    };
    db.read(move |q| logs::read(q, id, device, &query)).await
}
