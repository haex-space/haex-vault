// src-tauri/src/database/core/execute.rs

use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use rusqlite::types::Value as RusqliteValue;
use rusqlite::{ToSql, Transaction};
use serde_json::Value as JsonValue;
use sqlparser::ast::{AssignmentTarget, ObjectName, Statement, TableFactor, TableObject};

use crate::crdt::column_sig::key_cache::SpaceKeyCache;
use crate::crdt::column_sig::register_lookup::RegisterLookup;
use crate::crdt::column_sig::storage::upsert_column_sigs;
use crate::crdt::column_sig::write::sign_column_for_spaces;
use crate::crdt::trigger::{
    get_table_schema, COLUMN_HLCS_COLUMN, COLUMN_SIGS_COLUMN, HLC_FUNCTION_NAME,
    HLC_TIMESTAMP_COLUMN,
};
use crate::database::core::connection::with_connection;
use crate::database::core::extract::extract_primary_table_name_from_sql;
use crate::database::core::parsing::{parse_single_statement, statement_has_returning};
use crate::database::core::value::{convert_value_ref_to_json, ValueConverter};
use crate::database::error::DatabaseError;
use crate::database::DbConnection;
use crate::extension::database::executor::SqlExecutor;
use crate::table_names::TABLE_CRDT_CONFIGS;

/// Table F2 owns (haex_shared_space_sync register) — signing there needs the
/// dedicated F2 flow so we skip it here to keep F1 orthogonal.
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

        if let Some((table_name, columns)) = touched {
            sign_written_rows(&tx, key_cache, &table_name, &columns)?;
        }

        tx.commit().map_err(DatabaseError::from)?;
        Ok(result)
    })
}

/// Extracts `(table_name, touched_columns)` for statements that carry column
/// writes; returns `None` for statements the signer doesn't handle
/// (SELECT/DELETE/DDL).
fn extract_touched_for_signing(stmt: &Statement) -> Option<(String, Vec<String>)> {
    match stmt {
        Statement::Insert(insert) => {
            let name = match &insert.table {
                TableObject::TableName(n) => object_name_last(n)?,
                _ => return None,
            };
            let cols: Vec<String> = insert
                .columns
                .iter()
                .filter_map(|obj| object_name_last(obj))
                .collect();
            Some((name, cols))
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
            Some((name, cols))
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

/// Post-write signing pass: for every row that carries `haex_hlc == tx_hlc`
/// on `table_name`, sign each touched non-meta column with every key from
/// `key_cache` whose space owns the row, then persist the sig into
/// `haex_column_sigs`.
fn sign_written_rows(
    tx: &Transaction,
    key_cache: &SpaceKeyCache,
    table_name: &str,
    columns: &[String],
) -> Result<(), DatabaseError> {
    // F2 territory — the register itself carries its own signing flow.
    if table_name.eq_ignore_ascii_case(REGISTER_TABLE) {
        return Ok(());
    }

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

    // Filter out CRDT meta columns and any columns not present in the schema
    // (defensive: parser can hand us anything).
    let schema_names: std::collections::HashSet<&str> =
        schema.iter().map(|c| c.name.as_str()).collect();
    let signable: Vec<String> = columns
        .iter()
        .filter(|c| {
            c.as_str() != HLC_TIMESTAMP_COLUMN
                && c.as_str() != COLUMN_HLCS_COLUMN
                && c.as_str() != COLUMN_SIGS_COLUMN
                && schema_names.contains(c.as_str())
        })
        .cloned()
        .collect();
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
