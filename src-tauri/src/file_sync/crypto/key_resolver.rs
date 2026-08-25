//! Round B — shared-space key resolution for the file envelope.
//!
//! **Scope:** shared-space (MLS epoch) keys only. Given a `(space_id, epoch)`
//! pair, look up the matching AEAD key from `haex_mls_sync_keys` and cache it
//! for the process lifetime. The own-vault branch — file-content encryption
//! under the vault key rather than an epoch key — is *not* handled here: the
//! caller decides which key path to take before invoking this module. That
//! split keeps this file a pure DB + cache concern and leaves vault-key
//! transport (TS → Rust via a Tauri command) as a Round D wiring problem.
//!
//! ## Why the open path never falls back
//!
//! The envelope header carries the epoch it was sealed under. `resolve_key`
//! looks up **exactly** that epoch; a missing row surfaces
//! [`KeyError::EpochNotFound`] rather than substituting the current epoch's
//! key. Silent fallback would break historical confidentiality: an epoch that
//! is no longer accessible (member left, key material pruned locally, DB
//! corruption) would be papered over with the wrong key and the reader would
//! get either garbage or an [`file_sync::crypto::chunk::CryptoError::OpenFailed`]
//! that is impossible to diagnose from logs.
//!
//! ## Cache
//!
//! Keys for a given `(space_id, epoch)` pair are immutable — MLS derives the
//! same epoch key from the same group state, and the in-place UPDATE path in
//! `mls_export_epoch_key` only rewrites the same bytes to survive a partial
//! failure. Caching is therefore correct without an invalidation hook.
//!
//! The cache lock uses `unwrap_or_else(|e| e.into_inner())` — same rationale as
//! [`file_sync::hashing`]'s `HASH_CACHE`: a poisoned lock here only means a
//! previous panic occurred while inserting derived data. Recomputing from the
//! DB on the next call is correct and cheap.

use std::collections::HashMap;
use std::sync::Mutex;

use base64::Engine;
use lazy_static::lazy_static;
use serde_json::Value as JsonValue;

use crate::database::core::select_with_crdt;
use crate::database::error::DatabaseError;
use crate::database::DbConnection;

/// AEAD key length (XChaCha20Poly1305 uses a 256-bit key).
pub const KEY_LEN: usize = 32;

/// Errors surfaced by the key resolver. Kept flat and non-`Copy` so callers
/// can attach context strings without wrapping.
#[derive(Debug, thiserror::Error)]
pub enum KeyError {
    #[error("no key found for space {space_id} at epoch {epoch}")]
    EpochNotFound { space_id: String, epoch: u64 },
    #[error("no epochs recorded for space {space_id}")]
    NoEpochsForSpace { space_id: String },
    #[error(
        "key blob for space {space_id} epoch {epoch} decoded to {len} bytes, expected {KEY_LEN}"
    )]
    InvalidKeyLength {
        space_id: String,
        epoch: u64,
        len: usize,
    },
    #[error("failed to decode base64 key for space {space_id} epoch {epoch}: {source}")]
    Decode {
        space_id: String,
        epoch: u64,
        #[source]
        source: base64::DecodeError,
    },
    #[error("malformed row for space {space_id}: {reason}")]
    RowShape { space_id: String, reason: String },
    #[error("database error: {0}")]
    Db(#[from] DatabaseError),
}

type CacheKey = (String, u64);

lazy_static! {
    static ref KEY_CACHE: Mutex<HashMap<CacheKey, [u8; KEY_LEN]>> = Mutex::new(HashMap::new());
}

fn cache_get(space_id: &str, epoch: u64) -> Option<[u8; KEY_LEN]> {
    KEY_CACHE
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(&(space_id.to_string(), epoch))
        .copied()
}

fn cache_put(space_id: &str, epoch: u64, key: [u8; KEY_LEN]) {
    KEY_CACHE
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert((space_id.to_string(), epoch), key);
}

