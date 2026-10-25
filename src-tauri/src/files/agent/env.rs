//! What the executors of the file actions need from the running app: holzi's own places, the
//! stored grants, the storages and the question to the user. [`TauriEnv`] reads them from the
//! managed states; tests put in their own.

use std::path::Path;

use async_trait::async_trait;
use tauri::{AppHandle, Emitter, Manager, Runtime};

use super::prompt::{FilesAgentPermissionRequest, PermissionPrompt, EVENT_PERMISSION_REQUEST};
use super::share_confirm::{FileShareConfirmRequest, ShareConfirm, EVENT_SHARE_CONFIRM};
use super::shares::SharesEnv;
use crate::error::HolziError;
use crate::files::access::AgentGrants;
use crate::files::browser_commands::peers::{contact_shares, peer_devices};
use crate::files::browser_commands::{
    device_storage, is_remote, peer_storage, window_storage, KnownPlace, StorageSource,
};
use crate::files::encrypted::device::DeviceRoot;
use crate::files::local::OwnPlaces;
use crate::files::peer::wire::Caller as PeerCaller;
use crate::files::permissions::{
    self, AgentFileKind, AgentFilePermission, AgentFileReach, AgentFileStatus,
};
use crate::files::share::received;
use crate::files::share::{FileShareLevel, SHARE_PREFIX};
use crate::files::state::FilesState;
use crate::files::storage_source::StorageFiles;
use crate::files::{FilesError, FilesErrorCode};
use crate::state::AppState;

#[async_trait]
pub trait AgentEnv: Send + Sync {
    /// holzi's own places, closed to every agent (FR-033).
    fn own(&self) -> OwnPlaces;

    /// The known places of the device (`files.sources`).
    fn known(&self) -> Vec<KnownPlace>;

    async fn grants(&self, agent_id: &str) -> Result<AgentGrants, FilesError>;

    /// Stores the user's answer for a storage.
    async fn store_grant(
        &self,
        agent_id: &str,
        storage_id: &str,
        status: AgentFileStatus,
    ) -> Result<(), FilesError>;

    /// Stores the user's answer for an encrypted folder, given with "Erlaubnis merken" (spec 048
    /// FR-031); `target` is `<storage_id>/<folder_id>`.
    async fn store_folder_grant(
        &self,
        agent_id: &str,
        target: &str,
        status: AgentFileStatus,
        reach: AgentFileReach,
    ) -> Result<(), FilesError>;

    /// Every storage of the vault, granted or not.
    async fn storages(&self) -> Result<Vec<StorageSource>, FilesError>;

    async fn open_storage(&self, storage_id: &str) -> Result<StorageFiles, FilesError>;

    /// Whether `storage_id` is a share a contact gave this vault for reading only: no agent writes
    /// there, whatever its own grant (spec 056 FR-041).
    async fn read_only(&self, _storage_id: &str) -> bool {
        false
    }

    /// The device as a storage for the absolute device path `path`: `/`, or its drive on Windows
    /// (spec 048 US7).
    fn device_root(&self, path: &Path) -> Option<DeviceRoot> {
        DeviceRoot::of(path)
    }

    /// The device root of the storage id `device:/` or `device:C:`.
    fn device_root_of(&self, storage_id: &str) -> Option<DeviceRoot> {
        DeviceRoot::from_id(storage_id)
    }

    fn prompt(&self) -> &PermissionPrompt;
}

/// The environment of the app.
pub struct TauriEnv<R: Runtime> {
    app: AppHandle<R>,
    prompt: PermissionPrompt,
}

impl<R: Runtime> TauriEnv<R> {
    /// Sends the question to the windows, if there is one (FR-034: without one nobody is asked).
    pub fn new(app: AppHandle<R>, prompt: PermissionPrompt) -> Self {
        let emitting = app.clone();
        prompt.set_emitter(std::sync::Arc::new(
            move |request: &FilesAgentPermissionRequest| {
                !emitting.webview_windows().is_empty()
                    && emitting.emit(EVENT_PERMISSION_REQUEST, request).is_ok()
            },
        ));
        Self { app, prompt }
    }

    fn vault(&self) -> Result<crate::vault_gate::VaultDb, FilesError> {
        open_vault(&self.app.state::<AppState>())
    }
}

/// The open vault's database.
pub(super) fn open_vault(state: &AppState) -> Result<crate::vault_gate::VaultDb, FilesError> {
    state
        .database()
        .map_err(|_| FilesError::new(FilesErrorCode::Unsupported, "no open vault"))
}

/// A database error, without its details.
pub(super) fn vault_error(error: HolziError) -> FilesError {
    log::warn!("files: the permissions of agents could not be read or written: {error}");
    FilesError::new(FilesErrorCode::Unsupported, "the vault could not be read")
}

#[async_trait]
impl<R: Runtime> AgentEnv for TauriEnv<R> {
    fn own(&self) -> OwnPlaces {
        self.app.state::<FilesState>().own.clone()
    }

    fn known(&self) -> Vec<KnownPlace> {
        self.app
            .state::<FilesState>()
            .known
            .iter()
            .map(|place| KnownPlace {
                name: place.name.to_owned(),
                path: place.path.to_string_lossy().into_owned(),
            })
            .collect()
    }

