//! Content keys of the scope "vault" (spec 024, FR-015, research R9).
//!
//! A content key is 32 random bytes with `key_id = SHA-256("holzi-key-id/v1" ‖
//! key)[0..16]` and a generation. A generation (`vault_key_generations`) is
//! bound to the device list that issues it and authorized by a main device of
//! that list. The key itself only travels in envelopes (`vault_key_envelopes`):
//! NIP-44 v2 from the issuing main device's device key to each listed device's
//! device key, each envelope authorized as well. A device unwraps its own
//! envelope into the device-local `vault_content_keys_no_sync`.
//!
//! `created_by` and `sender` are data only: a generation or an envelope counts
//! only if its authorization verifies with the key of a device the referenced
//! valid list names as a main device. So no linked device can issue either.
//!
//! In spec 024 the key seals the device names in the device list and derives
//! the presence mailbox (R7).
//!
//! Maintainability exception (spaex 500-LoC rule): issuing, verifying and unwrapping share the
//! signed records and their checks, so they have stayed together; sealed names already live in
//! `content_keys_names.rs`. Concrete split plan, if this grows further: move the selection of
//! held keys (`current_key`, `current_key_for_list`, `listening_keys`) into
//! `content_keys_held.rs`, leaving issuing and the envelopes here.

use std::collections::BTreeMap;

use haex_crdt::rusqlite::params;
use haex_crdt::CrdtTransaction;
use nostr::nips::nip44;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

use crate::storage::query::Query;
use crate::sync::device_list::SignedList;
use crate::sync::keys::{random_bytes, DeviceKeys};
use crate::sync::signing::{self, Domain};

/// The scope of every content key in spec 024.
pub const SCOPE_VAULT: &str = "vault";

const NONCE_LEN: usize = 24;

/// One generation of the content key.
pub struct ContentKey {
    pub key_id: [u8; 16],
    pub generation: u64,
    pub key: Zeroizing<[u8; 32]>,
}

impl std::fmt::Debug for ContentKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ContentKey")
            .field("key_id", &crate::sync::keys::hex(&self.key_id))
            .field("generation", &self.generation)
            .finish_non_exhaustive()
    }
}

impl ContentKey {
    /// A fresh key of `generation`.
    pub fn generate(generation: u64) -> Self {
        let key = random_bytes::<32>();
        Self {
            key_id: key_id(&key),
            generation,
            key,
        }
    }
}

/// `SHA-256("holzi-key-id/v1" ‖ key)[0..16]`.
pub fn key_id(key: &[u8; 32]) -> [u8; 16] {
    let digest: [u8; 32] = Sha256::new()
        .chain_update(b"holzi-key-id/v1")
        .chain_update(key)
        .finalize()
        .into();
    digest[..16].try_into().expect("16 of 32 bytes")
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum KeyError {
    #[error("the referenced device list is not valid")]
    UnknownList,
    #[error("no main device of the referenced list authorized this")]
    Unauthorized,
    #[error("a stored value has the wrong length")]
    Malformed,
    #[error("the envelope cannot be opened: {0}")]
    Envelope(String),
    #[error("the envelope holds another key than its row names")]
    KeyMismatch,
    #[error("the sealed name cannot be opened")]
    SealedName,
}

impl From<KeyError> for haex_crdt::Error {
    fn from(err: KeyError) -> Self {
        haex_crdt::Error::consumer(err)
    }
}

/// The canonical record a generation's authorization signs.
#[derive(Serialize)]
struct GenerationRecord<'a> {
    key_id: [u8; 16],
    scope: &'a str,
    generation: u64,
    created_by: [u8; 32],
    created_at: u64,
    device_list_hash: [u8; 32],
}

/// The canonical record an envelope's authorization signs.
#[derive(Serialize)]
pub(crate) struct EnvelopeRecord<'a> {
    pub(crate) key_id: [u8; 16],
    pub(crate) generation: u64,
    pub(crate) recipient: [u8; 32],
    pub(crate) sender: [u8; 32],
    pub(crate) envelope: &'a str,
    pub(crate) device_list_hash: [u8; 32],
}

/// What an envelope carries.
#[derive(Serialize, Deserialize)]
struct EnvelopeContent {
    scope: String,
    generation: u64,
    key_id: String,
    key: String,
}

