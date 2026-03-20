use serde_json::Value as JsonValue;
use tauri::State;

use crate::database::core::with_connection;
use crate::database::error::DatabaseError;
use crate::logging::{LogEntry, LogLevel, LogQueryParams, get_effective_log_level, insert_log, query_logs};
use crate::AppState;

/// Write an extension log entry.
/// The extension_id is set server-side — extensions cannot spoof their source.
#[tauri::command]
pub fn extension_logging_write(
    state: State<'_, AppState>,
    level: String,
    extension_id: String,
    message: String,
    metadata: Option<JsonValue>,
    device_id: String,
) -> Result<(), DatabaseError> {
    let log_level = LogLevel::from_str(&level)
        .ok_or_else(|| DatabaseError::ValidationError { reason: format!("Invalid log level: {level}") })?;

    let should_log = with_connection(&state.db, |conn| {
        Ok(log_level >= get_effective_log_level(conn, Some(&extension_id)))
    })?;

    if !should_log {
        return Ok(());
    }

    insert_log(&state, &level, &extension_id, Some(&extension_id), &message, metadata, &device_id)
}

/// Read extension logs — only returns logs for the requesting extension.
#[tauri::command]
pub fn extension_logging_read(
    state: State<'_, AppState>,
    extension_id: String,
    query: LogQueryParams,
) -> Result<Vec<LogEntry>, DatabaseError> {
    let mut filtered = query;
    filtered.extension_id = Some(extension_id);

    with_connection(&state.db, |conn| {
        query_logs(conn, &filtered)
    })
}
