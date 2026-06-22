use serde_json::Value as JsonValue;
use tauri::{State, WebviewWindow};

use crate::database::core::with_connection;
use crate::database::error::DatabaseError;
use crate::extension::utils::resolve_extension_id;
use crate::logging::{
    get_effective_log_level, insert_log, query_logs, LogEntry, LogLevel, LogQueryParams,
};
use crate::AppState;

/// Resolve the calling extension's id from the (unspoofable) window context, or
/// from frontend-verified public_key/name for iframe extensions. Maps the
/// extension error into a `DatabaseError` so the command's return type stays stable.
fn resolve_caller(
    window: &WebviewWindow,
    state: &State<'_, AppState>,
    public_key: Option<String>,
    name: Option<String>,
) -> Result<String, DatabaseError> {
    resolve_extension_id(window, state, public_key, name).map_err(|e| {
        DatabaseError::ValidationError {
            reason: e.to_string(),
        }
    })
}

/// Write an extension log entry.
///
/// SECURITY: the source extension id is resolved server-side via
/// `resolve_extension_id` (window label for WebView extensions, frontend-verified
/// public_key/name for iframe extensions). An extension cannot spoof its source
/// by passing an arbitrary id — it can only write logs attributed to itself.
#[tauri::command(rename_all = "camelCase")]
pub fn extension_logging_write(
    window: WebviewWindow,
    state: State<'_, AppState>,
    level: String,
    message: String,
    metadata: Option<JsonValue>,
    device_id: String,
    public_key: Option<String>,
    name: Option<String>,
) -> Result<(), DatabaseError> {
    let extension_id = resolve_caller(&window, &state, public_key, name)?;

    let log_level = LogLevel::from_str(&level).ok_or_else(|| DatabaseError::ValidationError {
        reason: format!("Invalid log level: {level}"),
    })?;

    let should_log = with_connection(&state.db, |conn| {
        Ok(log_level >= get_effective_log_level(conn, Some(&extension_id)))
    })?;

    if !should_log {
        return Ok(());
    }

    insert_log(
        &state,
        &level,
        &extension_id,
        Some(&extension_id),
        &message,
        metadata,
        &device_id,
    )
}

/// Read extension logs — only returns logs for the requesting extension.
///
/// SECURITY: the extension id is resolved server-side (see
/// `extension_logging_write`); an extension can only read its OWN logs and
/// cannot pass another extension's id to read theirs.
#[tauri::command(rename_all = "camelCase")]
pub fn extension_logging_read(
    window: WebviewWindow,
    state: State<'_, AppState>,
    query: LogQueryParams,
    public_key: Option<String>,
    name: Option<String>,
) -> Result<Vec<LogEntry>, DatabaseError> {
    let extension_id = resolve_caller(&window, &state, public_key, name)?;

    let mut filtered = query;
    filtered.extension_id = Some(extension_id);

    query_logs(&state.db, &filtered)
}