/// Issues generation `key` for the devices of `list`: stores the authorized
/// generation, one authorized envelope per listed device and, since the issuer
/// is listed too, the issuer's own unwrapped key. `issuer` must be a main
/// device of `list`.
pub fn issue_generation(
    tx: &mut CrdtTransaction<'_>,
    key: &ContentKey,
    list: &SignedList,
    issuer: &DeviceKeys,
    created_at: u64,
) -> haex_crdt::Result<()> {
    let record = GenerationRecord {
        key_id: key.key_id,
        scope: SCOPE_VAULT,
        generation: key.generation,
        created_by: issuer.device_pubkey,
        created_at,
        device_list_hash: list.hash,
    };
    let authorization = sign(Domain::KeyGeneration, &record, &issuer.device_secret)?;
    tx.execute(
        "INSERT OR IGNORE INTO vault_key_generations \
           (key_id, scope, generation, created_by, created_at, device_list_hash, authorization) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            key.key_id.as_slice(),
            SCOPE_VAULT,
            to_i64(key.generation)?,
            issuer.device_pubkey.as_slice(),
            to_i64(created_at)?,
            list.hash.as_slice(),
            authorization.as_slice(),
        ],
    )?;
    for device in &list.list.devices {
        let envelope = wrap(key, issuer, &device.device_pubkey)?;
        let record = EnvelopeRecord {
            key_id: key.key_id,
            generation: key.generation,
            recipient: device.device_pubkey,
            sender: issuer.device_pubkey,
            envelope: &envelope,
            device_list_hash: list.hash,
        };
        let authorization = sign(Domain::KeyEnvelope, &record, &issuer.device_secret)?;
        tx.execute(
            "INSERT OR IGNORE INTO vault_key_envelopes \
               (key_id, recipient, sender, envelope, device_list_hash, authorization) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                key.key_id.as_slice(),
                device.device_pubkey.as_slice(),
                issuer.device_pubkey.as_slice(),
                envelope,
                list.hash.as_slice(),
                authorization.as_slice(),
            ],
        )?;
    }
    if list.list.device(&issuer.device_pubkey).is_some() {
        store_own_key(tx, key)?;
    }
    Ok(())
}

/// A `vault_key_generations` row as read.
#[derive(Debug, Clone)]
pub struct StoredGeneration {
    pub key_id: Vec<u8>,
    pub scope: String,
    pub generation: i64,
    pub created_by: Vec<u8>,
    pub created_at: i64,
    pub device_list_hash: Vec<u8>,
    pub authorization: Vec<u8>,
}

/// A `vault_key_envelopes` row as read.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoredEnvelope {
    pub key_id: Vec<u8>,
    pub recipient: Vec<u8>,
    pub sender: Vec<u8>,
    pub envelope: String,
    pub device_list_hash: Vec<u8>,
    pub authorization: Vec<u8>,
}

impl StoredGeneration {
    /// Checks that a main device of the referenced valid list authorized this
    /// generation, and returns that list.
    pub fn verify<'a>(
        &self,
        valid: &'a BTreeMap<[u8; 32], SignedList>,
    ) -> Result<&'a SignedList, KeyError> {
        let list_hash: [u8; 32] = fixed(&self.device_list_hash)?;
        let list = valid.get(&list_hash).ok_or(KeyError::UnknownList)?;
        let record = GenerationRecord {
            key_id: fixed(&self.key_id)?,
            scope: &self.scope,
            generation: u64::try_from(self.generation).map_err(|_| KeyError::Malformed)?,
            created_by: fixed(&self.created_by)?,
            created_at: u64::try_from(self.created_at).map_err(|_| KeyError::Malformed)?,
            device_list_hash: list_hash,
        };
        authorized_by_main(Domain::KeyGeneration, &record, &self.authorization, list)?;
        Ok(list)
    }
}

