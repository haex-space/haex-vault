//! One `haex-vault-files/1` connection per target device (spec 056 research R5): opened on the first
//! request, opened again once it ended, and gone with the session, which drops it on vault lock. Own
//! devices are found by their network id on the vault's device list, at the address presence noted
//! on the endpoint; a received share needs the knock and answers `Unsupported` until then.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use iroh::{EndpointAddr, EndpointId};

use crate::files::peer::client::{PeerLink, PeerLinks};
use crate::files::peer::handshake::DialError;
use crate::files::peer::serve::ServeContext;
use crate::files::share::{PEER_PREFIX, SHARE_PREFIX};
use crate::remote_storage::StorageError;
use crate::sync::device_list;
use crate::sync::endpoint::SyncNode;
use crate::sync::foreign_auth::ForeignAuthError;

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

    fn slot(&self, target: &str) -> Slot {
        let mut slots = self.slots.lock().unwrap_or_else(|e| e.into_inner());
        Arc::clone(slots.entry(target.to_owned()).or_default())
    }

    /// The address of `target`.
    async fn addr_of(&self, target: &str) -> Result<EndpointAddr, StorageError> {
        if target.starts_with(SHARE_PREFIX) {
            // Contacts are found by the knock (US2).
            return Err(StorageError::Unsupported);
        }
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
        let slot = self.slot(target);
        let mut slot = slot.lock().await;
        if let Some(link) = slot.as_ref().filter(|link| link.is_reusable()) {
            return Ok(Arc::clone(link));
        }
        let addr = self.addr_of(target).await?;
        let link = PeerLink::open(&self.node, &self.ctx.local(), addr)
            .await
            .map_err(dial_error)?;
        let link = Arc::new(link);
        *slot = Some(Arc::clone(&link));
        Ok(link)
    }
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
