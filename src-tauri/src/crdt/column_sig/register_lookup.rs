//! Per-transaction resolver for "into which spaces is this row shared?".
//!
//! For each `(table, row_pks)` pair the resolver returns the list of every
//! `space_id` that has ever been declared as sharing this row (extension path)
//! or the row's own space (infra path). Result is cached inside the lookup
//! instance so a single transaction that touches many columns of the same row
//! only pays one DB round-trip.
//!
//! Two paths:
//!   * **Built-in space-scoped tables** — [`SPACE_SCOPED_CRDT_TABLES`] carry a
//!     `space_id` column inline; the row itself is authoritative. We SELECT
//!     from that table.
//!   * **Extension tables** — everything else. Ownership is expressed via
//!     the share register `haex_shared_space_sync`. We return only mappings
//!     whose routing columns were signed by an identity whose private key
//!     belongs to this vault. Space membership alone is not proof that this
//!     vault initiated the share (ADR 0002 I2).

use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, BTreeSet, HashMap};

use rusqlite::{params_from_iter, types::Value as SqlValue, Connection};
use serde_json::Value as JsonValue;
use tracing::error;

use crate::crdt::scanner::is_space_scoped_table;
use crate::crdt::trigger::{get_table_schema, is_safe_identifier};

/// System-table payloads that intentionally use the register rather than an
/// inline `space_id`. This list is fail-closed: adding a new `haex_*` table
/// requires an explicit security review.
///
/// `haex_s3_backends` stores the scoped child credential created by the
/// remote-storage share flow; the owner/backend rows are never registered.
const REGISTER_SHAREABLE_SYSTEM_TABLES: &[&str] = &["haex_s3_backends"];

/// True iff `table` may not appear as `haex_shared_space_sync.table_name`.
///
/// Applied on BOTH sig paths (ADR 0002 §4b):
///   * F2 (`execute.rs::sign_share_insert_targets`) — the register INSERT
///     itself is rejected with an I1 error.
///   * F1 (`resolve_extension_row` in this file) — a legacy or malicious
///     register row targeting a forbidden table is silently ignored (the
///     path returns an empty space list so no sig is produced), because
///     it may already be present in the DB and we do not want to error
///     on every read.
pub fn is_register_target_forbidden(table: &str) -> bool {
    let lower = table.to_ascii_lowercase();
    // I1 is fail-closed: every new Haex system table is private until it is
    // explicitly reviewed as a register-carried payload.
    (lower.starts_with("haex_") && !REGISTER_SHAREABLE_SYSTEM_TABLES.contains(&lower.as_str()))
        || lower.starts_with("sqlite_")
        || lower.ends_with("_no_sync")
}

/// Candidate mappings plus this vault's own member DID for the space.
const SQL_SELECT_REGISTER_SPACES: &str = "\
    SELECT r.space_id, r.haex_column_sigs, i.did \
    FROM haex_shared_space_sync r \
    JOIN haex_space_members m ON m.space_id = r.space_id \
    JOIN haex_identities i ON i.id = m.identity_id AND i.private_key IS NOT NULL \
    WHERE r.table_name = ?1 \
      AND r.row_pks = ?2";

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
    /// Returns every self-authored mapping (extension path) or the row's own
    /// `space_id` (built-in space-scoped path).
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
///
/// `pub(crate)`: also used by `execute.rs`'s registry-row sign-on-write pass
/// (Task B.3) to canonicalise `haex_shared_space_sync.row_pks` before it is
/// signed/persisted — this is the same exact-string-match value
/// [`resolve_extension_row`] compares against, so the two must agree on one
/// canonical form.
pub(crate) fn canonicalize_row_pks(row_pks_json: &str) -> rusqlite::Result<String> {
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

    // F#2: enforce full-PK coverage. A partial PK on a composite-key table
    // (e.g. `haex_space_members` (space_id, identity_id) with only
    // `identity_id` supplied) would otherwise silently match an arbitrary
    // row via LIMIT 1 and return the wrong `space_id` for signing.
    let schema = get_table_schema(conn, table_name)?;
    let expected_pk_names: BTreeSet<&str> = schema
        .iter()
        .filter(|c| c.is_pk)
        .map(|c| c.name.as_str())
        .collect();
    if expected_pk_names.is_empty() {
        // Infra tables always have a PK; missing means schema drift.
        error!(
            target: "column_sig",
            table = table_name,
            "Infra table has no primary key — refusing to sign"
        );
        return Err(rusqlite::Error::InvalidParameterName(format!(
            "Infra table '{table_name}' has no primary key"
        )));
    }
    let supplied_pk_names: BTreeSet<&str> = pks.keys().map(|s| s.as_str()).collect();
    if supplied_pk_names != expected_pk_names {
        error!(
            target: "column_sig",
            table = table_name,
            expected = ?expected_pk_names,
            supplied = ?supplied_pk_names,
            "Infra-table PK column set mismatch — refusing to sign"
        );
        return Err(rusqlite::Error::InvalidParameterName(format!(
            "Infra-table PK mismatch on '{table_name}': expected {:?}, got {:?}",
            expected_pk_names, supplied_pk_names
        )));
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

/// Extension-table path: consult only self-authored share-register rows.
///
/// If `haex_shared_space_sync` does not exist (test harnesses that skip the
/// full CRDT-bootstrap, or freshly-created vaults pre-migration), treat that
/// as "no share entries" and return an empty list — signing zero spaces is
/// the correct semantic fallback.
///
/// F#3 (Runde-4 review): apply the I1 exclusion here too. A legacy or
/// malicious register row targeting a forbidden `haex_*` / `sqlite_*`
/// / `_no_sync` table must not cause F1 to sign private-column values
/// for the target space. Returning an empty vec (rather than erroring)
/// keeps read-side flows tolerant of legacy garbage — F2 rejects new
/// register INSERTs of that shape with a hard I1 error.
fn resolve_extension_row(
    conn: &Connection,
    table_name: &str,
    canonical_pks: &str,
) -> rusqlite::Result<Vec<String>> {
    if is_register_target_forbidden(table_name) {
        return Ok(Vec::new());
    }
    for required in [
        "haex_shared_space_sync",
        "haex_space_members",
        "haex_identities",
    ] {
        let exists = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1)",
            [required],
            |row| row.get::<_, bool>(0),
        )?;
        if !exists {
            return Ok(Vec::new());
        }
    }

    let mut stmt = conn.prepare(SQL_SELECT_REGISTER_SPACES)?;
    let mut rows = stmt.query((table_name, canonical_pks))?;
    let mut out = Vec::new();
    while let Some(row) = rows.next()? {
        let space_id = row.get::<_, String>(0)?;
        let sigs = row.get::<_, String>(1)?;
        let own_did = row.get::<_, String>(2)?;
        if register_routing_is_self_authored(&sigs, &space_id, &own_did) {
            out.push(space_id);
        }
    }
    out.sort();
    out.dedup();
    Ok(out)
}

fn register_routing_is_self_authored(sigs: &str, space_id: &str, own_did: &str) -> bool {
    let Ok(JsonValue::Object(root)) = serde_json::from_str::<JsonValue>(sigs) else {
        return false;
    };
    ["table_name", "row_pks", "space_id"].iter().all(|column| {
        root.get(*column)
            .and_then(|by_space| by_space.get(space_id))
            .and_then(|record| record.get("authorDid"))
            .and_then(JsonValue::as_str)
            == Some(own_did)
    })
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
