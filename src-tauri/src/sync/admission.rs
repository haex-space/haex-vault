//! Copies of the vault file and their admission (spec 024, user story 7,
//! FR-044, FR-045, research R12, R20).
//!
//! A copy opens with keys of its own (genesis creates them; the source's row
//! stays untouched). A copy of a main device holds the vault secret and
//! enrolls itself as a main device ([`enroll_as_main`]). A copy of a linked
//! device cannot sign a device list: it asks the devices of the vault to
//! admit it with a request signed by its new device key ([`Request`]), sent
//! with its presence. Every listed device that receives a valid request keeps
//! it in the synchronized `admission_requests` ([`store`]), so it reaches
//! every main device. A main device admits it ([`decide`]) into a new device
//! list with the content keys wrapped for it, or refuses it; nothing admits a
//! copy without that decision (FR-045).
//!
//! The request carries the copy's `vault_device_uuid` (its CRDT node id)
//! beyond what contracts/nostr-events.md first listed, because the device
//! list names a device by it and the changes the copy made while it waited
//! carry it. Its transient reachability (`iroh_relay`, `addrs`) is not part
//! of the signature and is never stored.

use std::collections::HashSet;

use haex_crdt::rusqlite::params;
use haex_crdt::CrdtTransaction;
use iroh::{EndpointAddr, RelayUrl};
use nostr::event::Kind;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::storage::query::Query;
use crate::sync::device_list::SignedList;
use crate::sync::keys::DeviceKeys;
use crate::sync::presence::hex_bytes32;
use crate::sync::signing::{self, lp, Domain};

/// The device-scoped preference that tells the user, once, that this copy
/// enrolled itself as a main device (FR-044).
pub const PREF_ENROLLED_AS_MAIN: &str = "sync.copy.enrolled_as_main";
/// The inner event kind of a request (contracts/nostr-events.md).
pub(crate) const ADMISSION_KIND: Kind = Kind::Custom(24101);
/// Open requests kept after a merge; the rest are refused (R20).
pub const OPEN_LIMIT: usize = 20;
/// An unanswered or refused request is dropped after this long (R20).
pub const MAX_AGE_MS: u64 = 30 * 24 * 60 * 60 * 1000;
/// How far ahead of this device's clock a request may claim to be.
const MAX_FUTURE_MS: u64 = 30_000;
/// The longest name a request may carry.
const MAX_NAME_BYTES: usize = 256;

const OPEN: &str = "open";
const REJECTED: &str = "rejected";

#[derive(Debug, thiserror::Error)]
pub enum AdmissionError {
    /// Only a main device, which holds the vault secret, admits or refuses.
    #[error("this device is not a main device")]
    NotMainDevice,
    #[error("there is no open request from this device")]
    NoRequest,
    #[error("the vault has no valid device list")]
    NoDeviceList,
    #[error("this device holds no content key to wrap for the new device")]
    NoContentKey,
    #[error(transparent)]
    Crdt(#[from] haex_crdt::Error),
    #[error("a database task failed: {0}")]
    Join(#[from] tokio::task::JoinError),
}

impl From<AdmissionError> for crate::error::HaexError {
    fn from(error: AdmissionError) -> Self {
        use crate::error::HaexError;
        match error {
            AdmissionError::NotMainDevice => HaexError::NotMainDevice,
            AdmissionError::Crdt(error) => error.into(),
            AdmissionError::Join(error) => HaexError::CrdtInit {
                reason: format!("admission task: {error}"),
            },
            other => HaexError::InvalidInput {
                reason: other.to_string(),
            },
        }
    }
}

/// A copy's request to be admitted, with the signature of its device key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    pub device: [u8; 32],
    pub endpoint: [u8; 32],
    pub vault_device_uuid: Uuid,
    pub name: String,
    pub requested_at: u64,
    pub signature: [u8; 64],
}

impl Request {
    /// A request signed by `keys`.
    pub fn sign(
        keys: &DeviceKeys,
        vault_device_uuid: Uuid,
        name: String,
        requested_at: u64,
    ) -> Result<Self, signing::SigningError> {
        let bytes = signed_bytes(
            &keys.device_pubkey,
            &keys.endpoint_id,
            &vault_device_uuid,
            &name,
            requested_at,
        );
        let signature = signing::sign(Domain::Admission, &bytes, &keys.device_secret)?;
        Ok(Self {
            device: keys.device_pubkey,
            endpoint: keys.endpoint_id,
            vault_device_uuid,
            name,
            requested_at,
            signature,
        })
    }

    /// Whether the signature is the device's own over exactly these fields.
    pub fn verify(&self) -> Result<(), signing::SigningError> {
        let bytes = signed_bytes(
            &self.device,
            &self.endpoint,
            &self.vault_device_uuid,
            &self.name,
            self.requested_at,
        );
        signing::verify(Domain::Admission, &bytes, &self.signature, &self.device)
    }
}

/// `lp(device) ‖ lp(endpoint) ‖ lp(vault_device_uuid) ‖ lp(name) ‖ requested_at`.
fn signed_bytes(
    device: &[u8; 32],
    endpoint: &[u8; 32],
    vault_device_uuid: &Uuid,
    name: &str,
    requested_at: u64,
) -> Vec<u8> {
    [
        lp(device),
        lp(endpoint),
        lp(vault_device_uuid.as_bytes()),
        lp(name.as_bytes()),
        requested_at.to_be_bytes().to_vec(),
    ]
    .concat()
}

/// The JSON of the inner event (contracts/nostr-events.md).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Content {
    pub v: u8,
    #[serde(with = "hex_bytes32")]
    pub device: [u8; 32],
    #[serde(with = "hex_bytes32")]
    pub endpoint: [u8; 32],
    pub vault_device_uuid: Uuid,
    pub name: String,
    pub requested_at: u64,
    #[serde(with = "hex_bytes64")]
    pub sig: [u8; 64],
    /// Where the copy can be reached right now: not signed, not stored.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub iroh_relay: Option<String>,
    #[serde(default)]
    pub addrs: Vec<String>,
}

