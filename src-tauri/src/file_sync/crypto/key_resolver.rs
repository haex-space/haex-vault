//! Round B — shared-space key resolution for the file envelope.
//!
//! **Scope:** shared-space (MLS epoch) keys only. Given a `(space_id, epoch)`
//! pair, look up the matching sync key from `haex_mls_sync_keys`, derive a
//! domain-separated file-content key from it, and cache the result for the
//! process lifetime. The own-vault branch — file-content encryption under the
//! vault key rather than an epoch key — is *not* handled here: the caller
//! decides which key path to take before invoking this module. That leaves
//! vault-key transport (TS → Rust via a Tauri command) as a Round D wiring
//! problem.
//!
//! ## Key separation
//!
//! `haex_mls_sync_keys.key_data` holds the MLS exporter output for the label
//! `haex-vault-sync` ([`crate::mls::manager::MlsManager::derive_epoch_key`]),
//! and the CRDT sync path already encrypts its payloads directly under those
//! bytes (`src/stores/sync/orchestrator/push.ts`, `pull/apply.ts`). Feeding
//! the same key to the file-content AEAD would put two protocols with
//! independent, unsynchronised nonce spaces on one secret. This module
//! therefore returns `BLAKE3::derive_key(FILE_KEY_CONTEXT, sync_key)` — never
//! the stored bytes — so a nonce-construction bug on either side stays
//! contained and a sync-payload key compromise is not automatically a
//! file-content compromise.
//!
//! ## Why the open path never falls back
//!
//! The envelope header carries the epoch it was sealed under. [`resolve_key`]
//! looks up **exactly** that epoch; a missing row surfaces
//! [`KeyError::EpochNotFound`] rather than substituting the current epoch's
//! key. Silent fallback would break historical confidentiality: an epoch that
//! is no longer accessible (member left, key material pruned locally, DB
//! corruption) would be papered over with the wrong key and the reader would
//! get either garbage or a
//! [`CryptoError::OpenFailed`](crate::file_sync::crypto::chunk::CryptoError::OpenFailed)
//! that is impossible to diagnose from logs.
//!
//! ## Why the seal path does not trust the DB for the epoch
//!
//! `haex_mls_sync_keys` is space-scoped, CRDT-replicated, and writable by any
//! member holding only [`Cap::Read`](crate::ucan::Cap) — it is on
//! [`MEMBERSHIP_SYSTEM_TABLES`](crate::crdt::scanner::MEMBERSHIP_SYSTEM_TABLES)
//! and
//! [`owner_column_for`](crate::space_delivery::local::inbound_sync) returns
//! `None` for it, so there is no per-row ownership check. Picking the seal key
//! with `ORDER BY epoch DESC LIMIT 1` would therefore let any member pin every
//! future seal to a key of their choosing by pushing one row with an
//! implausibly high epoch. [`resolve_latest`] instead asks the local MLS group
//! for the current epoch — the same authority the CRDT seal path uses via
//! `mls_export_epoch_key` — and then does an exact lookup.
//!
//! ## Cache
//!
//! Cached per `(space_id, epoch)`. The premise is *not* that the row is
//! immutable: `key_data` is a plain CRDT-LWW column that any write member can
//! replace, and the row is deleted by the `ON DELETE cascade` from
//! `haex_spaces`. The premise is that all honest members of an epoch derive
//! the *same* exporter output from the same group state, so a well-formed row
//! for a given `(space_id, epoch)` always carries the same bytes. A row whose
//! value diverges from an already-resolved one is not a legitimate rewrite —
//! see [`resolve_key`]'s divergence check.
//!
//! [`clear_key_cache`] drops all entries; `database::create::close_database`
//! calls it so epoch keys do not outlive the vault they belong to.
//!
//! The cache lock uses `unwrap_or_else(|e| e.into_inner())` — same rationale as
//! [`crate::file_sync::hashing`]'s `HASH_CACHE`: a poisoned lock here only
//! means a previous panic occurred while inserting derived data. Recomputing
//! from the DB on the next call is correct and cheap.

use std::collections::HashMap;
use std::sync::Mutex;

use base64::Engine;
use lazy_static::lazy_static;
use serde_json::Value as JsonValue;

use crate::database::core::select_with_crdt;
use crate::database::error::DatabaseError;
use crate::database::DbConnection;
use crate::mls::manager::MlsManager;

/// AEAD key length (XChaCha20Poly1305 uses a 256-bit key).
pub const KEY_LEN: usize = 32;

/// BLAKE3 derivation context separating file-content keys from the MLS
/// `haex-vault-sync` exporter output the CRDT sync path encrypts under.
/// Changing this string invalidates every previously sealed file.
const FILE_KEY_CONTEXT: &str = "haex-vault 2026-08-25 file-content key v1";

