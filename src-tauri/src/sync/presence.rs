//! Presence over Nostr, so devices of the same vault find each other and
//! connect on their own (spec 024, FR-007, FR-008, FR-010,
//! contracts/nostr-events.md).
//!
//! A presence meeting follows NIP-59's three layers, but with a shared
//! *mailbox* key pair instead of one recipient's real identity, so every
//! device that holds the vault's current content key can open it, and with
//! real (not randomized-backward) timestamps, since these events are
//! ephemeral (kind 21059) and never stored by a relay:
//!
//! - gift wrap (kind 21059): signed by a fresh, one-time key; `p`-tag the
//!   mailbox pubkey `mb_pk`; content = NIP-44 v2 of the seal, encrypted to
//!   `mb_pk`.
//! - seal (kind 13): signed by the sender's real device key; content =
//!   NIP-44 v2 of the inner event, *also* encrypted to `mb_pk` (not the
//!   sender-recipient pair NIP-59 normally uses) — the ECDH shared secret is
//!   the same computed from either side (`ECDH(sender_sk, mb_pk) ==
//!   ECDH(mb_sk, sender_pk)`), so any device that can derive `mb_sk` can
//!   open it, regardless of which specific device sent it.
//! - inner event (kind 24100, unsigned): its own content is the presence
//!   JSON below.
//!
//! `mb_pk`/`mb_sk` are derived from the vault's current content key and
//! today's (or yesterday's) date, so they rotate daily without any of the
//! devices agreeing on it out of band.

use std::net::SocketAddr;
use std::time::{SystemTime, UNIX_EPOCH};

use haex_crdt::rusqlite::params;
use haex_crdt::CrdtTransaction;
use hkdf::Hkdf;
use iroh::{EndpointAddr, RelayUrl, TransportAddr};
use nostr::event::{Event, Kind};
use nostr::key::{Keys, PublicKey, SecretKey};
use serde::{Deserialize, Serialize};
use sha2::Sha256;

use crate::storage::query::Query;
use crate::sync::gift_wrap::{unwrap_shaped, wrap_shaped, WrapShape};
use crate::sync::keys::{random_bytes, DeviceKeys};

/// A presence meeting is never sent or kept over this size (contracts/
/// nostr-events.md).
pub(crate) const MAX_EVENT_BYTES: usize = 16 * 1024;
/// How stale a meeting's `ts` may be.
const MAX_AGE_MS: u64 = 150_000;
/// How far into the future a meeting's `ts` may claim to be.
const MAX_FUTURE_MS: u64 = 30_000;
/// The presence event kind (contracts/nostr-events.md).
pub(crate) const PRESENCE_KIND: Kind = Kind::Custom(24100);
/// The gift-wrap kind holzi uses (not NIP-59's standard 1059: presence is
/// ephemeral and never stored, so it uses the ephemeral range instead).
pub(crate) const GIFT_WRAP_KIND: Kind = Kind::Custom(21059);

#[derive(Debug, thiserror::Error)]
pub enum PresenceError {
    #[error("the event is over the {MAX_EVENT_BYTES}-byte limit")]
    TooLarge,
    #[error("not a valid secp256k1 key")]
    InvalidKey,
    #[error("the gift wrap or seal does not decrypt or verify: {0}")]
    Crypto(String),
    #[error("the payload does not decode: {0}")]
    Malformed(String),
    #[error("the inner event is not a presence meeting")]
    WrongKind,
}

impl From<PresenceError> for haex_crdt::Error {
    /// Carries a presence failure through a storage transaction as a consumer error.
    fn from(error: PresenceError) -> Self {
        haex_crdt::Error::consumer(error.to_string())
    }
}

/// The presence content (contracts/nostr-events.md): this device's current
/// reachability.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PresenceContent {
    pub v: u8,
    #[serde(with = "hex_bytes32")]
    pub device: [u8; 32],
    #[serde(with = "hex_bytes32")]
    pub endpoint: [u8; 32],
    #[serde(skip_serializing_if = "Option::is_none")]
    pub iroh_relay: Option<String>,
    pub addrs: Vec<String>,
    pub list_generation: u64,
    pub ts: u64,
    #[serde(with = "hex_nonce")]
    pub nonce: [u8; 16],
}

