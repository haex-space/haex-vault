//! Developer mode for holzi's window (spec 017, US12, T094, contracts/tauri-commands.md): the
//! switch, loading a project folder, confirming its permissions and unloading it.

use std::path::PathBuf;

use serde::Serialize;
use tauri::{AppHandle, State};
use ts_rs::TS;
use uuid::Uuid;

use crate::error::{HolziError, Result};
use crate::extensions::dev;
use crate::extensions::registry::install::{InstallPreview, PermissionChoice};
use crate::extensions::registry::lifecycle::reconcile;
use crate::passwords::clock::unix_millis;
use crate::state::AppState;
use crate::state_utils::active_database;
use crate::storage::wm_session_commands::current_device_uuid;

/// Developer mode on this device and whether holzi's window can show development servers yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct DevModeState {
    pub enabled: bool,
    /// The document holzi's window shows was served with the development origins; when `enabled`
    /// differs, the window reloads to take the new policy.
    pub frames_allowed: bool,
}

fn task_error(error: impl std::fmt::Display) -> HolziError {
    HolziError::InvalidInput {
        reason: format!("developer mode task: {error}"),
    }
}

fn state_of(state: &State<'_, AppState>, enabled: bool) -> DevModeState {
    let host = state.extensions();
    host.set_dev_frames(enabled);
    DevModeState {
        enabled,
        frames_allowed: host.served_dev_frames(),
    }
}

/// Developer mode on this device; also tells the next document of holzi's window whether it may
/// frame development servers.
#[tauri::command]
pub async fn extension_dev_mode_get(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<DevModeState> {
    let db = active_database(&state)?;
    let device = current_device_uuid(&app, &db)?;
    let enabled = db
        .read(move |q| dev::mode(q, device).map_err(Into::into))
        .await?;
    Ok(state_of(&state, enabled))
}

/// Switches developer mode on this device (FR-063). The caller reloads holzi's window.
#[tauri::command]
pub async fn extension_dev_mode_set(
    app: AppHandle,
    state: State<'_, AppState>,
    enabled: bool,
) -> Result<DevModeState> {
    let db = active_database(&state)?;
    let device = current_device_uuid(&app, &db)?;
    db.write(move |tx| dev::set_mode(tx, device, enabled).map_err(Into::into))
        .await?;
    super::emit_changed(&app, Vec::new());
    Ok(state_of(&state, enabled))
}

/// Reads a project folder and shows what loading it would do; refused while developer mode is
/// off or the prefix is taken (FR-065).
#[tauri::command]
pub async fn extension_dev_load(
    app: AppHandle,
    state: State<'_, AppState>,
    project_path: String,
) -> Result<InstallPreview> {
    let db = active_database(&state)?;
    let device = current_device_uuid(&app, &db)?;
    let path = PathBuf::from(project_path);
    let project = tauri::async_runtime::spawn_blocking(move || dev::read_project(&path))
        .await
        .map_err(task_error)??;
    db.read(move |q| {
        if !dev::mode(q, device)? {
            return Err(HolziError::ExtensionInstall {
                reason: "dev_mode_off".into(),
            }
            .into());
        }
        dev::preview(q, &project, device).map_err(Into::into)
    })
    .await
}

/// Registers the project folder with the permissions the developer confirmed.
#[tauri::command]
pub async fn extension_dev_confirm(
    app: AppHandle,
    state: State<'_, AppState>,
    project_path: String,
    accepted: Vec<PermissionChoice>,
) -> Result<String> {
    let db = active_database(&state)?;
    let device = current_device_uuid(&app, &db)?;
    let path = PathBuf::from(project_path);
    let id = tauri::async_runtime::spawn_blocking(move || {
        dev::confirm(
            &db,
            &path,
            accepted,
            device,
            unix_millis(std::time::SystemTime::now()),
        )
    })
    .await
    .map_err(task_error)??;
    super::emit_changed(&app, vec![id.to_string()]);
    Ok(id.to_string())
}

/// Unloads a development version and deletes its tables; a signed install of the same prefix
/// that waited starts now.
#[tauri::command]
pub async fn extension_dev_unload(
    app: AppHandle,
    state: State<'_, AppState>,
    extension_id: String,
) -> Result<()> {
    let id = Uuid::parse_str(&extension_id).map_err(|_| HolziError::ExtensionNotFound)?;
    let db = active_database(&state)?;
    let device = current_device_uuid(&app, &db)?;
    let host = state.extensions();
    tauri::async_runtime::spawn_blocking(move || {
        dev::unload(&db, id)?;
        // The id is the same when the project loads again: no decision of this session carries over.
        host.permissions.forget_extension(id);
        host.fs.watches.end_all(id);
        host.notifications.close_all(id);
        host.mail_watches.end_all(id);
        host.shells.end_all(id);
        host.peers.end_all(id);
        reconcile(
            &db,
            &host,
            device,
            unix_millis(std::time::SystemTime::now()),
        )
        .map(drop)
    })
    .await
    .map_err(task_error)??;
    super::emit_changed(&app, vec![extension_id]);
    Ok(())
}