impl StoredEnvelope {
    /// Checks that a main device of the referenced valid list authorized this
    /// envelope for `generation`.
    pub fn verify(
        &self,
        generation: u64,
        valid: &BTreeMap<[u8; 32], SignedList>,
    ) -> Result<(), KeyError> {
        let list_hash: [u8; 32] = fixed(&self.device_list_hash)?;
        let list = valid.get(&list_hash).ok_or(KeyError::UnknownList)?;
        let record = EnvelopeRecord {
            key_id: fixed(&self.key_id)?,
            generation,
            recipient: fixed(&self.recipient)?,
            sender: fixed(&self.sender)?,
            envelope: &self.envelope,
            device_list_hash: list_hash,
        };
        authorized_by_main(Domain::KeyEnvelope, &record, &self.authorization, list)
    }
}

/// Unwraps every authorized envelope addressed to `own` whose key this device
/// does not hold yet, and stores it. Returns how many keys were added.
pub fn unwrap_own_envelopes(
    tx: &mut CrdtTransaction<'_>,
    own: &DeviceKeys,
    valid: &BTreeMap<[u8; 32], SignedList>,
) -> haex_crdt::Result<usize> {
    let generations = load_generations(tx)?;
    let envelopes: Vec<StoredEnvelope> = tx.query_map(
        "SELECT e.key_id, e.recipient, e.sender, e.envelope, e.device_list_hash, e.authorization \
         FROM vault_key_envelopes e \
         WHERE e.recipient = ?1 \
           AND NOT EXISTS (SELECT 1 FROM vault_content_keys_no_sync k WHERE k.key_id = e.key_id)",
        params![own.device_pubkey.as_slice()],
        envelope_row,
    )?;
    let mut added = 0;
    for envelope in envelopes {
        let Some(generation) = generations.iter().find(|g| g.key_id == envelope.key_id) else {
            continue;
        };
        if generation.verify(valid).is_err() {
            continue;
        }
        let generation_number = generation_list_generation(generation)?;
        if envelope.verify(generation_number, valid).is_err() {
            continue;
        }
        let key = match unwrap(&envelope, own) {
            Ok(key) => key,
            Err(error) => {
                log::warn!("sync: an authorized envelope does not open: {error}");
                continue;
            }
        };
        if key.key_id.as_slice() != envelope.key_id.as_slice()
            || key.generation != generation_number
        {
            log::warn!("sync: {}", KeyError::KeyMismatch);
            continue;
        }
        store_own_key(tx, &key)?;
        added += 1;
    }
    Ok(added)
}

/// The key this device holds of the highest generation that is wrapped for no
/// device in `removed` (research R9). `None` when this device holds no key.
pub fn current_key(
    q: &mut impl Query,
    removed: &[[u8; 32]],
) -> haex_crdt::Result<Option<ContentKey>> {
    current_key_for_list(q, removed, None)
}

/// The highest held key issued for `list_hash` whose envelopes do not name a
/// removed device. A same-generation fork can leave two different keys in a
/// replica; preferring the effective list's key avoids selecting a key from
/// the losing fork when both generations are otherwise equal.
pub fn current_key_for_list(
    q: &mut impl Query,
    removed: &[[u8; 32]],
    list_hash: Option<&[u8; 32]>,
) -> haex_crdt::Result<Option<ContentKey>> {
    let mut held: Vec<(Vec<u8>, i64, Vec<u8>)> = if let Some(hash) = list_hash {
        q.query_map(
            "SELECT k.key_id, k.generation, k.key \
             FROM vault_content_keys_no_sync k \
             JOIN vault_key_generations g ON g.key_id = k.key_id \
             WHERE g.device_list_hash = ?1 \
                OR EXISTS (SELECT 1 FROM vault_key_envelopes e \
                           WHERE e.key_id = k.key_id AND e.device_list_hash = ?1) \
             ORDER BY k.generation DESC",
            params![hash.as_slice()],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )?
    } else {
        q.query_map(
            "SELECT key_id, generation, key FROM vault_content_keys_no_sync \
             ORDER BY generation DESC",
            &[],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )?
    };
    // Legacy databases may not have a generation explicitly tied to the current
    // list yet. Fall back to the complete local key set in that case.
    if held.is_empty() && list_hash.is_some() {
        held = q.query_map(
            "SELECT key_id, generation, key FROM vault_content_keys_no_sync \
             ORDER BY generation DESC",
            &[],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )?;
    }
    for (key_id, generation, key) in held {
        let key = Zeroizing::new(key);
        let recipients: Vec<Vec<u8>> = q.query_map(
            "SELECT recipient FROM vault_key_envelopes WHERE key_id = ?1",
            params![key_id],
            |r| r.get(0),
        )?;
        if recipients
            .iter()
            .any(|recipient| removed.iter().any(|r| r.as_slice() == recipient.as_slice()))
        {
            continue;
        }
        return Ok(Some(ContentKey {
            key_id: fixed(&key_id)?,
            generation: u64::try_from(generation).map_err(|_| KeyError::Malformed)?,
            key: Zeroizing::new(fixed(&key)?),
        }));
    }
    Ok(None)
}