/// Resolve the AEAD key for `(space_id, epoch)` from `haex_mls_sync_keys`.
///
/// Open path: the envelope carries the epoch, and this function looks up
/// exactly that key. A missing row is an error, never a fallback to the
/// current epoch — see the module-level rationale.
pub fn resolve_key(
    space_id: &str,
    epoch: u64,
    db: &DbConnection,
) -> Result<[u8; KEY_LEN], KeyError> {
    if let Some(cached) = cache_get(space_id, epoch) {
        return Ok(cached);
    }
    let rows = select_with_crdt(
        "SELECT key_data FROM haex_mls_sync_keys WHERE space_id = ?1 AND epoch = ?2".to_string(),
        vec![
            JsonValue::String(space_id.to_string()),
            JsonValue::Number((epoch as i64).into()),
        ],
        db,
    )?;
    let row = rows.first().ok_or_else(|| KeyError::EpochNotFound {
        space_id: space_id.to_string(),
        epoch,
    })?;
    let key_col = row.first().ok_or_else(|| KeyError::RowShape {
        space_id: space_id.to_string(),
        reason: format!("epoch {epoch}: missing key_data column"),
    })?;
    let key = decode_key(space_id, epoch, key_col)?;
    cache_put(space_id, epoch, key);
    Ok(key)
}

/// Resolve the newest `(epoch, key)` pair for `space_id`.
///
/// Seal path: MLS epochs advance forward, so a fresh seal always uses the
/// highest epoch present locally. Never mints new keys — the MLS layer
/// produces them and CRDT-syncs them into `haex_mls_sync_keys` before this
/// function is called. If nothing is present (handshake incomplete, member
/// just joined and their `haex_mls_sync_keys` fanout has not landed yet),
/// this returns [`KeyError::NoEpochsForSpace`].
pub fn resolve_latest(space_id: &str, db: &DbConnection) -> Result<(u64, [u8; KEY_LEN]), KeyError> {
    let rows = select_with_crdt(
        "SELECT epoch, key_data FROM haex_mls_sync_keys \
         WHERE space_id = ?1 ORDER BY epoch DESC LIMIT 1"
            .to_string(),
        vec![JsonValue::String(space_id.to_string())],
        db,
    )?;
    let row = rows.first().ok_or_else(|| KeyError::NoEpochsForSpace {
        space_id: space_id.to_string(),
    })?;
    let epoch_col = row.first().ok_or_else(|| KeyError::RowShape {
        space_id: space_id.to_string(),
        reason: "missing epoch column".to_string(),
    })?;
    let epoch_i64 = epoch_col.as_i64().ok_or_else(|| KeyError::RowShape {
        space_id: space_id.to_string(),
        reason: format!("epoch not an integer: {epoch_col}"),
    })?;
    let epoch = epoch_i64 as u64;
    let key_col = row.get(1).ok_or_else(|| KeyError::RowShape {
        space_id: space_id.to_string(),
        reason: format!("epoch {epoch}: missing key_data column"),
    })?;
    let key = decode_key(space_id, epoch, key_col)?;
    cache_put(space_id, epoch, key);
    Ok((epoch, key))
}

fn decode_key(space_id: &str, epoch: u64, key_col: &JsonValue) -> Result<[u8; KEY_LEN], KeyError> {
    let key_b64 = key_col.as_str().ok_or_else(|| KeyError::RowShape {
        space_id: space_id.to_string(),
        reason: format!("epoch {epoch}: key_data not a string: {key_col}"),
    })?;
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(key_b64)
        .map_err(|source| KeyError::Decode {
            space_id: space_id.to_string(),
            epoch,
            source,
        })?;
    if decoded.len() != KEY_LEN {
        return Err(KeyError::InvalidKeyLength {
            space_id: space_id.to_string(),
            epoch,
            len: decoded.len(),
        });
    }
    let mut key = [0u8; KEY_LEN];
    key.copy_from_slice(&decoded);
    Ok(key)
}
