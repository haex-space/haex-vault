//! Multi-space column signer.
//!
//! Combines the [`RegisterLookup`] (per-tx resolver of "which spaces does this
//! row belong to?"), the [`SpaceKeyCache`] (session signing keys), and the
//! pure [`sign_column`] primitive into a single write-time helper.
//!
//! The output is a `HashMap<space_id, SigRecord>` — one signature per owning
//! space. `execute_with_crdt` folds each record into the row's
//! `haex_column_sigs` JSON via the storage helper (Task E3).
//!
//! DID derivation: the author DID for each space is derived from the
//! `SigningKey.verifying_key()` we pulled from the cache. This means one DB
//! query is enough per space (the key), and the DID is guaranteed to match
//! the key we sign with.

use std::collections::HashMap;

use rusqlite::{types::Value, Connection};

use crate::crdt::column_sig::{
    key_cache::SpaceKeyCache, limits::check_value_bytes_len, register_lookup::RegisterLookup,
    sign::sign_column, storage::SigRecord, value_bytes,
};
use crate::ucan::verify::did_key_from_public_key;

#[derive(Debug, thiserror::Error)]
pub enum SignForSpacesError {
    #[error("value bytes too large: {0}")]
    ValueBytesTooLarge(String),
    #[error("no signing key available for space {0}")]
    NoKeyForSpace(String),
    /// Reserved for future use: today we derive the author DID from the
    /// `SigningKey.verifying_key()`, so the DID is always available whenever
    /// a key is. Kept in the contract so callers can pattern-match on it if
    /// the derivation strategy ever changes (e.g. wallet-managed keys where
    /// the DID lives in a separate row).
    #[error("could not resolve author did for space {0}")]
    #[allow(dead_code)]
    NoAuthorDid(String),
    #[error("database error: {0}")]
    Database(#[from] rusqlite::Error),
}

/// Sign one column-write for every space the row is shared into.
///
/// Returns `HashMap<space_id, SigRecord>`. An empty map is a valid result
/// (row is not in any space → caller writes `{}` into `haex_column_sigs`).
#[allow(clippy::too_many_arguments)]
pub fn sign_column_for_spaces(
    conn: &Connection,
    key_cache: &SpaceKeyCache,
    register: &RegisterLookup,
    table_name: &str,
    row_pks_json: &str,
    column_name: &str,
    hlc: &str,
    value: &Value,
) -> Result<HashMap<String, SigRecord>, SignForSpacesError> {
    let value_bytes_vec = value_bytes::to_canonical_bytes(value);
    check_value_bytes_len(value_bytes_vec.len())
        .map_err(|e| SignForSpacesError::ValueBytesTooLarge(e.to_string()))?;

    let spaces = register.resolve(conn, table_name, row_pks_json)?;
    let mut out = HashMap::with_capacity(spaces.len());

    for space_id in spaces {
        let key = key_cache
            .get_or_reload(conn, &space_id)?
            .ok_or_else(|| SignForSpacesError::NoKeyForSpace(space_id.clone()))?;
        let did = did_key_from_public_key(&key.verifying_key());
        let sig = sign_column(
            &key,
            space_id.as_bytes(),
            table_name.as_bytes(),
            row_pks_json.as_bytes(),
            column_name.as_bytes(),
            hlc.as_bytes(),
            did.as_bytes(),
            &value_bytes_vec,
        );
        out.insert(
            space_id,
            SigRecord {
                author_did: did,
                sig: sig.to_bytes(),
            },
        );
    }

    Ok(out)
}