impl Content {
    /// The event content for `request`, reachable at `addr`.
    pub fn new(request: &Request, addr: &EndpointAddr) -> Self {
        Self {
            v: 1,
            device: request.device,
            endpoint: request.endpoint,
            vault_device_uuid: request.vault_device_uuid,
            name: request.name.clone(),
            requested_at: request.requested_at,
            sig: request.signature,
            iroh_relay: addr.relay_urls().next().map(RelayUrl::to_string),
            addrs: addr.ip_addrs().map(|a| a.to_string()).collect(),
        }
    }

    /// The request this content carries, once `sender` (the seal's signer) is
    /// the device it names and its fields are sane, fresh and signed.
    pub fn request(&self, sender: &[u8; 32], now_ms: u64) -> Option<Request> {
        if self.v != 1
            || &self.device != sender
            || self.name.len() > MAX_NAME_BYTES
            || self.requested_at > now_ms.saturating_add(MAX_FUTURE_MS)
            || now_ms.saturating_sub(self.requested_at) > MAX_AGE_MS
        {
            return None;
        }
        let request = Request {
            device: self.device,
            endpoint: self.endpoint,
            vault_device_uuid: self.vault_device_uuid,
            name: self.name.clone(),
            requested_at: self.requested_at,
            signature: self.sig,
        };
        request.verify().ok()?;
        Some(request)
    }

    /// The address the copy announced, for dialing it once it is admitted.
    pub fn endpoint_addr(&self) -> Option<EndpointAddr> {
        crate::sync::presence::endpoint_addr_of(
            &self.endpoint,
            self.iroh_relay.as_deref(),
            &self.addrs,
        )
        .ok()
    }
}

mod hex_bytes64 {
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(bytes: &[u8; 64], s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&crate::sync::keys::hex(bytes))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<[u8; 64], D::Error> {
        let text = String::deserialize(d)?;
        let bytes = crate::sync::presence::decode_hex(&text).map_err(serde::de::Error::custom)?;
        bytes
            .try_into()
            .map_err(|_| serde::de::Error::custom("expected 64 bytes"))
    }
}

/// Devices the effective list names or has removed: requests from them are
/// settled and never shown.
pub fn settled(list: &SignedList) -> HashSet<[u8; 32]> {
    list.list
        .devices
        .iter()
        .map(|d| d.device_pubkey)
        .chain(list.list.removed.iter().map(|r| r.device_pubkey))
        .collect()
}

#[path = "admission_decision.rs"]
mod decision;
#[path = "admission_store.rs"]
mod store_impl;
pub use decision::{decide, enroll_as_main, Decided};
pub use store_impl::{load_open, store, sweep, sweep_now, OpenRequest};

/// This device's name for a request or a list entry: its alias, or the
/// computer's name while the alias is still the default.
pub fn own_name(q: &mut impl Query, vault_device_uuid: Uuid) -> haex_crdt::Result<String> {
    let alias: Option<Option<String>> = q.query_row(
        "SELECT alias FROM known_devices WHERE vault_device_uuid = ?1",
        params![vault_device_uuid.to_string()],
        |r| r.get(0),
    )?;
    let alias = alias
        .flatten()
        .map(|alias| alias.trim().to_string())
        .filter(|alias| !alias.is_empty() && alias != crate::sync::genesis::FALLBACK_DEVICE_NAME);
    Ok(alias
        .or_else(crate::hardware::hostname::suggested_alias)
        .unwrap_or_else(|| crate::sync::genesis::FALLBACK_DEVICE_NAME.to_string()))
}

/// Gives a copy the computer's name as its alias while the alias is still the
/// default, so the copy and its source do not both show as "haex-vault". A name the
/// user chose stays; without a computer name nothing changes.
pub fn adopt_computer_name(
    tx: &mut CrdtTransaction<'_>,
    vault_device_uuid: Uuid,
) -> haex_crdt::Result<()> {
    let Some(name) = crate::hardware::hostname::suggested_alias() else {
        return Ok(());
    };
    tx.execute(
        "UPDATE known_devices SET alias = ?1 \
         WHERE vault_device_uuid = ?2 AND (alias IS NULL OR alias = ?3)",
        params![
            name,
            vault_device_uuid.to_string(),
            crate::sync::genesis::FALLBACK_DEVICE_NAME
        ],
    )?;
    Ok(())
}

/// Whether this device has to ask to be admitted: it is neither on the
/// effective list nor removed by it.
pub fn needs_request(effective: &SignedList, own: &[u8; 32]) -> bool {
    effective.list.device(own).is_none() && !effective.list.removes(own)
}

#[cfg(test)]
#[path = "admission_decision_tests.rs"]
mod decision_tests;
#[cfg(test)]
#[path = "admission_store_tests.rs"]
mod store_tests;
#[cfg(test)]
#[path = "admission_tests.rs"]
mod tests;
