// src-tauri/src/database/core/execute.rs

use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use rusqlite::types::Value as RusqliteValue;
use rusqlite::{OptionalExtension, ToSql, Transaction};
use serde_json::Value as JsonValue;
use sqlparser::ast::{AssignmentTarget, ObjectName, Statement, TableFactor, TableObject};

use crate::crdt::column_sig::key_cache::SpaceKeyCache;
use crate::crdt::column_sig::register_lookup::{is_register_target_forbidden, RegisterLookup};
use crate::crdt::column_sig::sign::sign_column;
use crate::crdt::column_sig::storage::{upsert_column_sigs, SigRecord};
use crate::crdt::column_sig::value_bytes;
use crate::crdt::column_sig::write::sign_column_for_spaces;
use crate::crdt::trigger::{
    get_table_schema, is_safe_identifier, COLUMN_HLCS_COLUMN, COLUMN_SIGS_COLUMN,
    HLC_FUNCTION_NAME, HLC_TIMESTAMP_COLUMN,
};
use crate::database::core::connection::with_connection;
use crate::database::core::extract::extract_primary_table_name_from_sql;
use crate::database::core::parsing::{parse_single_statement, statement_has_returning};
use crate::database::core::value::{convert_value_ref_to_json, ValueConverter};
use crate::database::error::DatabaseError;
use crate::database::DbConnection;
use crate::extension::database::executor::SqlExecutor;
use crate::table_names::TABLE_CRDT_CONFIGS;
use crate::ucan::verify::did_key_from_public_key;

/// The share register has two signing duties: the normal F1 pass signs the
/// register row itself, while F2 retro-signs the referenced content row.
const REGISTER_TABLE: &str = "haex_shared_space_sync";

/// Maximum serialized size of a single CRDT transaction (ADR 0001).
///
/// One `execute_with_crdt` call parses one statement and runs it in its own
/// `conn.transaction()` — and nothing nests `execute_with_crdt` calls — so one
/// call is exactly one SQLite transaction (one HLC). Enforcing the cap per call
/// is therefore equivalent to a per-transaction byte counter, with no extra
/// plumbing. Larger payloads must use file storage, never CRDT columns.
pub const MAX_CRDT_TRANSACTION_BYTES: usize = 100 * 1024 * 1024;

/// Returns `Some(bytes)` if the serialized size of `params` exceeds `limit`,
/// else `None`. Fail-closed: an unmeasurable payload counts as over-limit.
///
/// `limit` is a parameter (not the const) so tests can inject a tiny limit
/// instead of allocating `MAX_CRDT_TRANSACTION_BYTES`.
fn write_payload_too_large(params: &[JsonValue], limit: usize) -> Option<usize> {
    let bytes = serde_json::to_vec(&params)
        .map(|v| v.len())
        .unwrap_or(usize::MAX);
    (bytes > limit).then_some(bytes)
}