impl PresenceContent {
    /// Builds this device's own current presence content.
    pub fn own(
        device_pubkey: [u8; 32],
        endpoint_id: [u8; 32],
        iroh_relay: Option<String>,
        addrs: Vec<SocketAddr>,
        list_generation: u64,
    ) -> Self {
        Self {
            v: 1,
            device: device_pubkey,
            endpoint: endpoint_id,
            iroh_relay,
            addrs: addrs.iter().map(SocketAddr::to_string).collect(),
            list_generation,
            ts: now_ms(),
            nonce: *random_bytes::<16>(),
        }
    }

    /// Whether `ts` is fresh enough to act on (contracts/nostr-events.md).
    pub fn is_fresh(&self, now_ms: u64) -> bool {
        let age = now_ms.saturating_sub(self.ts);
        let future = self.ts.saturating_sub(now_ms);
        age <= MAX_AGE_MS && future <= MAX_FUTURE_MS
    }

    /// The [`EndpointAddr`] this meeting names, for [`iroh::address_lookup::MemoryLookup`].
    pub fn endpoint_addr(&self) -> Result<EndpointAddr, PresenceError> {
        endpoint_addr_of(&self.endpoint, self.iroh_relay.as_deref(), &self.addrs)
    }
}

/// The [`EndpointAddr`] of `endpoint` reachable at the relay and the socket
/// addresses a meeting names; an address that does not parse is left out.
pub(crate) fn endpoint_addr_of(
    endpoint: &[u8; 32],
    iroh_relay: Option<&str>,
    addrs: &[String],
) -> Result<EndpointAddr, PresenceError> {
    let id = iroh::EndpointId::from_bytes(endpoint).map_err(|_| PresenceError::InvalidKey)?;
    let mut addrs: Vec<TransportAddr> = addrs
        .iter()
        .filter_map(|a| a.parse::<SocketAddr>().ok())
        .map(TransportAddr::Ip)
        .collect();
    if let Some(url) = iroh_relay.and_then(|relay| relay.parse::<RelayUrl>().ok()) {
        addrs.push(TransportAddr::Relay(url));
    }
    Ok(EndpointAddr::from_parts(id, addrs))
}

/// Wall-clock milliseconds since the Unix epoch; 0 if the clock is before it.
pub(crate) fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Days since 1970-01-01 UTC, for today's and yesterday's mailbox tag.
pub fn day_tag_now() -> u32 {
    day_tag_at(now_ms())
}

/// Days since 1970-01-01 UTC at `now_ms`, the mailbox rotation step.
fn day_tag_at(now_ms: u64) -> u32 {
    (now_ms / 86_400_000) as u32
}

/// The mailbox key pair for `day_tag`, derived from the vault's content key
/// (contracts/nostr-events.md): `HKDF-SHA256(ikm = content_key, info =
/// "holzi/presence/v1" ‖ tag_u32 BE)`.
pub fn mailbox_keys(
    content_key: &[u8; 32],
    day_tag: u32,
) -> Result<(SecretKey, PublicKey), PresenceError> {
    let hkdf = Hkdf::<Sha256>::new(None, content_key);
    let mut info = b"holzi/presence/v1".to_vec();
    info.extend_from_slice(&day_tag.to_be_bytes());
    let mut okm = [0u8; 32];
    hkdf.expand(&info, &mut okm)
        .expect("32 bytes are a valid HKDF-SHA256 output length");
    let secret = SecretKey::from_slice(&okm).map_err(|_| PresenceError::InvalidKey)?;
    let public = Keys::new(secret.clone()).public_key();
    Ok((secret, public))
}

/// A raw device secret as a Nostr secret key.
pub(crate) fn nostr_secret(secret: &[u8; 32]) -> Result<SecretKey, PresenceError> {
    SecretKey::from_slice(secret).map_err(|_| PresenceError::InvalidKey)
}

