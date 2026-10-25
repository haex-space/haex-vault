//! The Tauri commands of file shares (spec 056 contracts/tauri-commands.md); they act for the user.
//! Agents reach shares only through the catalog with their own permission (US6).

use tauri::{AppHandle, State};

use super::device_access;
use super::manage::{self, FileShare, FileShareSaveArgs, Here};
use crate::device::commands::resolve_vault_device_uuid;
use crate::files::state::FilesState;
use crate::files::{FilesError, FilesErrorCode, SourceRef};
use crate::state::AppState;
use crate::vault_gate::VaultDb;

/// The open vault and this device's `vault_device_uuid`.
async fn here(app: &AppHandle, state: &AppState) -> Result<(VaultDb, String), FilesError> {
    let closed = || FilesError::new(FilesErrorCode::Unsupported, "no open vault");
    let db = state.database().map_err(|_| closed())?;
    let device = resolve_vault_device_uuid(app, &db)
        .await
        .map_err(|_| closed())?;
    Ok((db, device.to_string()))
}

#[tauri::command]
pub async fn file_shares_list(
    app: AppHandle,
    state: State<'_, AppState>,
    files: State<'_, FilesState>,
) -> Result<Vec<FileShare>, FilesError> {
    let (db, device) = here(&app, &state).await?;
    manage::list(&Here {
        db: &db,
        device: &device,
        own: &files.own,
    })
    .await
}

#[tauri::command]
pub async fn file_share_of(
    app: AppHandle,
    state: State<'_, AppState>,
    files: State<'_, FilesState>,
    source: SourceRef,
    path: String,
) -> Result<Option<FileShare>, FilesError> {
    let (db, device) = here(&app, &state).await?;
    manage::of(
        &Here {
            db: &db,
            device: &device,
            own: &files.own,
        },
        source,
        path,
    )
    .await
}

#[tauri::command]
pub async fn file_share_save(
    app: AppHandle,
    state: State<'_, AppState>,
    files: State<'_, FilesState>,
    args: FileShareSaveArgs,
) -> Result<FileShare, FilesError> {
    let (db, device) = here(&app, &state).await?;
    manage::save_from(
        &Here {
            db: &db,
            device: &device,
            own: &files.own,
        },
        args,
        Some(&crate::files::browser_commands::storage_opener(&app)),
    )
    .await
}

#[tauri::command]
pub async fn file_share_revoke(
    app: AppHandle,
    state: State<'_, AppState>,
    files: State<'_, FilesState>,
    share_id: String,
    vault_pubkey: String,
) -> Result<FileShare, FilesError> {
    let (db, device) = here(&app, &state).await?;
    let here = Here {
        db: &db,
        device: &device,
        own: &files.own,
    };
    manage::revoke(&here, share_id, vault_pubkey).await
}

#[tauri::command]
pub async fn file_share_delete(
    app: AppHandle,
    state: State<'_, AppState>,
    files: State<'_, FilesState>,
    share_id: String,
) -> Result<(), FilesError> {
    let (db, device) = here(&app, &state).await?;
    manage::delete(
        &Here {
            db: &db,
            device: &device,
            own: &files.own,
        },
        share_id,
    )
    .await
}

#[tauri::command]
pub async fn file_shares_of_contact(
    app: AppHandle,
    state: State<'_, AppState>,
    files: State<'_, FilesState>,
    contact_id: String,
) -> Result<Vec<FileShare>, FilesError> {
    let (db, device) = here(&app, &state).await?;
    manage::of_contact(
        &Here {
            db: &db,
            device: &device,
            own: &files.own,
        },
        contact_id,
    )
    .await
}

/// Whether other own devices may reach this device's files (spec 056 FR-014).
#[tauri::command]
pub async fn files_device_access_get(state: State<'_, AppState>) -> Result<bool, FilesError> {
    let db = state
        .database()
        .map_err(|_| FilesError::new(FilesErrorCode::Unsupported, "no open vault"))?;
    db.read(|q| Ok(device_access::enabled(q)?))
        .await
        .map_err(|error| FilesError::new(FilesErrorCode::Unsupported, error.to_string()))
}

/// Turns this device's switch on or off; only the user, through a guardrail action (research R12).
/// Returns the state now.
#[tauri::command]
pub async fn files_device_access_set(
    state: State<'_, AppState>,
    enabled: bool,
) -> Result<bool, FilesError> {
    let db = state
        .database()
        .map_err(|_| FilesError::new(FilesErrorCode::Unsupported, "no open vault"))?;
    let now = crate::passwords::clock::now();
    db.write(move |tx| {
        device_access::set_enabled(tx, enabled, &now)?;
        Ok(device_access::enabled(tx)?)
    })
    .await
    .map_err(|error| FilesError::new(FilesErrorCode::Unsupported, error.to_string()))
}