/// Execute SQL mit CRDT-Transformation (für Drizzle-Integration).
///
/// Läuft nach dem Write die Column-Signing-Nachlese: für jede geschriebene
/// Zeile werden alle nicht-Meta-Spalten mit den Space-Signing-Keys aus
/// `key_cache` signiert und über `haex_column_sigs` persistiert.
/// Unterstützt RETURNING-Klausel: Falls vorhanden, werden die Ergebnis-Rows zurückgegeben.
pub fn execute_with_crdt(
    sql: String,
    params: Vec<JsonValue>,
    connection: &DbConnection,
    hlc_service: &std::sync::MutexGuard<crate::crdt::hlc::HlcService>,
    key_cache: &SpaceKeyCache,
) -> Result<Vec<Vec<JsonValue>>, DatabaseError> {
    // ADR 0001: reject an oversized single transaction before writing anything.
    if let Some(bytes) = write_payload_too_large(&params, MAX_CRDT_TRANSACTION_BYTES) {
        return Err(DatabaseError::TransactionTooLarge {
            bytes,
            limit: MAX_CRDT_TRANSACTION_BYTES,
        });
    }

    // Parse statement to check for RETURNING clause + touched-column extraction.
    let statement = parse_single_statement(&sql)?;
    let has_returning = statement_has_returning(&statement);
    let touched = extract_touched_for_signing(&statement);

    // F#2 (Runde-4 review, HLC-forgery guard): reject any caller-supplied
    // write to CRDT meta columns. The transformer would otherwise clobber
    // `haex_hlc` silently, and — critically — a caller-supplied
    // `haex_column_hlcs = '{"col":"9999-…"}'` would feed a forged HLC into
    // the sig-preimage on the next F2 pass, letting an attacker mint a
    // valid Ed25519 signature over an arbitrary HLC. Hard rejection is the
    // only safe choice — silent stripping is indistinguishable from success.
    if let Some(bad) = touched
        .as_ref()
        .and_then(|(_, cols)| cols.explicit().iter().find(|c| is_crdt_meta_column(c)))
    {
        return Err(DatabaseError::CrdtMetaColumnWriteForbidden {
            column: bad.clone(),
        });
    }

    with_connection(connection, |conn| {
        let tx = conn.transaction().map_err(DatabaseError::from)?;

        let result = if has_returning {
            let (_modified_tables, rows) =
                SqlExecutor::query_internal(&tx, hlc_service, &sql, &params)?;
            rows
        } else {
            let _modified_tables = SqlExecutor::execute_internal(&tx, hlc_service, &sql, &params)?;
            vec![]
        };

        if let Some((table_name, columns)) = &touched {
            sign_written_rows(&tx, key_cache, table_name, columns)?;
        }

        // F2: an INSERT into the share register itself declares that a
        // pre-existing extension row now belongs to a new space. Retro-sign
        // every non-meta column of that row for the newly-declared space,
        // using the local vault's key. I1/I2 are enforced first — a violation
        // fails the transaction and rolls back the register-row insert too.
        if is_share_register_insert(&statement, touched.as_ref()) {
            sign_share_insert_targets(&tx, key_cache)?;
        }

        tx.commit().map_err(DatabaseError::from)?;
        Ok(result)
    })
}

/// True iff `statement` is an INSERT whose target is the shared-space register.
fn is_share_register_insert(
    statement: &Statement,
    touched: Option<&(String, TouchedColumns)>,
) -> bool {
    // Fast path: `touched` already carries the table name for INSERT statements.
    if let Some((name, _)) = touched {
        return matches!(statement, Statement::Insert(_))
            && name.eq_ignore_ascii_case(REGISTER_TABLE);
    }
    false
}

/// Which columns of the target table a statement writes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum TouchedColumns {
    /// The statement names its columns: `INSERT INTO t (a, b) …` /
    /// `UPDATE t SET a = …`. Exactly these, nothing else.
    Explicit(Vec<String>),
    /// `INSERT INTO t VALUES (…)` with no column list. The write covers every
    /// column of the table positionally, so the signer must fall back to the
    /// table's schema instead of treating "no names" as "nothing written" —
    /// otherwise the row lands in a shared space with no signatures at all
    /// and remote peers see it as unsigned.
    AllColumns,
}

impl TouchedColumns {
    /// Column names the caller spelled out, for the meta-column write guard.
    /// Empty for [`Self::AllColumns`] — there are no names to inspect; that
    /// case is caught later against the real schema in [`sign_written_rows`].
    fn explicit(&self) -> &[String] {
        match self {
            Self::Explicit(cols) => cols,
            Self::AllColumns => &[],
        }
    }
}

/// Extracts `(table_name, touched_columns)` for statements that carry column
/// writes; returns `None` for statements the signer doesn't handle
/// (SELECT/DELETE/DDL).
fn extract_touched_for_signing(stmt: &Statement) -> Option<(String, TouchedColumns)> {
    match stmt {
        Statement::Insert(insert) => {
            let name = match &insert.table {
                TableObject::TableName(n) => object_name_last(n)?,
                _ => return None,
            };
            if insert.columns.is_empty() {
                return Some((name, TouchedColumns::AllColumns));
            }
            let cols: Vec<String> = insert
                .columns
                .iter()
                .filter_map(|obj| object_name_last(obj))
                .collect();
            Some((name, TouchedColumns::Explicit(cols)))
        }
        Statement::Update(update) => {
            let name = match &update.table.relation {
                TableFactor::Table { name, .. } => object_name_last(name)?,
                _ => return None,
            };
            let cols: Vec<String> = update
                .assignments
                .iter()
                .filter_map(|a| match &a.target {
                    AssignmentTarget::ColumnName(obj) => object_name_last(obj),
                    _ => None,
                })
                .collect();
            Some((name, TouchedColumns::Explicit(cols)))
        }
        _ => None,
    }
}

