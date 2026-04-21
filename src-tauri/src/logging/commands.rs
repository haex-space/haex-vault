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

    let should_log = with_connection(&state.db, |conn| {
        Ok(log_level >= get_effective_log_level(conn, None))
    })?;

    if !should_log {
        return Ok(());
    }

    insert_log(&state, &level, &source, None, &message, metadata, &device_id)
}

/// Read logs (system has full access to all logs).
#[tauri::command]
pub fn log_read(
    state: State<'_, AppState>,
    query: LogQueryParams,
) -> Result<Vec<LogEntry>, DatabaseError> {
    query_logs(&state.db, &query)
}

/// Clean up old log entries based on retention settings.
#[tauri::command]
pub fn log_cleanup(
    state: State<'_, AppState>,
) -> Result<usize, DatabaseError> {
    cleanup_logs(&state)
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

    let hlc = state.hlc.lock().map_err(|_| DatabaseError::ValidationError {
        reason: "Failed to lock HLC service".to_string(),
    })?;

    let mut total_deleted = 0;
    for id in &ids {
        let sql = format!("DELETE FROM {} WHERE id = ?1", crate::table_names::TABLE_LOGS);
        crate::database::core::execute_with_crdt(
            sql,
            vec![JsonValue::String(id.clone())],
            &state.db,
            &hlc,
        )?;
        total_deleted += 1;
    }
    Ok(total_deleted)
}

/// Delete all log entries.
#[tauri::command]
pub fn log_clear_all(
    state: State<'_, AppState>,
) -> Result<usize, DatabaseError> {
    let hlc = state.hlc.lock().map_err(|_| DatabaseError::ValidationError {
        reason: "Failed to lock HLC service".to_string(),
    })?;

    let ids: Vec<String> = with_connection(&state.db, |conn| {
        let sql = format!("SELECT id FROM {}", crate::table_names::TABLE_LOGS);
        let mut stmt = conn.prepare(&sql).map_err(|e| DatabaseError::QueryError { reason: e.to_string() })?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0))
            .map_err(|e| DatabaseError::QueryError { reason: e.to_string() })?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| DatabaseError::QueryError { reason: e.to_string() })
    })?;

    let count = ids.len();
    for id in ids {
        let sql = format!("DELETE FROM {} WHERE id = ?1", crate::table_names::TABLE_LOGS);
        crate::database::core::execute_with_crdt(
            sql,
            vec![JsonValue::String(id)],
            &state.db,
            &hlc,
        )?;
    }
    Ok(count)
}
