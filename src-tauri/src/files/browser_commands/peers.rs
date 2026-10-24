//! The other devices of the vault in the file browser (spec 056 US1, FR-011): each with its name
//! and whether it can be opened now, and the watch of a folder there (FR-046).

use std::collections::HashSet;
use std::sync::Arc;

use serde::Serialize;
use tauri::ipc::Channel;
use tauri::{AppHandle, Manager};
use tokio::time::{Duration, Instant};
use ts_rs::TS;

use super::navigate::{FolderChanged, KnownPlace};
use super::{peer_links, remote_access};
use crate::files::peer::contacts::{self, FilesContactShares};
use crate::files::peer::store::{peer_access, PeerStore};
use crate::files::peer::wire::{read_head, Caller, Reply, WireRoot};
use crate::files::share::PEER_PREFIX;
use crate::files::state::FilesState;
use crate::files::storage_source::{folder_prefix, storage_error};
use crate::files::FilesError;
use crate::instances::paths::get_app_local_data;
use crate::state::AppState;
use crate::sync::device_view;
use crate::sync::registry::SyncRegistry;

/// Whether another device can be opened now.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub enum FilesPeerState {
    /// A session with it is open.
    Reachable,
    Unreachable,
    /// It turned this device away: its file access is switched off (FR-014).
    AccessOff,
}

/// Another device of the vault for the sidebar.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct FilesPeerDevice {
    /// The storage id `peer:<vault_device_uuid>`.
    pub id: String,
    /// `None` for a device without a name yet.
    pub name: Option<String>,
    pub state: FilesPeerState,
}

/// Every other device of the effective list; empty without an open vault or a sync service.
pub async fn peer_devices(app: &AppHandle, state: &AppState) -> Vec<FilesPeerDevice> {
    let Some(runtime) = app
        .try_state::<Arc<SyncRegistry>>()
        .and_then(|registry| registry.get())
    else {
        return Vec::new();
    };
    let (Ok(db), Ok(local_data)) = (state.database(), get_app_local_data(app)) else {
        return Vec::new();
    };
    let Ok(installation) = crate::identity::read_or_mint_installation_uuid(
        &crate::identity::installation_id_path(&local_data),
    ) else {
        return Vec::new();
    };
    let connected: HashSet<[u8; 32]> = runtime.node.connected().into_iter().collect();
    let devices = match db
        .read(move |r| device_view::load(r, installation, &connected))
        .await
    {
        Ok(devices) => devices,
        Err(error) => {
            log::warn!("files: the other devices could not be read: {error}");
            return Vec::new();
        }
    };
    devices
        .into_iter()
        .filter(|device| !device.is_current)
        .map(|device| {
            let id = format!("{PEER_PREFIX}{}", device.vault_device_uuid);
            let off = runtime
                .files
                .as_ref()
                .is_some_and(|links| links.access_off(&id));
            let state = match (off, device.online) {
                (true, _) => FilesPeerState::AccessOff,
                (false, true) => FilesPeerState::Reachable,
                (false, false) => FilesPeerState::Unreachable,
            };
            FilesPeerDevice {
                id,
                name: device.alias,
                state,
            }
        })
        .collect()
}

/// How long another device may take to start a watch.
const WATCH_START: Duration = Duration::from_secs(10);

/// Watches the folder `path` of the other device `storage_id`; every change, and the end of the
/// watch (the device went away, its access was switched off), goes to `channel` as a change of
/// `path`, so the window reloads it and shows what holds now.
pub async fn watch_peer(
    app: &AppHandle,
    files: &FilesState,
    storage_id: &str,
    path: String,
    channel: Channel<FolderChanged>,
) -> Result<u64, FilesError> {
    let store = PeerStore::new(peer_links(app)?, Caller::User);
    let access = remote_access(&app.state::<AppState>(), storage_id).await?;
    let mut changes = store
        .watch(
            &access,
            &folder_prefix(&path)?,
            Instant::now() + WATCH_START,
        )
        .await
        .map_err(storage_error)?;
    let task = tauri::async_runtime::spawn(async move {
        let changed = || {
            let _ = channel.send(FolderChanged {
                paths: vec![path.clone()],
            });
        };
        while let Ok(Reply::Changed) = read_head(&mut changes).await {
            changed();
        }
        changed();
    });
    Ok(files.keep_peer_watch(task))
}

/// Where another own device can be opened: its drives and known places, by window path.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct FilesPeerRoots {
    pub drives: Vec<KnownPlace>,
    pub known: Vec<KnownPlace>,
}

/// How long another device may take to name its places.
const ROOTS_LIMIT: Duration = Duration::from_secs(10);

/// The drives and known places of the other device `storage_id`, so the window opens it at its
/// home folder as it opens this device (spec 056 US1).
#[tauri::command]
pub async fn files_peer_roots(
    app: AppHandle,
    storage_id: String,
) -> Result<FilesPeerRoots, FilesError> {
    let store = PeerStore::new(peer_links(&app)?, Caller::User);
    let (drives, known) = store
        .roots(
            &peer_access(&storage_id, None),
            Instant::now() + ROOTS_LIMIT,
        )
        .await
        .map_err(storage_error)?;
    let places = |roots: Vec<WireRoot>| {
        roots
            .into_iter()
            .map(|root| KnownPlace {
                name: root.name,
                path: root.path,
            })
            .collect()
    };
    Ok(FilesPeerRoots {
        drives: places(drives),
        known: places(known),
    })
}

/// Asks every device of the contact `contact_id` for the shares it gave this vault (spec 056 US2,
/// FR-028): only when the person expands the contact.
#[tauri::command]
pub async fn files_contact_query(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    contact_id: String,
) -> Result<FilesContactShares, FilesError> {
    let db = state
        .database()
        .map_err(|_| FilesError::new(crate::files::FilesErrorCode::Unsupported, "no open vault"))?;
    let links = peer_links(&app)?;
    contacts::query(&db, &links, contact_id).await
}

/// The contacts with a stored card and what is known of their shares, for the sidebar (FR-031).
pub async fn contact_shares(app: &AppHandle, state: &AppState) -> Vec<FilesContactShares> {
    let Ok(db) = state.database() else {
        return Vec::new();
    };
    let links = peer_links(app).ok();
    contacts::known(&db, links.as_deref())
        .await
        .unwrap_or_else(|error| {
            log::warn!("files: the contacts' shares could not be listed: {error}");
            Vec::new()
        })
}