fn object_name_last(obj: &ObjectName) -> Option<String> {
    obj.0
        .last()
        .and_then(|p| p.as_ident())
        .map(|i| i.value.clone())
}

/// True iff `col` is one of the CRDT meta columns whose value must be produced
/// by the CRDT layer, not by the caller — see `CrdtMetaColumnWriteForbidden`.
///
/// The `__crdt_ts` / `__crdt_source` / `__crdt_ts_source` suffixes are a
/// defensive belt: no such suffix is currently emitted by the transformer,
/// but they were part of an earlier design and refusing them costs nothing.
fn is_crdt_meta_column(col: &str) -> bool {
    if col == HLC_TIMESTAMP_COLUMN || col == COLUMN_HLCS_COLUMN || col == COLUMN_SIGS_COLUMN {
        return true;
    }
    col.ends_with("__crdt_ts")
        || col.ends_with("__crdt_source")
        || col.ends_with("__crdt_ts_source")
}

/// Post-write signing pass: for every row that carries `haex_hlc == tx_hlc`
/// on `table_name`, sign each touched non-meta column with every key from
/// `key_cache` whose space owns the row, then persist the sig into
/// `haex_column_sigs`.
fn sign_written_rows(
    tx: &Transaction,
    key_cache: &SpaceKeyCache,
    table_name: &str,
    columns: &TouchedColumns,
) -> Result<(), DatabaseError> {
    let schema = get_table_schema(tx, table_name).map_err(|e| DatabaseError::DatabaseError {
        reason: format!("get_table_schema({table_name}) failed: {e}"),
    })?;
    if schema.is_empty() {
        return Ok(());
    }
    // Only sign if the target actually has the sig column (skips `_no_sync`
    // and system tables that don't carry CRDT meta).
    if !schema.iter().any(|c| c.name == COLUMN_SIGS_COLUMN) {
        return Ok(());
    }

    let is_meta =
        |c: &str| c == HLC_TIMESTAMP_COLUMN || c == COLUMN_HLCS_COLUMN || c == COLUMN_SIGS_COLUMN;

    let signable: Vec<String> = match columns {
        // Filter out CRDT meta columns and any columns not present in the
        // schema (defensive: parser can hand us anything).
        TouchedColumns::Explicit(cols) => {
            let schema_names: std::collections::HashSet<&str> =
                schema.iter().map(|c| c.name.as_str()).collect();
            cols.iter()
                .filter(|c| !is_meta(c) && schema_names.contains(c.as_str()))
                .cloned()
                .collect()
        }
        // A columnless `INSERT INTO t VALUES (…)` writes every column
        // positionally. On a CRDT table that necessarily includes the three
        // meta columns, which the caller must never supply — the statement
        // sidesteps `CrdtMetaColumnWriteForbidden` only because it names no
        // columns for that guard to inspect. Reject it here, where the schema
        // is available; `sign_written_rows` runs before `tx.commit()`, so the
        // write rolls back.
        TouchedColumns::AllColumns => {
            return Err(DatabaseError::CrdtMetaColumnWriteForbidden {
                column: format!(
                    "{COLUMN_SIGS_COLUMN} (columnless INSERT INTO \"{table_name}\" VALUES (…) \
                     assigns CRDT meta columns positionally — name the columns explicitly)"
                ),
            });
        }
    };
    if signable.is_empty() {
        return Ok(());
    }

    let pk_cols: Vec<String> = schema
        .iter()
        .filter(|c| c.is_pk)
        .map(|c| c.name.clone())
        .collect();
    if pk_cols.is_empty() {
        return Ok(());
    }

    // Read the transaction-scoped HLC — the transformer wrote it into
    // haex_hlc on every row it just touched, so we use it as the WHERE key.
    let tx_hlc: String = tx
        .query_row(&format!("SELECT {HLC_FUNCTION_NAME}()"), [], |r| r.get(0))
        .map_err(|e| DatabaseError::HlcError {
            reason: format!("current_hlc read for column-sign: {e}"),
        })?;

    let quoted_cols: Vec<String> = pk_cols
        .iter()
        .chain(signable.iter())
        .map(|c| format!("\"{c}\""))
        .collect();
    let select_sql = format!(
        "SELECT {} FROM \"{table_name}\" WHERE \"{HLC_TIMESTAMP_COLUMN}\" = ?1",
        quoted_cols.join(", ")
    );

    let register = RegisterLookup::new();
    let mut stmt = tx.prepare(&select_sql).map_err(DatabaseError::from)?;
    let mut rows = stmt
        .query([&tx_hlc as &dyn ToSql])
        .map_err(DatabaseError::from)?;

    while let Some(row) = rows.next().map_err(DatabaseError::from)? {
        let mut pk_map = serde_json::Map::with_capacity(pk_cols.len());
        for (i, col) in pk_cols.iter().enumerate() {
            let v: RusqliteValue = row.get(i).map_err(DatabaseError::from)?;
            pk_map.insert(col.clone(), sql_value_to_json(&v));
        }
        let row_pks_json = serde_json::to_string(&JsonValue::Object(pk_map)).map_err(|e| {
            DatabaseError::SerializationError {
                reason: format!("row_pks JSON: {e}"),
            }
        })?;

        for (idx, col) in signable.iter().enumerate() {
            let val: RusqliteValue = row.get(pk_cols.len() + idx).map_err(DatabaseError::from)?;
            let sig_map = sign_column_for_spaces(
                &*tx,
                key_cache,
                &register,
                table_name,
                &row_pks_json,
                col,
                &tx_hlc,
                &val,
            )
            .map_err(|e| DatabaseError::ExecutionError {
                sql: format!("column-sign {col} on {table_name}"),
                reason: e.to_string(),
                table: Some(table_name.to_string()),
            })?;
            for (space_id, rec) in sig_map {
                upsert_column_sigs(&*tx, table_name, &row_pks_json, col, &space_id, &rec)
                    .map_err(DatabaseError::from)?;
            }
        }
    }
    Ok(())
}