/// Errors surfaced by the key resolver. Kept flat and non-`Copy` so callers
/// can attach context strings without wrapping.
#[derive(Debug, thiserror::Error)]
pub enum KeyError {
    #[error("no key found for space {space_id} at epoch {epoch}")]
    EpochNotFound { space_id: String, epoch: u64 },
    #[error(
        "conflicting key rows for space {space_id} at epoch {epoch}: \
         {count} rows disagree on key_data"
    )]
    AmbiguousKey {
        space_id: String,
        epoch: u64,
        count: usize,
    },
    #[error("no local MLS epoch for space {space_id}: {reason}")]
    MlsEpochUnavailable { space_id: String, reason: String },
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
    #[error("malformed row for space {space_id} epoch {epoch}: key_data not a string")]
    RowShape { space_id: String, epoch: u64 },
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

/// Drop every cached file-content key.
///
/// Called from `database::create::close_database`: the cache is process-wide
/// while the keys are per-vault, so without this a closed vault's key material
/// stays resident and would be served — without touching the DB at all — to
/// the next vault that mounts the same space.
pub fn clear_key_cache() {
    KEY_CACHE.lock().unwrap_or_else(|e| e.into_inner()).clear();
}

/// Resolve the file-content AEAD key for `(space_id, epoch)`.
///
/// Open path: the envelope carries the epoch, and this function looks up
/// exactly that key. A missing row is an error, never a fallback to the
/// current epoch — see the module-level rationale.
///
/// The returned bytes are the domain-separated derivative of
/// `haex_mls_sync_keys.key_data`, not the stored value itself.
///
/// `(space_id, epoch)` has no UNIQUE constraint and each device mints its own
/// row `id`, so duplicates arise legitimately when two members export the same
/// epoch before either has seen the other's row. Honest duplicates carry
/// identical bytes; a set that *disagrees* means someone pushed a forged key,
/// and picking one at scan order would be a coin flip. That case surfaces
/// [`KeyError::AmbiguousKey`] instead.
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
    if rows.is_empty() {
        return Err(KeyError::EpochNotFound {
            space_id: space_id.to_string(),
            epoch,
        });
    }

    // `select_with_crdt` always pushes `stmt.column_count()` values per row,
    // so index 0 of a one-column SELECT is present by construction.
    let mut resolved: Option<[u8; KEY_LEN]> = None;
    for row in &rows {
        let sync_key = decode_key(space_id, epoch, &row[0])?;
        match resolved {
            None => resolved = Some(sync_key),
            Some(seen) if seen == sync_key => {}
            Some(_) => {
                return Err(KeyError::AmbiguousKey {
                    space_id: space_id.to_string(),
                    epoch,
                    count: rows.len(),
                })
            }
        }
    }

    let key = derive_file_key(&resolved.expect("rows is non-empty"));
    cache_put(space_id, epoch, key);
    Ok(key)
}

/// Resolve the `(epoch, key)` pair to seal a fresh file under for `space_id`.
///
/// The epoch comes from the **local MLS group**, never from
/// `haex_mls_sync_keys` — that table is peer-writable, so a max-epoch scan
/// would be attacker-steerable (see the module-level rationale). Never mints
/// new keys: the MLS layer produces them and CRDT-syncs them into
/// `haex_mls_sync_keys` before this function is called. A group whose key row
/// has not landed yet surfaces [`KeyError::EpochNotFound`]; no local group at
/// all surfaces [`KeyError::MlsEpochUnavailable`].
pub fn resolve_latest(space_id: &str, db: &DbConnection) -> Result<(u64, [u8; KEY_LEN]), KeyError> {
    let epoch = MlsManager::new(db.0.clone())
        .current_epoch(space_id)
        .map_err(|reason| KeyError::MlsEpochUnavailable {
            space_id: space_id.to_string(),
            reason,
        })?;
    let key = resolve_key(space_id, epoch, db)?;
    Ok((epoch, key))
}

/// Domain-separate the MLS `haex-vault-sync` exporter output into a
/// file-content key. See the module-level "Key separation" section.
pub(super) fn derive_file_key(sync_key: &[u8; KEY_LEN]) -> [u8; KEY_LEN] {
    blake3::derive_key(FILE_KEY_CONTEXT, sync_key)
}

fn decode_key(space_id: &str, epoch: u64, key_col: &JsonValue) -> Result<[u8; KEY_LEN], KeyError> {
    let key_b64 = key_col.as_str().ok_or_else(|| KeyError::RowShape {
        space_id: space_id.to_string(),
        epoch,
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
