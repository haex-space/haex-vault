//! Per-transaction resolver for "into which spaces is this row shared?".
//!
//! For each `(table, row_pks)` pair the resolver returns the list of `space_id`s
//! for which the *local* vault should produce column signatures. The result is
//! cached inside the lookup instance so a single transaction that touches many
//! columns of the same row only pays one DB round-trip.
//!
//! Two paths:
//!   * **Infra tables** — the five [`SPACE_SCOPED_CRDT_TABLES`] carry a
//!     `space_id` column inline; the row itself is authoritative. We SELECT
//!     from that table.
//!   * **Extension tables** — everything else. Ownership is expressed via
//!     the share register `haex_shared_space_sync`. We join it against the
//!     local `haex_identities` (I2 filter) so only entries authored by *this
//!     vault's* identity survive; foreign share entries pointing to other
//!     spaces are ignored — we cannot sign for a space we do not own.
//!
//! The I2 filter currently keys off `haex_shared_space_sync.authored_by_did`;
//! once Task D1/G1 removes that column in favour of column-sig-based author
//! identification, this filter is expected to migrate to the sig column.

use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, HashMap};

use rusqlite::{params_from_iter, types::Value as SqlValue, Connection};
use serde_json::Value as JsonValue;

use crate::crdt::scanner::is_space_scoped_table;
use crate::crdt::trigger::is_safe_identifier;

/// SQL for the extension-table path. Uses the I2 filter: only rows whose
/// `authored_by_did` matches one of the local identities (`private_key
/// IS NOT NULL`) are counted.
const SQL_SELECT_REGISTER_OWN_SPACES: &str = "\
    SELECT DISTINCT r.space_id \
    FROM haex_shared_space_sync r \
    WHERE r.table_name = ?1 \
      AND r.row_pks = ?2 \
      AND r.authored_by_did IN \
          (SELECT did FROM haex_identities WHERE private_key IS NOT NULL)";

/// Per-transaction resolver for space membership of a `(table, row_pks)` pair.
///
/// Not `Send`/`Sync`: uses interior mutability (`RefCell`, `Cell`) and is
/// intended to live for the duration of a single write transaction.
#[derive(Default)]
pub struct RegisterLookup {
    cache: RefCell<HashMap<(String, String), Vec<String>>>,
    hits: Cell<usize>,
}

impl RegisterLookup {
    pub fn new() -> Self {
        Self::default()
    }

    /// Resolves which space_ids `(table_name, row_pks_json)` is shared into.
    ///
    /// Result is filtered by the I2 rule for extension tables (only share
    /// entries authored by a local identity). Infra tables return the
    /// row's own `space_id`.
    ///
    /// `row_pks_json` is expected to be a JSON object of PK column values
    /// as produced by the CRDT scanner (e.g. `{"id":"abc-123"}`). It is
    /// normalised via `serde_json` before use so that cache keys survive
    /// whitespace / key-order differences produced by different callers.
    pub fn resolve(
        &self,
        conn: &Connection,
        table_name: &str,
        row_pks_json: &str,
    ) -> rusqlite::Result<Vec<String>> {
        let canonical_pks = canonicalize_row_pks(row_pks_json)?;
        let cache_key = (table_name.to_string(), canonical_pks.clone());

        if let Some(cached) = self.cache.borrow().get(&cache_key) {
            self.hits.set(self.hits.get() + 1);
            return Ok(cached.clone());
        }

        let spaces = if is_space_scoped_table(table_name) {
            resolve_infra_row(conn, table_name, &canonical_pks)?
        } else {
            resolve_extension_row(conn, table_name, &canonical_pks)?
        };

        self.cache.borrow_mut().insert(cache_key, spaces.clone());
        Ok(spaces)
    }

