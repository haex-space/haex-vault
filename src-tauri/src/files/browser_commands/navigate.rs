//! Sources, folders and watching (spec 044 US1).

use serde::Serialize;
use tauri::ipc::Channel;
use tauri::{AppHandle, State};
use ts_rs::TS;

use super::{blocking, device_path, device_storage, place_of, Place};
use crate::files::access::Want;
use crate::files::encrypted::device::{name_encrypted, DeviceRoot};
use crate::files::encrypted::format::is_folder_prefix;
use crate::files::encrypted::redacted::Redacted;
use crate::files::encrypted::route::{route, Route};
use crate::files::local::drives::{drives, Drive};
use crate::files::local::ops;
use crate::files::state::FilesState;
use crate::files::{Entry, EntryKind, FilesError, FilesErrorCode, SourceRef};
use crate::state::AppState;
use tauri_plugin_holzi_android::HolziAndroidExt;

/// A known place for the sidebar.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct KnownPlace {
    pub name: String,
    pub path: String,
}

/// What the sidebar shows.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct Sources {
    pub drives: Vec<Drive>,
    pub known: Vec<KnownPlace>,
    /// The storages of spec 038 (US5).
    pub storages: Vec<StorageSource>,
}

/// A storage for the sidebar.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct StorageSource {
    pub id: String,
    pub name: String,
}

/// A change in a watched folder; the window reloads it.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct FolderChanged {
    pub paths: Vec<String>,
}

/// Drives and known places of this device.
#[tauri::command]
pub async fn files_sources(
    state: State<'_, AppState>,
    files: State<'_, FilesState>,
) -> Result<Sources, FilesError> {
    let known = files
        .known
        .iter()
        .map(|place| KnownPlace {
            name: place.name.to_owned(),
            path: place.path.to_string_lossy().into_owned(),
        })
        .collect();
    let drives = blocking(|| Ok(drives())).await?;
    // Without an open vault (or with a storage list that fails) the device alone shows.
    let storages = match crate::remote_storage::commands::service(&state) {
        Ok(service) => service
            .overview()
            .await
            .map(|overview| {
                overview
                    .storages
                    .into_iter()
                    .map(|storage| StorageSource {
                        id: storage.id,
                        name: storage.name,
                    })
                    .collect()
            })
            .unwrap_or_default(),
        Err(_) => Vec::new(),
    };
    Ok(Sources {
        drives,
        known,
        storages,
    })
}

/// Whether the file browser reaches the files of this device (FR-039): on Android "Zugriff auf alle
/// Dateien", everywhere else always. A failing question counts as not granted.
#[tauri::command]
pub async fn files_all_access(app: AppHandle) -> Result<bool, FilesError> {
    blocking(move || {
        Ok(app
            .holzi_android()
            .all_files_access()
            .unwrap_or_else(|error| {
                log::warn!("files: the access to all files could not be checked: {error}");
                false
            }))
    })
    .await
}

/// Leads to the system's setting for the files of this device (FR-039); the window checks again
/// when the person comes back. Nothing to do outside Android.
#[tauri::command]
pub async fn files_request_all_access(app: AppHandle) -> Result<(), FilesError> {
    blocking(move || {
        app.holzi_android()
            .request_all_files_access()
            .map_err(|error| {
                log::warn!("files: the setting for all files could not be opened: {error}");
                FilesError::new(
                    FilesErrorCode::Unsupported,
                    "the setting could not be opened",
                )
            })
    })
    .await
}