/// F2 — cross-table signing for freshly-inserted share-register rows.
///
/// For every row inserted into `haex_shared_space_sync` during this tx
/// (identified by matching `haex_hlc` against the tx HLC), do:
///
/// 1. **I1**: reject if `table_name` targets an unreviewed
///    Haex/SQLite/internal table (see
///    `column_sig::register_lookup::is_register_target_forbidden`).
/// 2. **I2**: reject if the vault does not hold a signing key for
///    `space_id_new` — the only way to legitimately author a share entry
///    for a space is to hold that space's member signing key. Sig-based
///    identity replaces the earlier `authored_by_did` DB lookup: no key
///    means not our space, and signing anyway would smuggle a foreign row
///    into that space.
/// 3. Load every non-meta column of the referenced row and sign it for
///    `space_id_new` using the local key. HLC per column comes from the row's
///    stored `haex_column_hlcs[col]` — we sign columns AS THEY EXIST, with
///    their historical timestamps, not the tx HLC of the register INSERT.
/// 4. Upsert each `(column, space_id_new)` sig into the row's `haex_column_sigs`.
///
/// Returns `Err(DatabaseError::I1…|I2…)` on violation — the caller's tx
/// scope aborts and rolls back the register-row insert too.
fn sign_share_insert_targets(
    tx: &Transaction,
    key_cache: &SpaceKeyCache,
) -> Result<(), DatabaseError> {
    // Guard: F2 needs the register carrying `haex_hlc` to identify the
    // just-inserted rows. Older fixtures / pre-migration tests may lack it —
    // treat as a no-op with a warn (schema drift or unmigrated fixture).
    let register_schema =
        get_table_schema(tx, REGISTER_TABLE).map_err(|e| DatabaseError::DatabaseError {
            reason: format!("get_table_schema({REGISTER_TABLE}) failed: {e}"),
        })?;
    let has_hlc = register_schema
        .iter()
        .any(|c| c.name == HLC_TIMESTAMP_COLUMN);
    if !has_hlc {
        tracing::warn!(
            target: "column_sig",
            register = REGISTER_TABLE,
            "F2 sig path skipped: share register is missing CRDT meta \
             (`haex_hlc`) — schema drift or unmigrated fixture. \
             Register INSERTs will not produce cross-table column sigs \
             until the schema catches up."
        );
        return Ok(());
    }

    let tx_hlc: String = tx
        .query_row(&format!("SELECT {HLC_FUNCTION_NAME}()"), [], |r| r.get(0))
        .map_err(|e| DatabaseError::HlcError {
            reason: format!("current_hlc read for share-insert: {e}"),
        })?;

    // Gather the freshly-inserted register rows into an owned Vec before
    // opening further prepared statements on the same connection.
    struct ShareRow {
        target_table: String,
        row_pks: String,
        space_id: String,
    }
    let share_rows: Vec<ShareRow> = {
        let mut stmt = tx
            .prepare(&format!(
                "SELECT table_name, row_pks, space_id \
                 FROM {REGISTER_TABLE} \
                 WHERE \"{HLC_TIMESTAMP_COLUMN}\" = ?1"
            ))
            .map_err(DatabaseError::from)?;
        let mut rows = stmt
            .query([&tx_hlc as &dyn ToSql])
            .map_err(DatabaseError::from)?;
        let mut out = Vec::new();
        while let Some(row) = rows.next().map_err(DatabaseError::from)? {
            out.push(ShareRow {
                target_table: row.get(0).map_err(DatabaseError::from)?,
                row_pks: row.get(1).map_err(DatabaseError::from)?,
                space_id: row.get(2).map_err(DatabaseError::from)?,
            });
        }
        out
    };

    for share in share_rows {
        // I1: never route through the register for system tables. The shared
        // fail-closed predicate keeps F1 and F2 aligned for existing and
        // future `haex_*` tables.
        if is_register_target_forbidden(&share.target_table)
            || !is_safe_identifier(&share.target_table)
        {
            return Err(DatabaseError::I1RegisterTargetsSystemTable {
                table: share.target_table,
            });
        }

        // I2: the vault must own a signing key for the declared space —
        // that key IS this vault's authorisation to author into the space.
        // No key → hard reject: register-INSERT would create a share row we
        // cannot cryptographically stand behind (Runde-5 sig-based I2).
        // Reload errors are treated as hard failure (schema is either dead
        // or the key was corrupted — both need the operator's attention).
        let signing_key = match key_cache.get_or_reload(&*tx, &share.space_id) {
            Ok(Some(k)) => k,
            Ok(None) | Err(_) => {
                return Err(DatabaseError::I2ForeignShareInsert {
                    space_id: share.space_id,
                });
            }
        };
        let derived_did = did_key_from_public_key(&signing_key.verifying_key());

        // Load target row.
        let schema = get_table_schema(&*tx, &share.target_table).map_err(|e| {
            DatabaseError::DatabaseError {
                reason: format!("get_table_schema({}) failed: {e}", share.target_table),
            }
        })?;
        if schema.is_empty() {
            continue;
        }
        // Row-sig column must exist on the target — otherwise the row has no
        // slot for the sig. Skip silently (the target is not sig-tracked).
        if !schema.iter().any(|c| c.name == COLUMN_SIGS_COLUMN) {
            continue;
        }

        // PK clause from row_pks JSON (canonicalised so key order does not matter).
        let (where_clause, pk_binds) = build_pk_where(&schema, &share.row_pks)?;
        if where_clause.is_empty() {
            // row_pks empty / mismatched → skip; caller-side validation should catch this.
            continue;
        }

        // SELECT all non-PK, non-CRDT-meta columns plus haex_column_hlcs for HLC lookup.
        let signable_cols: Vec<&str> = schema
            .iter()
            .filter(|c| {
                !c.is_pk
                    && c.name != HLC_TIMESTAMP_COLUMN
                    && c.name != COLUMN_HLCS_COLUMN
                    && c.name != COLUMN_SIGS_COLUMN
            })
            .map(|c| c.name.as_str())
            .collect();
        if signable_cols.is_empty() {
            continue;
        }

        let mut select_cols: Vec<String> =
            signable_cols.iter().map(|c| format!("\"{c}\"")).collect();
        select_cols.push(format!("\"{COLUMN_HLCS_COLUMN}\""));
        select_cols.push(format!("\"{HLC_TIMESTAMP_COLUMN}\""));
        let select_sql = format!(
            "SELECT {} FROM \"{}\" WHERE {} LIMIT 1",
            select_cols.join(", "),
            share.target_table,
            where_clause
        );

        let row_data: Option<Vec<RusqliteValue>> = {
            let mut stmt = tx.prepare(&select_sql).map_err(DatabaseError::from)?;
            let binds: Vec<&dyn ToSql> = pk_binds.iter().map(|v| v as &dyn ToSql).collect();
            stmt.query_row(&binds[..], |row| {
                let mut vals = Vec::with_capacity(select_cols.len());
                for i in 0..select_cols.len() {
                    vals.push(row.get::<_, RusqliteValue>(i)?);
                }
                Ok(vals)
            })
            .optional()
            .map_err(DatabaseError::from)?
        };
        let Some(values) = row_data else {
            // Referenced row does not exist yet — nothing to sign.
            continue;
        };

        // Extract per-column HLCs blob + row-level HLC (fallback).
        let column_hlcs_json = match &values[signable_cols.len()] {
            RusqliteValue::Text(s) => Some(s.clone()),
            _ => None,
        };
        let row_hlc = match &values[signable_cols.len() + 1] {
            RusqliteValue::Text(s) => Some(s.clone()),
            _ => None,
        };
        let per_column_hlcs: Option<serde_json::Map<String, JsonValue>> = column_hlcs_json
            .as_deref()
            .and_then(|s| serde_json::from_str::<JsonValue>(s).ok())
            .and_then(|v| match v {
                JsonValue::Object(m) => Some(m),
                _ => None,
            });

        for (idx, col) in signable_cols.iter().enumerate() {
            let val = &values[idx];
            let value_bytes_vec = value_bytes::to_canonical_bytes(val);

            let col_hlc = per_column_hlcs
                .as_ref()
                .and_then(|m| m.get(*col))
                .and_then(|v| v.as_str())
                .map(str::to_string)
                .or_else(|| row_hlc.clone());
            let Some(col_hlc) = col_hlc else {
                // No historical HLC available — cannot bind sig to a timestamp.
                continue;
            };

            let signature = sign_column(
                &signing_key,
                share.space_id.as_bytes(),
                share.target_table.as_bytes(),
                share.row_pks.as_bytes(),
                col.as_bytes(),
                col_hlc.as_bytes(),
                derived_did.as_bytes(),
                &value_bytes_vec,
            );
            upsert_column_sigs(
                &*tx,
                &share.target_table,
                &share.row_pks,
                col,
                &share.space_id,
                &SigRecord {
                    author_did: derived_did.clone(),
                    sig: signature.to_bytes(),
                    storage_class: value_bytes::StorageClass::of(val),
                },
            )
            .map_err(DatabaseError::from)?;
        }
    }
    Ok(())
}

