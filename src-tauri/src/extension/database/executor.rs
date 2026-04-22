// src-tauri/src/extension/database/executor.rs

use super::planner::SqlExecutionPlanner;
use crate::crdt::hlc::{HlcError, HlcService};
use crate::crdt::transformer::CrdtTransformer;
use crate::crdt::trigger::HLC_FUNCTION_NAME;
use crate::database::core::{convert_value_ref_to_json, strip_main_schema_prefix};
use crate::database::error::DatabaseError;
use rusqlite::{params_from_iter, ToSql, Transaction};
use serde_json::Value as JsonValue;
use std::collections::HashSet;
use std::str::FromStr;
use uhlc::Timestamp;

/// Returns the transaction-scoped HLC for this statement. All statements that
/// run inside the same `Transaction` get the same timestamp because the
/// `current_hlc()` UDF caches it in the per-connection slot until commit or
/// rollback clears it.
///
/// The returned timestamp is also fed back into the HLC service (so the clock
/// keeps advancing across transactions) and persisted to `haex_crdt_configs`.
fn tx_scoped_hlc(
    tx: &Transaction,
    hlc_service: &HlcService,
) -> Result<Timestamp, DatabaseError> {
    let hlc_str: String = tx
        .query_row(&format!("SELECT {HLC_FUNCTION_NAME}()"), [], |row| row.get(0))
        .map_err(|e| DatabaseError::HlcError {
            reason: format!("Failed to read {HLC_FUNCTION_NAME}(): {e}"),
        })?;

    let timestamp = Timestamp::from_str(&hlc_str).map_err(|e| DatabaseError::HlcError {
        reason: format!("Invalid HLC from UDF: {e:?}"),
    })?;

    // Align the HlcService instance with the tx-slot value so later code paths
    // that draw from hlc_service keep monotonic with what we just handed out.
    hlc_service
        .update_with_timestamp(&timestamp)
        .map_err(|e: HlcError| DatabaseError::HlcError { reason: e.to_string() })?;

    HlcService::persist_timestamp(tx, &timestamp).map_err(|e| DatabaseError::HlcError {
        reason: e.to_string(),
    })?;

    Ok(timestamp)
}

/// SQL-Executor OHNE Berechtigungsprüfung - für interne Nutzung
pub struct SqlExecutor;

impl SqlExecutor {
    /// Führt ein SQL Statement OHNE RETURNING aus (mit CRDT)
    /// Returns: modified_schema_tables
    ///
    /// Note: This function does NOT automatically create CRDT triggers for CREATE TABLE.
    /// The caller is responsible for setting up triggers using `trigger::setup_triggers_for_table`
    /// when needed (e.g., for production extensions but not for dev mode extensions).
    pub fn execute_internal_typed(
        tx: &Transaction,
        hlc_service: &HlcService,
        sql: &str,
        params: &[&dyn ToSql],
    ) -> Result<HashSet<String>, DatabaseError> {
        let mut statement = SqlExecutionPlanner::parse_single_statement(sql)?;

        let transformer = CrdtTransformer::new();
        let hlc_timestamp = tx_scoped_hlc(tx, hlc_service)?;

        let mut modified_schema_tables = HashSet::new();
        if let Some(table_name) =
            transformer.transform_execute_statement(&mut statement, &hlc_timestamp)?
        {
            modified_schema_tables.insert(table_name);
        }

        // Remove "main." schema prefix that sqlparser adds
        let raw_sql = statement.to_string();
        let sql_str = strip_main_schema_prefix(&raw_sql);

        // Führe Statement aus
        tx.execute(&sql_str, params)
            .map_err(|e| DatabaseError::ExecutionError {
                sql: sql_str.clone(),
                table: None,
                reason: format!("Execute failed: {e}"),
            })?;

        Ok(modified_schema_tables)
    }

