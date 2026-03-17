use serde_json::Value as JsonValue;
use tauri::State;

use crate::database::core::with_connection;
use crate::database::error::DatabaseError;
use crate::AppState;

use super::{LogEntry, LogLevel, LogQueryParams, get_effective_log_level, insert_log, query_logs, cleanup_logs};

/// Write a system log entry.
#[tauri::command]
pub fn log_write_system(
    state: State<'_, AppState>,
    level: String,
    source: String,
    message: String,
    metadata: Option<JsonValue>,
    device_id: String,
) -> Result<(), DatabaseError> {
    let log_level = LogLevel::from_str(&level)
        .ok_or_else(|| DatabaseError::ValidationError { reason: format!("Invalid log level: {level}") })?;

    with_connection(&state.db, |conn| {
        if log_level < get_effective_log_level(conn, None) {
            return Ok(());
        }
        insert_log(conn, &level, &source, None, &message, metadata, &device_id)
    })
}

/// Read logs (system has full access to all logs).
#[tauri::command]
pub fn log_read(
    state: State<'_, AppState>,
    query: LogQueryParams,
) -> Result<Vec<LogEntry>, DatabaseError> {
    with_connection(&state.db, |conn| {
        query_logs(conn, &query)
    })
}

/// Clean up old log entries based on retention settings.
#[tauri::command]
pub fn log_cleanup(
    state: State<'_, AppState>,
) -> Result<usize, DatabaseError> {
    with_connection(&state.db, |conn| {
        cleanup_logs(conn)
    })
}

/// Delete specific log entries by ID.
#[tauri::command]
pub fn log_delete(
    state: State<'_, AppState>,
    ids: Vec<String>,
) -> Result<usize, DatabaseError> {
    if ids.is_empty() {
        return Ok(0);
    }
    with_connection(&state.db, |conn| {
        let placeholders: Vec<String> = ids.iter().enumerate().map(|(i, _)| format!("?{}", i + 1)).collect();
        let sql = format!(
            "DELETE FROM {} WHERE id IN ({})",
            crate::table_names::TABLE_LOGS,
            placeholders.join(",")
        );
        let params: Vec<Box<dyn rusqlite::types::ToSql>> = ids.iter().map(|id| Box::new(id.clone()) as Box<dyn rusqlite::types::ToSql>).collect();
        let param_refs: Vec<&dyn rusqlite::types::ToSql> = params.iter().map(|p| p.as_ref()).collect();
        let deleted = conn.execute(&sql, param_refs.as_slice())
            .map_err(|e| DatabaseError::ExecutionError {
                sql: "DELETE logs".into(),
                reason: e.to_string(),
                table: Some("haex_logs".into()),
            })?;
        Ok(deleted)
    })
}

/// Delete all log entries.
#[tauri::command]
pub fn log_clear_all(
    state: State<'_, AppState>,
) -> Result<usize, DatabaseError> {
    with_connection(&state.db, |conn| {
        let deleted = conn.execute(
            &format!("DELETE FROM {}", crate::table_names::TABLE_LOGS),
            [],
        ).map_err(|e| DatabaseError::ExecutionError {
            sql: "DELETE all logs".into(),
            reason: e.to_string(),
            table: Some("haex_logs".into()),
        })?;
        Ok(deleted)
    })
}
