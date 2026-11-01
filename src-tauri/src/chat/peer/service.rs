//! Starts the delivery of the chat with contacts with the vault session and ends it with the close
//! (spec 072 research R3): tracked work of the vault gate under its cancel token, like the sync.

use std::sync::Arc;

use tauri::{AppHandle, Manager, Runtime};

use super::delivery::Delivery;
use super::session::{Me, PeerRegistry, Session};
use super::targets;
use crate::identity::{installation_id_path, read_or_mint_installation_uuid};
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