    /// Führt ein SQL Statement MIT RETURNING aus (mit CRDT)
    /// Returns: (modified_schema_tables, returning_results)
    pub fn query_internal_typed(
        tx: &Transaction,
        hlc_service: &HlcService,
        sql: &str,
        params: &[&dyn ToSql],
    ) -> Result<(HashSet<String>, Vec<Vec<JsonValue>>), DatabaseError> {
        let mut statement = SqlExecutionPlanner::parse_single_statement(sql)?;

        let transformer = CrdtTransformer::new();
        let hlc_timestamp = tx_scoped_hlc(tx, hlc_service)?;

        let mut modified_schema_tables = HashSet::new();
        if let Some(table_name) =
            transformer.transform_execute_statement(&mut statement, &hlc_timestamp)?
        {
            modified_schema_tables.insert(table_name);
        }

        // Remove "main." schema prefix that sqlparser adds
        let raw_sql = statement.to_string();
        let sql_str = strip_main_schema_prefix(&raw_sql);

        // Prepare und query ausführen
        let mut stmt = tx
            .prepare(&sql_str)
            .map_err(|e| DatabaseError::ExecutionError {
                sql: sql_str.clone(),
                table: None,
                reason: e.to_string(),
            })?;

        let column_names: Vec<String> = stmt
            .column_names()
            .into_iter()
            .map(|s| s.to_string())
            .collect();
        let num_columns = column_names.len();

        let mut rows = stmt.query(params_from_iter(params.iter())).map_err(|e| {
            DatabaseError::ExecutionError {
                sql: sql_str.clone(),
                table: None,
                reason: e.to_string(),
            }
        })?;

        let mut result_vec: Vec<Vec<JsonValue>> = Vec::new();

        // Lese alle RETURNING Zeilen
        while let Some(row) = rows.next().map_err(|e| DatabaseError::ExecutionError {
            sql: sql_str.clone(),
            table: None,
            reason: e.to_string(),
        })? {
            let mut row_values: Vec<JsonValue> = Vec::new();
            for i in 0..num_columns {
                let value_ref = row.get_ref(i).map_err(|e| DatabaseError::ExecutionError {
                    sql: sql_str.clone(),
                    table: None,
                    reason: e.to_string(),
                })?;
                let json_value = convert_value_ref_to_json(value_ref)?;
                row_values.push(json_value);
            }
            result_vec.push(row_values);
        }

        Ok((modified_schema_tables, result_vec))
    }

    /// Führt ein einzelnes SQL Statement OHNE Typinformationen aus (JSON params)
    pub fn execute_internal(
        tx: &Transaction,
        hlc_service: &HlcService,
        sql: &str,
        params: &[JsonValue],
    ) -> Result<HashSet<String>, DatabaseError> {
        let sql_params = SqlExecutionPlanner::convert_params(params)?;
        let param_refs: Vec<&dyn ToSql> = sql_params.iter().map(|p| p as &dyn ToSql).collect();
        Self::execute_internal_typed(tx, hlc_service, sql, &param_refs)
    }

    /// Query-Variante (mit RETURNING) OHNE Typinformationen (JSON params)
    pub fn query_internal(
        tx: &Transaction,
        hlc_service: &HlcService,
        sql: &str,
        params: &[JsonValue],
    ) -> Result<(HashSet<String>, Vec<Vec<JsonValue>>), DatabaseError> {
        let sql_params = SqlExecutionPlanner::convert_params(params)?;
        let param_refs: Vec<&dyn ToSql> = sql_params.iter().map(|p| p as &dyn ToSql).collect();
        Self::query_internal_typed(tx, hlc_service, sql, &param_refs)
    }

    /// Query für SELECT-Statements (read-only, kein CRDT nötig außer Filter)
    #[allow(dead_code)]
    pub fn query_select(
        conn: &rusqlite::Connection,
        sql: &str,
        params: &[JsonValue],
    ) -> Result<Vec<Vec<JsonValue>>, DatabaseError> {
        // Use planner for safe parsing
        let stmt_to_execute = SqlExecutionPlanner::parse_single_statement(sql)?;
        let raw_sql = stmt_to_execute.to_string();
        let transformed_sql = strip_main_schema_prefix(&raw_sql);

        // Convert JSON params to SQLite values using planner
        let sql_params = SqlExecutionPlanner::convert_params(params)?;

        let mut prepared_stmt = conn.prepare(&transformed_sql)?;

        let num_columns = prepared_stmt.column_count();

        let param_refs: Vec<&dyn ToSql> = sql_params.iter().map(|p| p as &dyn ToSql).collect();

        let mut rows = prepared_stmt.query(params_from_iter(param_refs.iter()))?;

        let mut result: Vec<Vec<JsonValue>> = Vec::new();
        while let Some(row) = rows.next()? {
            let mut row_values: Vec<JsonValue> = Vec::new();
            for i in 0..num_columns {
                let value_ref = row.get_ref(i)?;
                let json_value = convert_value_ref_to_json(value_ref)?;
                row_values.push(json_value);
            }
            result.push(row_values);
        }

        Ok(result)
    }
}