/// A raw x-only pubkey as a Nostr public key.
fn nostr_public(pubkey: &[u8; 32]) -> Result<PublicKey, PresenceError> {
    PublicKey::from_slice(pubkey).map_err(|_| PresenceError::InvalidKey)
}

/// Builds the gift-wrapped presence meeting ready to publish.
pub fn build(
    sender: &DeviceKeys,
    content: &PresenceContent,
    mb_pk: &PublicKey,
) -> Result<Event, PresenceError> {
    let json =
        serde_json::to_string(content).map_err(|e| PresenceError::Malformed(e.to_string()))?;
    wrap_payload(PRESENCE_KIND, &sender.device_secret, &json, mb_pk)
}

/// Opens a received gift-wrapped meeting: decrypts both layers, verifies the
/// seal's signature, and returns the authenticated sender device pubkey
/// (contracts/nostr-events.md — the caller still has to check it against the
/// effective device list) together with the presence content.
pub fn open(
    gift_wrap: &Event,
    mb_sk: &SecretKey,
) -> Result<([u8; 32], PresenceContent), PresenceError> {
    let (sender, json) = unwrap_payload(gift_wrap, mb_sk, PRESENCE_KIND)?;
    let content: PresenceContent =
        serde_json::from_str(&json).map_err(|e| PresenceError::Malformed(e.to_string()))?;
    Ok((sender, content))
}

/// The three layers of a meeting (gift wrap, seal, unsigned inner event of
/// `inner_kind` carrying `payload`), sealed by `sender_secret` for the shared
/// mailbox key `recipient`. Presence and the link rendezvous use the same
/// layers with different inner kinds (contracts/nostr-events.md).
pub(crate) fn wrap_payload(
    inner_kind: Kind,
    sender_secret: &[u8; 32],
    payload: &str,
    recipient: &PublicKey,
) -> Result<Event, PresenceError> {
    wrap_shaped(
        WrapShape::PRESENCE,
        inner_kind,
        sender_secret,
        payload,
        recipient,
    )
}

/// Opens a gift wrap made by [`wrap_payload`]: decrypts both layers with
/// `mb_sk`, verifies the seal's signature and that the inner event has
/// `inner_kind`, and returns the seal signer with the inner payload.
pub(crate) fn unwrap_payload(
    gift_wrap: &Event,
    mb_sk: &SecretKey,
    inner_kind: Kind,
) -> Result<([u8; 32], String), PresenceError> {
    let (sender, kind, payload) = unwrap_rumor(gift_wrap, mb_sk)?;
    if kind != inner_kind {
        return Err(PresenceError::WrongKind);
    }
    Ok((sender, payload))
}

/// Opens a gift wrap like [`unwrap_payload`] without knowing the inner kind
/// in advance: returns the seal signer, the inner event's kind and its
/// payload, for a mailbox that carries presence and admission requests.
pub(crate) fn unwrap_rumor(
    gift_wrap: &Event,
    mb_sk: &SecretKey,
) -> Result<([u8; 32], Kind, String), PresenceError> {
    unwrap_shaped(gift_wrap, mb_sk, MAX_EVENT_BYTES)
}

/// Checks a device pubkey is a real secp256k1 key, for callers that only
/// have a raw `[u8; 32]` (e.g. from the device list) and need a
/// [`PublicKey`] to build a filter or verify against.
pub fn public_key(pubkey: &[u8; 32]) -> Result<PublicKey, PresenceError> {
    nostr_public(pubkey)
}

/// Records that `device_pubkey` was seen just now, reachable at
/// `endpoint_addr` (or with no address, if the meeting named none this
/// device could parse). An earlier `problem` stays: a device that is
/// reachable can still be one this device must not sync with, and only a
/// successful handshake clears it.
pub fn record_seen(
    tx: &mut CrdtTransaction<'_>,
    device_pubkey: &[u8; 32],
    last_seen_ms: u64,
    endpoint_addr: Option<&EndpointAddr>,
) -> haex_crdt::Result<()> {
    let encoded = endpoint_addr
        .map(postcard::to_stdvec)
        .transpose()
        .map_err(|e| haex_crdt::Error::consumer(format!("endpoint addr: {e}")))?;
    let last_seen = i64::try_from(last_seen_ms)
        .map_err(|_| haex_crdt::Error::consumer("last_seen beyond i64"))?;
    tx.execute(
        "INSERT INTO device_presence_no_sync (device_pubkey, last_seen, endpoint_addr, problem) \
         VALUES (?1, ?2, ?3, NULL) \
         ON CONFLICT(device_pubkey) DO UPDATE SET \
           last_seen = excluded.last_seen, endpoint_addr = excluded.endpoint_addr",
        params![device_pubkey.as_slice(), last_seen, encoded],
    )?;
    Ok(())
}