/// The keys this device holds besides `current`: up to `limit` of a lower
/// generation, newest first, then up to `limit` of a higher one, which
/// `current_key` skipped for a removed recipient. Presence only listens with
/// them (contracts/nostr-events.md): a device that was away while the key
/// changed still announces itself with a lower one, and one that has not yet
/// heard of a removal with a skipped higher one.
pub fn listening_keys(
    q: &mut impl Query,
    current: &ContentKey,
    limit: usize,
) -> haex_crdt::Result<Vec<Zeroizing<[u8; 32]>>> {
    let generation = i64::try_from(current.generation).map_err(|_| KeyError::Malformed)?;
    let limit = i64::try_from(limit).unwrap_or(i64::MAX);
    let mut held: Vec<Vec<u8>> = q.query_map(
        "SELECT key FROM vault_content_keys_no_sync WHERE generation < ?1 \
         ORDER BY generation DESC LIMIT ?2",
        params![generation, limit],
        |r| r.get(0),
    )?;
    held.extend(q.query_map(
        "SELECT key FROM vault_content_keys_no_sync WHERE generation > ?1 \
         ORDER BY generation ASC LIMIT ?2",
        params![generation, limit],
        |r| r.get(0),
    )?);
    held.into_iter()
        .map(|key| {
            let key = Zeroizing::new(key);
            Ok(Zeroizing::new(fixed(&key)?))
        })
        .collect()
}

/// Serializes the vault content key and its metadata into a JSON payload and
/// encrypts it for `recipient` with NIP-44 v2 using the sender's device secret.
pub(crate) fn wrap(
    key: &ContentKey,
    sender: &DeviceKeys,
    recipient: &[u8; 32],
) -> haex_crdt::Result<String> {
    let content = serde_json::to_string(&EnvelopeContent {
        scope: SCOPE_VAULT.to_string(),
        generation: key.generation,
        key_id: crate::sync::keys::hex(&key.key_id),
        key: crate::sync::keys::hex(key.key.as_slice()),
    })
    .expect("an envelope content always serializes");
    let content = Zeroizing::new(content);
    let secret = nostr_secret(&sender.device_secret)?;
    let recipient = nostr::key::PublicKey::from_slice(recipient)
        .map_err(|e| KeyError::Envelope(e.to_string()))?;
    nip44::encrypt(&secret, &recipient, content.as_bytes(), nip44::Version::V2)
        .map_err(|e| KeyError::Envelope(e.to_string()).into())
}

/// Decrypts an envelope with this device's secret and decodes its content key,
/// checking the vault scope and that the payload's key id matches the key.
/// The caller must verify authorization and match the key to the stored row.
fn unwrap(envelope: &StoredEnvelope, own: &DeviceKeys) -> haex_crdt::Result<ContentKey> {
    let secret = nostr_secret(&own.device_secret)?;
    let sender = nostr::key::PublicKey::from_slice(&envelope.sender)
        .map_err(|e| KeyError::Envelope(e.to_string()))?;
    let plaintext = Zeroizing::new(
        nip44::decrypt_to_bytes(&secret, &sender, envelope.envelope.as_bytes())
            .map_err(|e| KeyError::Envelope(e.to_string()))?,
    );
    let content: EnvelopeContent =
        serde_json::from_slice(&plaintext).map_err(|e| KeyError::Envelope(e.to_string()))?;
    let key = Zeroizing::new(content.key);
    let key: [u8; 32] = fixed(&from_hex(&key)?)?;
    let key = Zeroizing::new(key);
    if content.scope != SCOPE_VAULT || from_hex(&content.key_id)? != key_id(&key) {
        return Err(KeyError::KeyMismatch.into());
    }
    Ok(ContentKey {
        key_id: key_id(&key),
        generation: content.generation,
        key,
    })
}

