// src-tauri/src/database/core/execute.rs

use crate::database::core::connection::with_connection;
use crate::database::core::extract::extract_primary_table_name_from_sql;
use crate::database::core::parsing::{parse_single_statement, statement_has_returning};
use crate::database::core::value::{convert_value_ref_to_json, ValueConverter};
use crate::database::error::DatabaseError;
use crate::database::DbConnection;
use crate::extension::database::executor::SqlExecutor;
use crate::table_names::TABLE_CRDT_CONFIGS;
use rusqlite::types::Value as RusqliteValue;
use rusqlite::ToSql;
use serde_json::Value as JsonValue;

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

/// Execute SQL mit CRDT-Transformation (für Drizzle-Integration)
/// Diese Funktion sollte von Drizzle verwendet werden, um CRDT-Support zu erhalten
/// Unterstützt RETURNING-Klausel: Falls vorhanden, werden die Ergebnis-Rows zurückgegeben
pub fn execute_with_crdt(
    sql: String,
    params: Vec<JsonValue>,
    connection: &DbConnection,
    hlc_service: &std::sync::MutexGuard<crate::crdt::hlc::HlcService>,
) -> Result<Vec<Vec<JsonValue>>, DatabaseError> {
    // ADR 0001: reject an oversized single transaction before writing anything.
    if let Some(bytes) = write_payload_too_large(&params, MAX_CRDT_TRANSACTION_BYTES) {
        return Err(DatabaseError::TransactionTooLarge {
            bytes,
            limit: MAX_CRDT_TRANSACTION_BYTES,
        });
    }

    // Parse statement to check for RETURNING clause (AST-basiert)
    let statement = parse_single_statement(&sql)?;
    let has_returning = statement_has_returning(&statement);

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

        tx.commit().map_err(DatabaseError::from)?;
        Ok(result)
    })
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