    async fn grants(&self, agent_id: &str) -> Result<AgentGrants, FilesError> {
        let agent_id = agent_id.to_owned();
        self.vault()?
            .read(move |q| Ok(permissions::grants_of(q, &agent_id)?))
            .await
            .map_err(vault_error)
    }

    async fn store_grant(
        &self,
        agent_id: &str,
        storage_id: &str,
        status: AgentFileStatus,
    ) -> Result<(), FilesError> {
        let row = AgentFilePermission {
            agent_id: agent_id.to_owned(),
            kind: AgentFileKind::Storage,
            target: storage_id.to_owned(),
            status,
            updated_at: crate::passwords::clock::unix_millis(std::time::SystemTime::now()),
            reach: None,
        };
        self.vault()?
            .write(move |tx| Ok(permissions::set(tx, &row)?))
            .await
            .map_err(vault_error)
    }

    async fn store_folder_grant(
        &self,
        agent_id: &str,
        target: &str,
        status: AgentFileStatus,
        reach: AgentFileReach,
    ) -> Result<(), FilesError> {
        let row = AgentFilePermission {
            agent_id: agent_id.to_owned(),
            kind: AgentFileKind::EncryptedFolder,
            target: target.to_owned(),
            status,
            updated_at: crate::passwords::clock::unix_millis(std::time::SystemTime::now()),
            reach: Some(reach),
        };
        self.vault()?
            .write(move |tx| Ok(permissions::set(tx, &row)?))
            .await
            .map_err(vault_error)
    }

    async fn storages(&self) -> Result<Vec<StorageSource>, FilesError> {
        let state = self.app.state::<AppState>();
        let service = crate::remote_storage::commands::service(&state)
            .map_err(|_| FilesError::new(FilesErrorCode::Unsupported, "no open vault"))?;
        let overview = service.overview().await.map_err(vault_error)?;
        let mut storages: Vec<StorageSource> = overview
            .storages
            .into_iter()
            .map(|storage| StorageSource {
                id: storage.id,
                name: storage.name,
            })
            .collect();
        // Spec 056: the other devices of the vault and the shares contacts gave it, by their names.
        for device in peer_devices(&self.app, &state).await {
            let name = device.name.unwrap_or_else(|| device.id.clone());
            storages.push(StorageSource {
                id: device.id,
                name,
            });
        }
        for contact in contact_shares(&self.app, &state).await {
            storages.extend(contact.shares.into_iter().map(|share| StorageSource {
                id: share.storage_id,
                name: share.name,
            }));
        }
        Ok(storages)
    }

    async fn read_only(&self, storage_id: &str) -> bool {
        let Some(id) = storage_id.strip_prefix(SHARE_PREFIX) else {
            return false;
        };
        let (Ok(db), id) = (self.vault(), id.to_owned()) else {
            return true;
        };
        // Unknown or unreadable: no write goes out.
        db.read(move |q| Ok(received::get(q, &id)?))
            .await
            .ok()
            .flatten()
            .is_none_or(|share| share.level == FileShareLevel::Read)
    }

    async fn open_storage(&self, storage_id: &str) -> Result<StorageFiles, FilesError> {
        // The device as a storage, for its encrypted folders (spec 048 US7).
        if let Some(root) = self.device_root_of(storage_id) {
            return device_storage(
                &self.app,
                &self.app.state::<AppState>(),
                &self.app.state::<FilesState>(),
                &root,
            );
        }
        // Another own device answers an agent as one (FR-007: holzi's own data stays closed).
        if is_remote(storage_id) {
            return peer_storage(
                &self.app,
                &self.app.state::<AppState>(),
                storage_id,
                PeerCaller::Agent,
            )
            .await;
        }
        window_storage(
            &self.app,
            &self.app.state::<AppState>(),
            &self.app.state::<FilesState>(),
            storage_id,
        )
        .await
    }

    fn prompt(&self) -> &PermissionPrompt {
        &self.prompt
    }
}

/// What the executor of the share actions needs from the app (spec 056 US6).
pub struct TauriSharesEnv {
    app: AppHandle,
    confirm: ShareConfirm,
}

impl TauriSharesEnv {
    /// Sends the confirmation to the windows, if there is one (no window: refused).
    pub fn new(app: AppHandle, confirm: ShareConfirm) -> Self {
        let emitting = app.clone();
        confirm.set_emitter(std::sync::Arc::new(
            move |request: &FileShareConfirmRequest| {
                !emitting.webview_windows().is_empty()
                    && emitting.emit(EVENT_SHARE_CONFIRM, request).is_ok()
            },
        ));
        Self { app, confirm }
    }
}

#[async_trait]
impl SharesEnv for TauriSharesEnv {
    async fn vault(&self) -> Result<(crate::vault_gate::VaultDb, String), FilesError> {
        let db = open_vault(&self.app.state::<AppState>())?;
        let device = crate::device::commands::resolve_vault_device_uuid(&self.app, &db)
            .await
            .map_err(vault_error)?;
        Ok((db, device.to_string()))
    }

    fn own(&self) -> OwnPlaces {
        self.app.state::<FilesState>().own.clone()
    }

    fn confirm(&self) -> &ShareConfirm {
        &self.confirm
    }
}
