use ed25519_dalek::SigningKey;
use rusqlite::types::Type;
use rusqlite::Connection;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use tracing::{error, warn};

use crate::extension::spaces::queries::{SQL_SELECT_ALL_OWN_SPACE_KEYS, SQL_SELECT_OWN_SPACE_KEY};
use crate::ucan::signing_key_from_pkcs8_base64;

#[derive(Clone, Default)]
pub struct SpaceKeyCache {
    inner: Arc<RwLock<HashMap<String, SigningKey>>>,
}

impl SpaceKeyCache {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get(&self, space_id: &str) -> Option<SigningKey> {
        match self.inner.read() {
            Ok(guard) => guard.get(space_id).cloned(),
            Err(e) => {
                error!(target: "column_sig", error = %e, space_id, "SpaceKeyCache RwLock poisoned on get");
                None
            }
        }
    }

    pub fn insert(&self, space_id: &str, key: SigningKey) {
        match self.inner.write() {
            Ok(mut w) => {
                w.insert(space_id.to_string(), key);
            }
            Err(e) => {
                error!(target: "column_sig", error = %e, space_id, "SpaceKeyCache RwLock poisoned on insert");
            }
        }
    }

    pub fn remove(&self, space_id: &str) {
        match self.inner.write() {
            Ok(mut w) => {
                w.remove(space_id);
            }
            Err(e) => {
                error!(target: "column_sig", error = %e, space_id, "SpaceKeyCache RwLock poisoned on remove");
            }
        }
    }

    pub fn contains(&self, space_id: &str) -> bool {
        match self.inner.read() {
            Ok(guard) => guard.contains_key(space_id),
            Err(e) => {
                error!(target: "column_sig", error = %e, space_id, "SpaceKeyCache RwLock poisoned on contains");
                false
            }
        }
    }

    /// Load every space where this vault holds a signing key and cache it.
    /// Replaces the current cache contents. Called at vault-open time; per-mutation
    /// invalidation (Task C4) keeps it in sync afterwards.
    ///
    /// Returns `Err(ToSqlConversionFailure)` if the internal `RwLock` is poisoned,
    /// so the caller's warn-log path fires (silent `Ok(0)` would mask the bug —
    /// see `critical-failure-pattern-plan`).
    pub fn populate_all(&self, conn: &Connection) -> rusqlite::Result<usize> {
        let rows = load_all_own_space_keys(conn)?;
        match self.inner.write() {
            Ok(mut w) => {
                w.clear();
                for (space_id, key) in &rows {
                    w.insert(space_id.clone(), key.clone());
                }
                Ok(rows.len())
            }
            Err(e) => {
                error!(target: "column_sig", error = %e, "SpaceKeyCache RwLock poisoned in populate_all");
                Err(rusqlite::Error::ToSqlConversionFailure(Box::new(
                    std::io::Error::other(format!("SpaceKeyCache RwLock poisoned: {e}")),
                )))
            }
        }
    }

    /// Return the signing key for `space_id`, reloading from the DB on a cache
    /// miss (belt-and-suspenders: Task C4 invalidates at every mutation site,
    /// but a warn-logged JIT reload keeps the signer working if a hook was
    /// missed).
    pub fn get_or_reload(
        &self,
        conn: &Connection,
        space_id: &str,
    ) -> rusqlite::Result<Option<SigningKey>> {
        if let Some(k) = self.get(space_id) {
            return Ok(Some(k));
        }
        warn!(target: "column_sig", space_id, "cache miss — JIT reload");
        if let Some(key) = load_one_own_space_key(conn, space_id)? {
            self.insert(space_id, key.clone());
            return Ok(Some(key));
        }
        Ok(None)
    }
}

// ---------------------------------------------------------------------------
// SQL loaders
// ---------------------------------------------------------------------------

fn load_all_own_space_keys(conn: &Connection) -> rusqlite::Result<Vec<(String, SigningKey)>> {
    let mut stmt = conn.prepare(&SQL_SELECT_ALL_OWN_SPACE_KEYS)?;
    let rows = stmt.query_map([], |row| {
        let space_id: String = row.get(0)?;
        let private_key_b64: String = row.get(1)?;
        Ok((space_id, private_key_b64))
    })?;

    let mut out = Vec::new();
    for row in rows {
        let (space_id, private_key_b64) = row?;
        let key = decode_pkcs8(&private_key_b64, 1)?;
        out.push((space_id, key));
    }
    Ok(out)
}

fn load_one_own_space_key(
    conn: &Connection,
    space_id: &str,
) -> rusqlite::Result<Option<SigningKey>> {
    let mut stmt = conn.prepare(&SQL_SELECT_OWN_SPACE_KEY)?;
    let mut rows = stmt.query([space_id])?;
    if let Some(row) = rows.next()? {
        let private_key_b64: String = row.get(0)?;
        let key = decode_pkcs8(&private_key_b64, 0)?;
        Ok(Some(key))
    } else {
        Ok(None)
    }
}

fn decode_pkcs8(private_key_b64: &str, col_idx: usize) -> rusqlite::Result<SigningKey> {
    signing_key_from_pkcs8_base64(private_key_b64).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(
            col_idx,
            Type::Text,
            Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("private_key decode: {e}"),
            )),
        )
    })
}