/// Every entry of a folder. A device folder shows its encrypted folders by name (spec 048 US7).
#[tauri::command]
pub async fn files_list(
    app: AppHandle,
    state: State<'_, AppState>,
    files: State<'_, FilesState>,
    source: SourceRef,
    path: String,
) -> Result<Vec<Entry>, FilesError> {
    if let Place::Storage {
        files: storage,
        path,
        device,
    } = place_of(&app, &state, &files, &source, &path, Want::Read).await?
    {
        let entries = storage.list(&path).await?;
        return Ok(entries
            .into_iter()
            .map(|entry| Place::entry(device.as_ref(), entry))
            .collect());
    }
    let real = device_path(&files, &source, &path, Want::Read)?;
    let own = files.own.clone();
    let folder = real.clone();
    let entries = blocking(move || ops::list(&folder, &own)).await?;
    let Some(root) = DeviceRoot::of(&real) else {
        return Ok(entries);
    };
    let has_encrypted = entries
        .iter()
        .any(|entry| entry.kind == EntryKind::Dir && is_folder_prefix(&entry.name));
    let Ok(storage) = device_storage(&app, &state, &files, &root) else {
        return Ok(entries);
    };
    match name_encrypted(&storage, &root, &real, entries.clone()).await {
        Ok(named) => Ok(named),
        Err(error) if has_encrypted => Err(error),
        // Only the cache could not be tidied; the folder still lists.
        Err(_) => Ok(entries),
    }
}

/// One entry.
#[tauri::command]
pub async fn files_stat(
    app: AppHandle,
    state: State<'_, AppState>,
    files: State<'_, FilesState>,
    source: SourceRef,
    path: String,
) -> Result<Entry, FilesError> {
    if let Place::Storage {
        files: storage,
        path,
        device,
    } = place_of(&app, &state, &files, &source, &path, Want::Read).await?
    {
        let entry = storage.stat(&path).await?;
        return Ok(Place::entry(device.as_ref(), entry));
    }
    let real = device_path(&files, &source, &path, Want::Read)?;
    let own = files.own.clone();
    blocking(move || ops::stat(&real, &own)).await
}

/// Watches the open folder (not its sub folders); every debounced batch goes to `channel`. In an
/// encrypted folder on the device the whole directory of that folder is watched and every change
/// reads as a change of the open folder (spec 048 research R17).
#[tauri::command]
pub async fn files_watch(
    app: AppHandle,
    state: State<'_, AppState>,
    files: State<'_, FilesState>,
    path: String,
    channel: Channel<FolderChanged>,
) -> Result<u64, FilesError> {
    let source = SourceRef::Device;
    let (real, recursive, report) =
        match place_of(&app, &state, &files, &source, &path, Want::Read).await? {
            Place::Storage {
                files: storage,
                path: view,
                device: Some(root),
            } => {
                let Route::Encrypted(at) = route(&storage, &view).await? else {
                    return Err(FilesError::new(FilesErrorCode::NotFound, "not found"));
                };
                (root.dir.join(&at.head.prefix), true, Some(path.clone()))
            }
            _ => (
                device_path(&files, &source, &path, Want::Read)?,
                false,
                None,
            ),
        };
    #[cfg(not(target_os = "ios"))]
    {
        let watch = crate::files::local::watch::watch_folder(&real, recursive, move |changes| {
            let paths = match &report {
                Some(open) => vec![open.clone()],
                None => changes
                    .into_iter()
                    .map(|change| change.path.to_string_lossy().into_owned())
                    .collect(),
            };
            let _ = channel.send(FolderChanged { paths });
        })
        .map_err(|reason| {
            log::warn!(
                "files: watching {} failed: {reason}",
                Redacted(&real.to_string_lossy())
            );
            FilesError::new(FilesErrorCode::Unsupported, "this folder cannot be watched")
        })?;
        Ok(files.keep_watch(watch))
    }
    #[cfg(target_os = "ios")]
    {
        let _ = (real, recursive, report, channel);
        Err(FilesError::new(
            FilesErrorCode::Unsupported,
            "no watching on this platform",
        ))
    }
}

/// Ends a watch of [`files_watch`].
#[tauri::command]
pub fn files_unwatch(files: State<'_, FilesState>, id: u64) {
    files.unwatch(id);
}