/// Build a WHERE clause + bind vector matching every PK column of the target
/// row from a canonicalised `row_pks_json` payload.
///
/// Returns `(empty, [])` if the PK column set on `schema` and the object keys
/// in `row_pks_json` disagree — the caller treats that as a silent skip since
/// register rows with malformed PK payloads should not silently sign the wrong row.
fn build_pk_where(
    schema: &[crate::crdt::trigger::ColumnInfo],
    row_pks_json: &str,
) -> Result<(String, Vec<RusqliteValue>), DatabaseError> {
    let pk_cols: Vec<&str> = schema
        .iter()
        .filter(|c| c.is_pk)
        .map(|c| c.name.as_str())
        .collect();
    if pk_cols.is_empty() {
        return Ok((String::new(), Vec::new()));
    }

    let parsed: serde_json::Map<String, JsonValue> =
        match serde_json::from_str::<JsonValue>(row_pks_json) {
            Ok(JsonValue::Object(m)) => m,
            _ => return Ok((String::new(), Vec::new())),
        };
    if parsed.len() != pk_cols.len() {
        return Ok((String::new(), Vec::new()));
    }
    let mut parts = Vec::with_capacity(pk_cols.len());
    let mut binds = Vec::with_capacity(pk_cols.len());
    for col in &pk_cols {
        let Some(v) = parsed.get(*col) else {
            return Ok((String::new(), Vec::new()));
        };
        if !is_safe_identifier(col) {
            return Ok((String::new(), Vec::new()));
        }
        parts.push(format!("\"{col}\" = ?"));
        binds.push(json_pk_to_sql(v));
    }
    Ok((parts.join(" AND "), binds))
}

