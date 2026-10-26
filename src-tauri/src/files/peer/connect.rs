//! One `haex-vault-files/1` connection per target device (spec 056 research R5): opened on the first
//! request, opened again once it ended, and gone with the session, which drops it on vault lock. Own
//! devices are found by their network id on the vault's device list, at the address presence noted
//! on the endpoint. A contact's device is dialed at what the contact's stored card names, its
//! network id and iroh relay, like the channels of spec 057 (FR-008); all shares it serves share
//! one connection.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use iroh::{EndpointAddr, EndpointId};

use crate::contacts::card::received as cards;
use crate::files::peer::client::{PeerLink, PeerLinks};
use crate::files::peer::handshake::DialError;
use crate::files::peer::serve::ServeContext;
use crate::files::peer::wire::Mode;
use crate::files::share::received::{self, ReceivedShare};
use crate::files::share::{PEER_PREFIX, SHARE_PREFIX};
use crate::remote_storage::StorageError;
use crate::sync::device_list;
use crate::sync::endpoint::SyncNode;
use crate::sync::foreign_auth::ForeignAuthError;
use crate::sync::keys::hex;
use crate::sync::presence::{decode_hex, endpoint_addr_of};

type Slot = Arc<tokio::sync::Mutex<Option<Arc<PeerLink>>>>;

/// The links of this session, one slot per target; a slot is held while its link opens, so a
/// device that does not answer keeps only its own requests waiting.
pub struct Connections {
    node: Arc<SyncNode>,
    ctx: Arc<ServeContext>,
    slots: Mutex<HashMap<String, Slot>>,
}

impl Connections {
    /// Links over `node`, proving this device as `ctx` does when it serves.
    pub fn new(node: Arc<SyncNode>, ctx: Arc<ServeContext>) -> Self {
        Self {
            node,
            ctx,
            slots: Mutex::new(HashMap::new()),
        }
    }

    /// Whether `target` turned this session away because its switch is off (FR-014), as its last
    /// handshake said; `false` for a target not tried yet or busy right now.
    pub fn access_off(&self, target: &str) -> bool {
        let slot = self
            .slots
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(target)
            .cloned();
        slot.and_then(|slot| {
            slot.try_lock().ok().and_then(|link| {
                link.as_ref()
                    .map(|link| link.welcome.mode == Mode::AccessOff)
            })
        })
        .unwrap_or(false)
    }

    /// Whether a contact's device at `endpoint` (hex) is linked right now.
    pub fn reaches(&self, endpoint: &str) -> bool {
        let slot = self
            .slots
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(&endpoint_key(endpoint))
            .cloned();
        slot.is_some_and(|slot| {
            slot.try_lock()
                .is_ok_and(|link| link.as_ref().is_some_and(|link| link.is_reusable()))
        })
    }

    fn slot(&self, key: &str) -> Slot {
        let mut slots = self.slots.lock().unwrap_or_else(|e| e.into_inner());
        Arc::clone(slots.entry(key.to_owned()).or_default())
    }

    /// The link in slot `key`, opened at `addr` when there is none that serves; `vault` is the
    /// vault the other side must prove for a contact.
    async fn open(
        &self,
        key: &str,
        addr: EndpointAddr,
        vault: Option<[u8; 32]>,
    ) -> Result<Arc<PeerLink>, StorageError> {
        let slot = self.slot(key);
        let mut slot = slot.lock().await;
        if let Some(link) = slot.as_ref().filter(|link| link.is_reusable()) {
            if vault.is_some_and(|vault| link.verified.vault != vault) {
                return Err(StorageError::MissingRight);
            }
            return Ok(Arc::clone(link));
        }
        let link = PeerLink::open(&self.node, &self.ctx.local(), addr)
            .await
            .map_err(dial_error)?;
        if vault.is_some_and(|vault| link.verified.vault != vault) {
            link.close();
            return Err(StorageError::MissingRight);
        }
        let link = Arc::new(link);
        *slot = Some(Arc::clone(&link));
        Ok(link)
    }

    /// The link to the device of `vault` at `addr`, a device its stored card names.
    pub async fn link_to_contact(
        &self,
        vault: [u8; 32],
        addr: EndpointAddr,
    ) -> Result<Arc<PeerLink>, StorageError> {
        let key = endpoint_key(&hex(addr.id.as_bytes()));
        self.open(&key, addr, Some(vault)).await
    }

