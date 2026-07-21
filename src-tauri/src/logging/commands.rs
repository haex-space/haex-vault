use serde_json::Value as JsonValue;
use tauri::State;

use crate::database::core::with_connection;
use crate::database::error::DatabaseError;
use crate::AppState;

use super::{
    cleanup_logs, count_logs, get_effective_log_level, insert_log, query_logs, LogEntry, LogLevel,
    LogQueryParams,
};

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
    let log_level = LogLevel::from_str(&level).ok_or_else(|| DatabaseError::ValidationError {
        reason: format!("Invalid log level: {level}"),
    })?;

    let should_log = with_connection(&state.db, |conn| {
        Ok(log_level >= get_effective_log_level(conn, None))
    })?;

    if !should_log {
        return Ok(());
    }

    insert_log(
        &state, &level, &source, None, &message, metadata, &device_id,
    )
}

/// Read logs (system has full access to all logs).
#[tauri::command]
pub fn log_read(
    state: State<'_, AppState>,
    query: LogQueryParams,
) -> Result<Vec<LogEntry>, DatabaseError> {
    query_logs(&state.db, &query)
}

/// Count logs matching the given filters. Used by paginated UIs.
#[tauri::command]
pub fn log_count(state: State<'_, AppState>, query: LogQueryParams) -> Result<i64, DatabaseError> {
    count_logs(&state.db, &query)
}

/// Clean up old log entries based on retention settings.
#[tauri::command]
pub fn log_cleanup(state: State<'_, AppState>) -> Result<usize, DatabaseError> {
    cleanup_logs(&state)
}

/// Delete specific log entries by ID.
#[tauri::command]
pub fn log_delete(state: State<'_, AppState>, ids: Vec<String>) -> Result<usize, DatabaseError> {
    if ids.is_empty() {
        return Ok(0);
    }

    let sink_guard = state
        .log_sink
        .lock()
        .map_err(|e| DatabaseError::LockError {
            reason: e.to_string(),
        })?;
    let Some(sink) = sink_guard.as_ref() else {
        return Ok(0);
    };

    let mut total_deleted = 0usize;
    for id in &ids {
        let sql = format!(
            "DELETE FROM {} WHERE id = ?1",
            crate::table_names::TABLE_LOGS
        );
        total_deleted += sink
            .execute(&sql, &[JsonValue::String(id.clone())])
            .map_err(|e| DatabaseError::DatabaseError {
                reason: format!("log sink delete failed: {e}"),
            })?;
    }
    Ok(total_deleted)
}

/// Delete all log entries.
#[tauri::command]
pub fn log_clear_all(state: State<'_, AppState>) -> Result<usize, DatabaseError> {
    let sink_guard = state
        .log_sink
        .lock()
        .map_err(|e| DatabaseError::LockError {
            reason: e.to_string(),
        })?;
    let Some(sink) = sink_guard.as_ref() else {
        return Ok(0);
    };

    let sql = format!("DELETE FROM {}", crate::table_names::TABLE_LOGS);
    sink.execute(&sql, &[])
        .map_err(|e| DatabaseError::DatabaseError {
            reason: format!("log sink clear_all failed: {e}"),
        })
}
