//! Commands of encrypted folders (spec 048 US1, US7, contracts/tauri-commands.md): create one in a
//! storage or on this device, the one-time notice before the first (FR-002, device preference
//! `files.encryptedNotice`), and loading a folder's sidecars again.

use serde::Serialize;
use tauri::{AppHandle, State};
use ts_rs::TS;

use super::device_storage;
use super::{device_path, place_of, Place};
use crate::device::commands::resolve_vault_device_uuid;
use crate::files::access::Want;
use crate::files::encrypted::device::DeviceRoot;
use crate::files::encrypted::ops;
use crate::files::encrypted::route::vault_error;
use crate::files::state::FilesState;
use crate::files::storage_source::StorageFiles;
use crate::files::{Entry, FilesError, FilesErrorCode, SourceRef};
use crate::state::AppState;
use crate::storage::preferences::{self, PrefScope};

/// The device preference that the notice of FR-002 was shown.
const NOTICE: &str = "files.encryptedNotice";

/// Whether this device showed the notice before the first encrypted folder.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct EncryptedNotice {
    pub shown: bool,
}

/// The storage of `path` with the path in it: a storage, or the device as its storage (spec 048
/// US7). A device folder must be one the user may change (not in holzi's own places).
async fn storage_at(
    app: &AppHandle,
    state: &AppState,
    files: &FilesState,
    source: &SourceRef,
    path: &str,
) -> Result<(StorageFiles, String, Option<DeviceRoot>), FilesError> {
    match place_of(app, state, files, source, path, Want::Write).await? {
        Place::Storage {
            files: storage,
            path,
            device,
        } => Ok((storage, path, device)),
        Place::Device => {
            let real = device_path(files, source, path, Want::Write)?;
            let root = DeviceRoot::of(&real).ok_or_else(|| {
                FilesError::new(
                    FilesErrorCode::Unsupported,
                    "no encrypted folder in this place",
                )
            })?;
            let storage = device_storage(app, state, files, &root)?;
            let view = root.view_of(&real);
            Ok((storage, view, Some(root)))
        }
    }
}

/// Creates the encrypted folder `name` in the ordinary folder `path` of a storage or this device.
#[tauri::command]
pub async fn files_create_encrypted_folder(
    app: AppHandle,
    state: State<'_, AppState>,
    files: State<'_, FilesState>,
    source: SourceRef,
    path: String,
    name: String,
) -> Result<Entry, FilesError> {
    let (storage, path, device) = storage_at(&app, &state, &files, &source, &path).await?;
    let entry = ops::create_encrypted_folder(&storage, &path, &name).await?;
    Ok(Place::entry(device.as_ref(), entry))
}

async fn device_scope(
    app: &AppHandle,
    state: &AppState,
) -> Result<(crate::vault_gate::VaultDb, PrefScope), FilesError> {
    let db = state.database().map_err(vault_error)?;
    let device = resolve_vault_device_uuid(app, &db)
        .await
        .map_err(vault_error)?;
    Ok((db, PrefScope::Device(device)))
}

/// Whether the notice was shown on this device.
#[tauri::command]
pub async fn files_encrypted_notice(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<EncryptedNotice, FilesError> {
    let (db, scope) = device_scope(&app, &state).await?;
    let value = db
        .read(move |q| preferences::get(q, scope, NOTICE))
        .await
        .map_err(vault_error)?;
    Ok(EncryptedNotice {
        shown: preferences::parse_bool(value.as_deref()) == Some(true),
    })
}

/// Notes on this device that the notice was shown.
#[tauri::command]
pub async fn files_encrypted_notice_ack(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), FilesError> {
    let (db, scope) = device_scope(&app, &state).await?;
    db.write(move |tx| preferences::insert_or_update(tx, scope, NOTICE, "true"))
        .await
        .map_err(vault_error)?;
    Ok(())
}

/// Loads the sidecars of the encrypted folder at `path` again (another device wrote into it).
#[tauri::command]
pub async fn files_encrypted_refresh(
    app: AppHandle,
    state: State<'_, AppState>,
    files: State<'_, FilesState>,
    source: SourceRef,
    path: String,
) -> Result<(), FilesError> {
    let (storage, path, _) = storage_at(&app, &state, &files, &source, &path).await?;
    ops::refresh(&storage, &path).await
}