    /// The received share `id`, the device serving it as its owner's card names it, and the
    /// owner's vault.
    async fn share_target(
        &self,
        id: &str,
    ) -> Result<(ReceivedShare, EndpointAddr, [u8; 32]), StorageError> {
        let id = id.to_owned();
        let found = self
            .ctx
            .db
            .read(move |q| {
                let Some(share) = received::get(q, &id)? else {
                    return Ok(None);
                };
                let Some(vault) = key_of(&share.owner_vault_pubkey) else {
                    return Ok(None);
                };
                if cards::is_blocked(q, &vault)? {
                    return Ok(None);
                }
                if cards::stored(q, &vault)?.is_none() {
                    return Ok(None);
                }
                // The devices of all stored cards of the vault, not of one (spec 060).
                let relay = cards::addresses(q, &vault)?
                    .devices
                    .iter()
                    .find(|device| hex(&device.endpoint_id) == share.owner_endpoint_id)
                    .map(|device| device.iroh_relay.clone());
                Ok(relay.map(|relay| (share, relay, vault)))
            })
            .await
            .map_err(|_| StorageError::Network)?
            .ok_or(StorageError::NotFound)?;
        let (share, relay, vault) = found;
        let endpoint = key_of(&share.owner_endpoint_id).ok_or(StorageError::NotFound)?;
        let addr = endpoint_addr_of(&endpoint, relay.as_deref(), &[])
            .map_err(|_| StorageError::Protocol)?;
        Ok((share, addr, vault))
    }

    /// The address of the own device `target`.
    async fn addr_of(&self, target: &str) -> Result<EndpointAddr, StorageError> {
        let device = target
            .strip_prefix(PEER_PREFIX)
            .ok_or(StorageError::Unsupported)?
            .to_owned();
        if device == self.ctx.device {
            return Err(StorageError::NotFound);
        }
        let vault = self.ctx.vault;
        let endpoint = self
            .ctx
            .db
            .read(move |q| {
                let rows = device_list::load_all(q)?;
                let valid = device_list::valid_lists(&rows, &vault);
                Ok(device_list::effective(&valid).and_then(|signed| {
                    signed
                        .list
                        .devices
                        .iter()
                        .find(|listed| listed.vault_device_uuid.to_string() == device)
                        .map(|listed| listed.endpoint_id)
                }))
            })
            .await
            .map_err(|_| StorageError::Network)?
            .ok_or(StorageError::NotFound)?;
        let id = EndpointId::from_bytes(&endpoint).map_err(|_| StorageError::Protocol)?;
        Ok(EndpointAddr::new(id))
    }
}

#[async_trait]
impl PeerLinks for Connections {
    async fn link(&self, target: &str) -> Result<Arc<PeerLink>, StorageError> {
        if let Some(id) = target.strip_prefix(SHARE_PREFIX) {
            let (share, addr, vault) = self.share_target(id).await?;
            return self
                .open(&endpoint_key(&share.owner_endpoint_id), addr, Some(vault))
                .await;
        }
        if !target.starts_with(PEER_PREFIX) {
            return Err(StorageError::Unsupported);
        }
        let addr = self.addr_of(target).await?;
        self.open(target, addr, None).await
    }
}

/// The slot of a contact's device, by its network id.
fn endpoint_key(endpoint: &str) -> String {
    format!("endpoint:{endpoint}")
}

/// 32 bytes from 64 hex characters.
pub fn key_of(text: &str) -> Option<[u8; 32]> {
    decode_hex(text).ok()?.try_into().ok()
}

/// A failed opening as the storage seam knows it: a refusal looks the same whatever its reason
/// (FR-042).
pub fn dial_error(error: DialError) -> StorageError {
    match error {
        DialError::Unreachable => StorageError::Network,
        DialError::Auth(ForeignAuthError::TimedOut) => StorageError::TimedOut,
        DialError::Auth(_) | DialError::Refused => StorageError::MissingRight,
        DialError::Version => StorageError::Unsupported,
    }
}

#[cfg(test)]
#[path = "connect_tests.rs"]
mod tests;