/// One device's presence row, as stored.
pub struct StoredPresence {
    pub device_pubkey: [u8; 32],
    pub last_seen_ms: u64,
    pub endpoint_addr: Option<EndpointAddr>,
}

/// Every device this device has ever recorded presence for.
pub fn load_all(q: &mut impl Query) -> haex_crdt::Result<Vec<StoredPresence>> {
    let rows: Vec<(Vec<u8>, i64, Option<Vec<u8>>)> = q.query_map(
        "SELECT device_pubkey, last_seen, endpoint_addr FROM device_presence_no_sync",
        &[],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
    )?;
    rows.into_iter()
        .map(|(device_pubkey, last_seen, endpoint_addr)| {
            let device_pubkey: [u8; 32] = device_pubkey
                .try_into()
                .map_err(|_| haex_crdt::Error::consumer("device_presence: malformed pubkey"))?;
            let endpoint_addr = endpoint_addr
                .map(|bytes| postcard::from_bytes(&bytes))
                .transpose()
                .map_err(|e| haex_crdt::Error::consumer(format!("endpoint addr: {e}")))?;
            Ok(StoredPresence {
                device_pubkey,
                last_seen_ms: u64::try_from(last_seen).unwrap_or(0),
                endpoint_addr,
            })
        })
        .collect()
}

/// Installs `ring` as the process-wide rustls crypto provider, unless one is
/// already installed. The relay client's `wss://` websocket asks rustls for
/// that default, and with both `ring` (iroh) and `aws-lc-rs` (reqwest) in
/// the dependency tree rustls cannot choose on its own and panics instead.
pub fn ensure_crypto_provider() {
    if rustls::crypto::CryptoProvider::get_default().is_none() {
        // A concurrent install winning the race is just as good.
        let _ = rustls::crypto::ring::default_provider().install_default();
    }
}

pub(crate) mod hex_bytes32 {
    use serde::{Deserialize, Deserializer, Serializer};

    /// Writes the 32 bytes as lowercase hex.
    pub fn serialize<S: Serializer>(bytes: &[u8; 32], s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&crate::sync::keys::hex(bytes))
    }

    /// Reads 32 bytes from a hex string.
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<[u8; 32], D::Error> {
        let text = String::deserialize(d)?;
        let bytes = super::decode_hex(&text).map_err(serde::de::Error::custom)?;
        bytes
            .try_into()
            .map_err(|_| serde::de::Error::custom("expected 32 bytes"))
    }
}

mod hex_nonce {
    use serde::{Deserialize, Deserializer, Serializer};

    /// Writes the 16-byte nonce as lowercase hex.
    pub fn serialize<S: Serializer>(bytes: &[u8; 16], s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&crate::sync::keys::hex(bytes))
    }

    /// Reads a 16-byte nonce from a hex string.
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<[u8; 16], D::Error> {
        let text = String::deserialize(d)?;
        let bytes = super::decode_hex(&text).map_err(serde::de::Error::custom)?;
        bytes
            .try_into()
            .map_err(|_| serde::de::Error::custom("expected 16 bytes"))
    }
}

/// Decodes an even-length hex string (either case).
pub(crate) fn decode_hex(text: &str) -> Result<Vec<u8>, &'static str> {
    if !text.is_ascii() || !text.len().is_multiple_of(2) {
        return Err("not an even-length hex string");
    }
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&text[i..i + 2], 16).map_err(|_| "not hex"))
        .collect()
}

#[cfg(test)]
#[path = "presence_tests.rs"]
mod tests;