/// Convert a JSON PK value into the SQLite storage class most likely used at
/// row-write time. Matches `resolve_infra_row`'s conversion in
/// `register_lookup.rs`.
fn json_pk_to_sql(value: &JsonValue) -> RusqliteValue {
    match value {
        JsonValue::Null => RusqliteValue::Null,
        JsonValue::Bool(b) => RusqliteValue::Integer(i64::from(*b)),
        JsonValue::Number(n) => {
            if let Some(i) = n.as_i64() {
                RusqliteValue::Integer(i)
            } else if let Some(f) = n.as_f64() {
                RusqliteValue::Real(f)
            } else {
                RusqliteValue::Text(n.to_string())
            }
        }
        JsonValue::String(s) => RusqliteValue::Text(s.clone()),
        other => RusqliteValue::Text(other.to_string()),
    }
}

fn sql_value_to_json(v: &RusqliteValue) -> JsonValue {
    match v {
        RusqliteValue::Null => JsonValue::Null,
        RusqliteValue::Integer(i) => JsonValue::Number((*i).into()),
        RusqliteValue::Real(f) => serde_json::Number::from_f64(*f)
            .map(JsonValue::Number)
            .unwrap_or(JsonValue::Null),
        RusqliteValue::Text(s) => JsonValue::String(s.clone()),
        RusqliteValue::Blob(b) => JsonValue::String(BASE64.encode(b)),
    }
}

