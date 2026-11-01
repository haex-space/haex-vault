//! Who this device is in the chat with contacts while a vault is open, and the signals between the
//! commands and the delivery service (spec 072 research R3).

use std::sync::{Arc, Mutex, PoisonError};

use tokio::sync::Notify;

use crate::error::{HolziError, Result};
use crate::sync::keys::{hex, DeviceKeys};

/// This device in the open vault.
#[derive(Clone, Debug)]
pub struct Me {
    pub vault: [u8; 32],
    pub vault_hex: String,
    pub keys: DeviceKeys,
    /// This device's `vault_device_uuid` (`inserted_by` of the rows it writes).
    pub device_uuid: String,
}

impl Me {
    pub fn new(vault: [u8; 32], keys: DeviceKeys, device_uuid: String) -> Self {
        Self {
            vault,
            vault_hex: hex(&vault),
            keys,
            device_uuid,
        }
    }

    pub fn device_hex(&self) -> String {
        hex(&self.keys.device_pubkey)
    }

    pub fn receiver(&self) -> super::receive::Receiver<'_> {
        super::receive::Receiver {
            vault: &self.vault_hex,
            device_uuid: &self.device_uuid,
        }
    }
}

/// The running chat of the open vault.
#[derive(Debug)]
pub struct Session {
    pub me: Me,
    /// Something was queued: publish now instead of on the next tick.
    pub kick: Notify,
    /// The app came back or the network changed (spec 043): reconnect the Nostr-Relays.
    pub wake: Notify,
}

impl Session {
    pub fn new(me: Me) -> Arc<Self> {
        Arc::new(Self {
            me,
            kick: Notify::new(),
            wake: Notify::new(),
        })
    }
}

/// The session of the open vault, for the commands (managed state, one per process).
#[derive(Debug, Default)]
pub struct PeerRegistry {
    session: Mutex<Option<Arc<Session>>>,
}

impl PeerRegistry {
    pub fn set(&self, session: Option<Arc<Session>>) {
        *self.session.lock().unwrap_or_else(PoisonError::into_inner) = session;
    }

    pub fn get(&self) -> Option<Arc<Session>> {
        self.session
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// The session, or an error while the chat of the open vault has not started.
    pub fn require(&self) -> Result<Arc<Session>> {
        self.get().ok_or_else(|| HolziError::NotFound {
            name: "chat session".into(),
        })
    }

    /// Wakes the delivery after the app came back or the network changed.
    pub fn wake(&self) {
        if let Some(session) = self.get() {
            session.wake.notify_one();
        }
    }
}
