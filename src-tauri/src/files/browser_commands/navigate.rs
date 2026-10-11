//! Sources, folders and watching (spec 044 US1).

use serde::Serialize;
use tauri::ipc::Channel;
use tauri::State;
use ts_rs::TS;

use super::{blocking, device_path, storage_files, storage_of};
use crate::files::access::Want;
use crate::files::local::drives::{drives, Drive};
use crate::files::local::ops;
use crate::files::state::FilesState;
use crate::files::{Entry, FilesError, FilesErrorCode, SourceRef};
use crate::state::AppState;

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

/// Every entry of a folder.
#[tauri::command]
pub async fn files_list(
    state: State<'_, AppState>,
    files: State<'_, FilesState>,
    source: SourceRef,
    path: String,
) -> Result<Vec<Entry>, FilesError> {
    if let Some(id) = storage_of(&source) {
        return storage_files(&state, id).await?.list(&path).await;
    }
    let real = device_path(&files, &source, &path, Want::Read)?;
    let own = files.own.clone();
    blocking(move || ops::list(&real, &own)).await
}

/// One entry.
#[tauri::command]
pub async fn files_stat(
    state: State<'_, AppState>,
    files: State<'_, FilesState>,
    source: SourceRef,
    path: String,
) -> Result<Entry, FilesError> {
    if let Some(id) = storage_of(&source) {
        return storage_files(&state, id).await?.stat(&path).await;
    }
    let real = device_path(&files, &source, &path, Want::Read)?;
    let own = files.own.clone();
    blocking(move || ops::stat(&real, &own)).await
}

/// Watches the open folder (not its sub folders); every debounced batch goes to `channel`.
#[tauri::command]
pub fn files_watch(
    files: State<'_, FilesState>,
    path: String,
    channel: Channel<FolderChanged>,
) -> Result<u64, FilesError> {
    let real = device_path(&files, &SourceRef::Device, &path, Want::Read)?;
    #[cfg(not(target_os = "ios"))]
    {
        let watch = crate::files::local::watch::watch_folder(&real, false, move |changes| {
            let paths = changes
                .into_iter()
                .map(|change| change.path.to_string_lossy().into_owned())
                .collect();
            let _ = channel.send(FolderChanged { paths });
        })
        .map_err(|reason| {
            log::warn!("files: watching {} failed: {reason}", real.display());
            FilesError::new(FilesErrorCode::Unsupported, "this folder cannot be watched")
        })?;
        Ok(files.keep_watch(watch))
    }
    #[cfg(target_os = "ios")]
    {
        let _ = (real, channel);
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