/// Execute SQL OHNE CRDT-Transformation.
///
/// Semantik: "no CRDT logic". Das heißt:
/// - Keine HLC-Population für INSERT/UPDATE (der CRDT-Transformer läuft nicht)
/// - Keine delete-log-Einträge für DELETE (BEFORE-DELETE-Trigger wird durch
///   `triggers_enabled='0'` umgangen)
/// - Kein dirty-table-Tracking
///
/// Der Trigger-Bypass wird transaktional durchgeführt: Flag setzen → Statement
/// ausführen → Flag zurücksetzen → commit. So sehen parallel laufende Sync-
/// Connections den Flag nie auf `'0'`.
pub fn execute(
    sql: String,
    params: Vec<JsonValue>,
    connection: &DbConnection,
) -> Result<Vec<Vec<JsonValue>>, DatabaseError> {
    let params_converted: Vec<RusqliteValue> = params
        .iter()
        .map(ValueConverter::json_to_rusqlite_value)
        .collect::<Result<Vec<_>, _>>()?;
    let params_sql: Vec<&dyn ToSql> = params_converted.iter().map(|v| v as &dyn ToSql).collect();

    let has_returning = {
        let stmt = parse_single_statement(&sql)?;
        statement_has_returning(&stmt)
    };

    with_connection(connection, |conn| {
        let tx = conn.transaction().map_err(DatabaseError::from)?;

        let disable_sql = format!(
            "INSERT INTO {TABLE_CRDT_CONFIGS} (key, type, value) VALUES ('triggers_enabled', 'system', '0')
             ON CONFLICT(key) DO UPDATE SET value = '0'"
        );
        tx.execute(&disable_sql, []).map_err(DatabaseError::from)?;

        let result = if has_returning {
            let mut result_vec: Vec<Vec<JsonValue>> = Vec::new();
            {
                let mut stmt = tx.prepare(&sql)?;
                let num_columns = stmt.column_count();
                let mut rows = stmt.query(&params_sql[..])?;

                while let Some(row) = rows.next()? {
                    let mut row_values: Vec<JsonValue> = Vec::with_capacity(num_columns);
                    for i in 0..num_columns {
                        let value_ref = row.get_ref(i)?;
                        let json_val = convert_value_ref_to_json(value_ref)?;
                        row_values.push(json_val);
                    }
                    result_vec.push(row_values);
                }
            }
            result_vec
        } else {
            tx.execute(&sql, &params_sql[..]).map_err(|e| {
                let table_name = extract_primary_table_name_from_sql(&sql).unwrap_or(None);
                DatabaseError::ExecutionError {
                    sql: sql.clone(),
                    reason: e.to_string(),
                    table: table_name,
                }
            })?;
            vec![]
        };

        let enable_sql = format!(
            "INSERT INTO {TABLE_CRDT_CONFIGS} (key, type, value) VALUES ('triggers_enabled', 'system', '1')
             ON CONFLICT(key) DO UPDATE SET value = '1'"
        );
        tx.execute(&enable_sql, []).map_err(DatabaseError::from)?;

        tx.commit().map_err(DatabaseError::from)?;
        Ok(result)
    })
}

#[cfg(test)]
#[path = "../core_max_tx_size_tests.rs"]
mod max_tx_size_tests;

#[cfg(test)]
#[path = "../core_execute_tests.rs"]
mod execute_tests;