    /// Number of resolves served from the in-memory cache. Useful for tests
    /// and future observability; not part of the correctness contract.
    pub fn cache_hits(&self) -> usize {
        self.hits.get()
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Normalises `row_pks_json` by parsing into a `BTreeMap` (sorted keys) and
/// re-serialising. Two callers that produce logically-equal PK payloads with
/// different key orderings will hit the same cache entry and match the same
/// register rows.
fn canonicalize_row_pks(row_pks_json: &str) -> rusqlite::Result<String> {
    let parsed: BTreeMap<String, JsonValue> = serde_json::from_str(row_pks_json).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(
            0,
            rusqlite::types::Type::Text,
            Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("row_pks JSON parse: {e}"),
            )),
        )
    })?;
    serde_json::to_string(&parsed).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(
            0,
            rusqlite::types::Type::Text,
            Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("row_pks JSON reserialise: {e}"),
            )),
        )
    })
}

/// Extract the space_id from an infra table's row itself.
///
/// The five [`SPACE_SCOPED_CRDT_TABLES`](crate::crdt::scanner::SPACE_SCOPED_CRDT_TABLES)
/// all carry a `space_id` column; the row is authoritative. Returns an empty
/// vector when the row does not exist (a caller mid-insert may resolve before
/// the row is materialised — signing zero spaces is the correct fallback).
fn resolve_infra_row(
    conn: &Connection,
    table_name: &str,
    canonical_pks: &str,
) -> rusqlite::Result<Vec<String>> {
    // Defense-in-depth: even though `is_space_scoped_table` limited us to a
    // hardcoded whitelist, re-check identifier shape before interpolating.
    if !is_safe_identifier(table_name) {
        return Ok(Vec::new());
    }

    let pks: BTreeMap<String, JsonValue> =
        serde_json::from_str(canonical_pks).expect("canonicalize_row_pks produced non-object JSON");
    if pks.is_empty() {
        return Ok(Vec::new());
    }
    if !pks.keys().all(|k| is_safe_identifier(k)) {
        return Ok(Vec::new());
    }

    let mut where_parts: Vec<String> = Vec::with_capacity(pks.len());
    let mut binds: Vec<SqlValue> = Vec::with_capacity(pks.len());
    for (col, value) in &pks {
        match value {
            JsonValue::Null => {
                where_parts.push(format!("\"{col}\" IS NULL"));
            }
            _ => {
                where_parts.push(format!("\"{col}\" = ?"));
                binds.push(json_to_sql_value(value));
            }
        }
    }

    let sql = format!(
        "SELECT space_id FROM \"{table_name}\" WHERE {} LIMIT 1",
        where_parts.join(" AND ")
    );
    let mut stmt = conn.prepare(&sql)?;
    let mut rows = stmt.query(params_from_iter(binds.iter()))?;
    if let Some(row) = rows.next()? {
        Ok(vec![row.get::<_, String>(0)?])
    } else {
        Ok(Vec::new())
    }
}

/// Extension-table path: consult the share register with the I2 filter.
fn resolve_extension_row(
    conn: &Connection,
    table_name: &str,
    canonical_pks: &str,
) -> rusqlite::Result<Vec<String>> {
    let mut stmt = conn.prepare(SQL_SELECT_REGISTER_OWN_SPACES)?;
    let mut rows = stmt.query((table_name, canonical_pks))?;
    let mut out = Vec::new();
    while let Some(row) = rows.next()? {
        out.push(row.get::<_, String>(0)?);
    }
    Ok(out)
}

/// Convert a JSON PK value into the SQLite storage class that the row was
/// most likely written as. Strings stay as text, integers stay as integers,
/// floats stay as reals, booleans map to `0`/`1`, and anything else is
/// serialised back to JSON text (matches how the CRDT scanner encodes
/// composite PKs).
fn json_to_sql_value(value: &JsonValue) -> SqlValue {
    match value {
        JsonValue::Null => SqlValue::Null,
        JsonValue::Bool(b) => SqlValue::Integer(if *b { 1 } else { 0 }),
        JsonValue::Number(n) => {
            if let Some(i) = n.as_i64() {
                SqlValue::Integer(i)
            } else if let Some(f) = n.as_f64() {
                SqlValue::Real(f)
            } else {
                SqlValue::Text(n.to_string())
            }
        }
        JsonValue::String(s) => SqlValue::Text(s.clone()),
        other => SqlValue::Text(other.to_string()),
    }
}
