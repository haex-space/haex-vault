use serde_json::Value as JsonValue;
use tauri::State;

use crate::database::core::with_connection;
use crate::database::error::DatabaseError;
use crate::AppState;

use super::{LogEntry, LogLevel, LogQueryParams, get_effective_log_level, insert_log, query_logs};

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
        insert_log(conn, &level, &source, "system", &message, metadata, &device_id)
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