/// Stores an unwrapped key in device-local storage, leaving an existing row
/// with the same key id unchanged. Rows are never deleted: encrypted folders
/// (spec 048, `files::encrypted::vault_keys`) unwrap their folder key with the
/// generation they were made under, however old it is.
fn store_own_key(tx: &mut CrdtTransaction<'_>, key: &ContentKey) -> haex_crdt::Result<()> {
    tx.execute(
        "INSERT OR IGNORE INTO vault_content_keys_no_sync (key_id, generation, key) \
         VALUES (?1, ?2, ?3)",
        params![
            key.key_id.as_slice(),
            to_i64(key.generation)?,
            key.key.as_slice()
        ],
    )?;
    Ok(())
}

/// Every stored generation, unchecked.
pub fn load_generations(q: &mut impl Query) -> haex_crdt::Result<Vec<StoredGeneration>> {
    q.query_map(
        "SELECT key_id, scope, generation, created_by, created_at, device_list_hash, \
                authorization \
         FROM vault_key_generations",
        &[],
        |r| {
            Ok(StoredGeneration {
                key_id: r.get(0)?,
                scope: r.get(1)?,
                generation: r.get(2)?,
                created_by: r.get(3)?,
                created_at: r.get(4)?,
                device_list_hash: r.get(5)?,
                authorization: r.get(6)?,
            })
        },
    )
}

fn envelope_row(r: &haex_crdt::rusqlite::Row<'_>) -> haex_crdt::rusqlite::Result<StoredEnvelope> {
    Ok(StoredEnvelope {
        key_id: r.get(0)?,
        recipient: r.get(1)?,
        sender: r.get(2)?,
        envelope: r.get(3)?,
        device_list_hash: r.get(4)?,
        authorization: r.get(5)?,
    })
}

fn generation_list_generation(generation: &StoredGeneration) -> Result<u64, KeyError> {
    u64::try_from(generation.generation).map_err(|_| KeyError::Malformed)
}

/// Verifies `signature` over `record` with each main device of `list` in turn.
fn authorized_by_main<T: Serialize>(
    domain: Domain,
    record: &T,
    signature: &[u8],
    list: &SignedList,
) -> Result<(), KeyError> {
    let signature: [u8; 64] = fixed(signature)?;
    let bytes = signing::canonical(record).map_err(|_| KeyError::Malformed)?;
    let authorized = list
        .list
        .devices
        .iter()
        .filter(|device| device.role == crate::sync::device_list::Role::Main)
        .any(|device| signing::verify(domain, &bytes, &signature, &device.device_pubkey).is_ok());
    if authorized {
        Ok(())
    } else {
        Err(KeyError::Unauthorized)
    }
}

pub(crate) fn sign<T: Serialize>(
    domain: Domain,
    record: &T,
    secret: &[u8; 32],
) -> haex_crdt::Result<[u8; 64]> {
    let bytes = signing::canonical(record).map_err(haex_crdt::Error::consumer)?;
    signing::sign(domain, &bytes, secret).map_err(haex_crdt::Error::consumer)
}

fn nostr_secret(secret: &[u8; 32]) -> Result<nostr::key::SecretKey, KeyError> {
    nostr::key::SecretKey::from_slice(secret).map_err(|e| KeyError::Envelope(e.to_string()))
}

pub(crate) fn fixed<const N: usize>(bytes: &[u8]) -> Result<[u8; N], KeyError> {
    bytes.try_into().map_err(|_| KeyError::Malformed)
}

fn from_hex(hex: &str) -> Result<Vec<u8>, KeyError> {
    if !hex.is_ascii() || !hex.len().is_multiple_of(2) {
        return Err(KeyError::Malformed);
    }
    (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).map_err(|_| KeyError::Malformed))
        .collect()
}

fn to_i64(value: u64) -> haex_crdt::Result<i64> {
    i64::try_from(value).map_err(|_| haex_crdt::Error::consumer("value beyond i64"))
}

#[path = "content_keys_names.rs"]
mod names;
pub use names::{open_name, seal_name, sealed_name_key_id};

#[cfg(test)]
#[path = "content_keys_tests.rs"]
mod tests;
