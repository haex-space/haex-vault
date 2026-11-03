//! Starts the delivery of the chat with contacts with the vault session and ends it with the close
//! (spec 072 research R3): tracked work of the vault gate under its cancel token, like the sync.

use std::sync::Arc;

use tauri::{AppHandle, Emitter, Manager, Runtime};

use super::delivery::Delivery;
use super::files::store::{self, ChatFileStore};
use super::files::transfers::{ChatFiles, TRANSFER_EVENT};
use super::session::{Me, PeerRegistry, Session};
use super::targets;
use crate::identity::{installation_id_path, read_or_mint_installation_uuid};
use crate::remote_storage::address::SystemResolver;
use crate::state::AppState;
use crate::storage::known_devices;
use crate::sync::keys;

/// Starts the chat of the vault `state` just opened. A failure is logged; the chat then stays
/// without delivery until the next open.
pub fn start_for_active_instance<R: Runtime>(app: &AppHandle<R>, state: &AppState) {
    let _ = app.manage(Arc::new(PeerRegistry::default()));
    let Some(registry) = app.try_state::<Arc<PeerRegistry>>() else {
        return;
    };
    let registry = Arc::clone(&registry);
    let db = match state.database() {
        Ok(db) => db,
        Err(error) => {
            log::warn!("chat: no open vault, no delivery: {error}");
            return;
        }
    };
    let installation = app
        .path()
        .app_local_data_dir()
        .map_err(|e| e.to_string())
        .and_then(|dir| {
            read_or_mint_installation_uuid(&installation_id_path(&dir)).map_err(|e| e.to_string())
        });
    let installation = match installation {
        Ok(installation) => installation,
        Err(error) => {
            log::warn!("chat: this installation is unknown, no delivery: {error}");
            return;
        }
    };
    let token = state.gate().token();
    let app = app.clone();
    let started = state.gate().spawn(async move {
        let found = db
            .read(move |r| {
                let keys = keys::load_device_keys(r, installation)?;
                let vault = keys::vault_pubkey(r)?;
                let device = known_devices::get_vault_device_uuid(r, installation)?;
                Ok((keys, vault, device, targets::listening_relays(r)?))
            })
            .await;
        let (keys, vault, device, relays) = match found {
            Ok((Some(keys), Some(vault), Some(device), relays)) => (keys, vault, device, relays),
            Ok(_) => {
                log::warn!("chat: no device keys, vault identity or device id yet, no delivery");
                return;
            }
            Err(error) => {
                log::warn!("chat: the delivery could not start: {error}");
                return;
            }
        };
        let me = Me::new(vault, keys, device.to_string());
        let session = Session::new(me.clone());
        // Spec 072 US3: this device's chat files, downloading beside the delivery until the close.
        match files(&app, &db, device) {
            Ok(files) => {
                tokio::spawn(Arc::clone(&files).run(token.clone()));
                let _ = session.files.set(files);
            }
            Err(error) => log::warn!("chat: no files on this device: {error}"),
        }
        registry.set(Some(Arc::clone(&session)));
        let device_uuid = uuid::Uuid::parse_str(&me.device_uuid).ok();
        match Delivery::connect(db.clone(), me, relays, token.clone()).await {
            Ok((delivery, incoming, forward)) => {
                // Spec 072 FR-023: the notifications run beside the delivery, until the close.
                if let Some(device) = device_uuid {
                    tokio::spawn(super::notify::run(
                        app.clone(),
                        db,
                        Arc::clone(&registry),
                        device,
                        delivery.arrivals(),
                        token,
                    ));
                }
                delivery.run(Arc::clone(&session), incoming, forward).await;
            }
            Err(error) => log::warn!("chat: the delivery did not connect: {error}"),
        }
        if registry
            .get()
            .is_some_and(|now| Arc::ptr_eq(&now, &session))
        {
            registry.set(None);
        }
    });
    if let Err(error) = started {
        log::warn!("chat: the delivery did not start: {error}");
    }
}

/// The file store of the open vault in `<app data>/chat-files/<instance>/`, reporting transfers to
/// the window (`chat-transfer`).
fn files<R: Runtime>(
    app: &AppHandle<R>,
    db: &crate::vault_gate::VaultDb,
    device: uuid::Uuid,
) -> crate::error::Result<Arc<ChatFiles>> {
    let not_found = |name: &str| crate::error::HolziError::NotFound { name: name.into() };
    let instance = app
        .state::<AppState>()
        .active_name()
        .ok()
        .flatten()
        .ok_or_else(|| not_found("open vault"))?;
    let data = app
        .path()
        .app_local_data_dir()
        .map_err(|_| not_found("app data folder"))?;
    let vault_app = app.clone();
    let store = ChatFileStore::new(
        &store::dir(&data, &instance),
        Arc::new(move || {
            vault_app.state::<AppState>().database().map_err(|_| {
                crate::files::FilesError::new(
                    crate::files::FilesErrorCode::Unsupported,
                    "no open vault",
                )
            })
        }),
    )?;
    let window = app.clone();
    Ok(Arc::new(ChatFiles {
        db: db.clone(),
        store,
        resolver: Arc::new(SystemResolver),
        progress: Some(Arc::new(move |progress| {
            let _ = window.emit(TRANSFER_EVENT, progress);
        })),
        wake: tokio::sync::Notify::new(),
        device,
    }))
}
