// src-tauri/src/extension/limits/commands.rs
//!
//! Tauri commands for extension limit configuration

use crate::database::core::with_connection;
use crate::database::error::DatabaseError;
use crate::extension::database::executor::SqlExecutor;
use crate::extension::error::ExtensionError;
use crate::extension::limits::ExtensionLimits;
use crate::AppState;
use serde::{Deserialize, Serialize};
use tauri::State;
use ts_rs::TS;

/// Request to update extension limits
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct UpdateExtensionLimitsRequest {
    pub extension_id: String,
    /// Query timeout in milliseconds (optional, keeps current value if not provided)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub query_timeout_ms: Option<i64>,
    /// Maximum result rows per query (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_result_rows: Option<i64>,
    /// Maximum concurrent queries (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_concurrent_queries: Option<i64>,
    /// Maximum query SQL size in bytes (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_query_size_bytes: Option<i64>,
}

/// Response with extension limits
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct ExtensionLimitsResponse {
    pub extension_id: String,
    pub query_timeout_ms: i64,
    pub max_result_rows: i64,
    pub max_concurrent_queries: i64,
    pub max_query_size_bytes: i64,
    /// Whether custom limits are configured (false = using defaults)
    pub is_custom: bool,
}

impl ExtensionLimitsResponse {
    pub fn from_limits(extension_id: String, limits: ExtensionLimits, is_custom: bool) -> Self {
        Self {
            extension_id,
            query_timeout_ms: limits.database.query_timeout_ms,
            max_result_rows: limits.database.max_result_rows,
            max_concurrent_queries: limits.database.max_concurrent_queries,
            max_query_size_bytes: limits.database.max_query_size_bytes,
            is_custom,
        }
    }
}

/// Get limits for an extension
#[tauri::command]
pub fn get_extension_limits(
    state: State<'_, AppState>,
    extension_id: String,
) -> Result<ExtensionLimitsResponse, ExtensionError> {
    // Check if extension exists
    let _extension = state
        .extension_manager
        .get_extension(&extension_id)
        .ok_or_else(|| ExtensionError::ValidationError {
            reason: format!("Extension with ID {} not found", extension_id),
        })?;

    // Try to get custom limits from database
    let (limits, is_custom) = with_connection(&state.db, |conn| {
        // Check if custom limits exist
        let custom_exists: bool = conn
            .query_row(
                "SELECT 1 FROM haex_extension_limits WHERE extension_id = ? AND IFNULL(haex_tombstone, 0) = 0",
                [&extension_id],
                |_| Ok(true),
            )
            .unwrap_or(false);

        let limits = state.limits.get_limits(conn, &extension_id)?;
        Ok((limits, custom_exists))
    })?;

    Ok(ExtensionLimitsResponse::from_limits(
        extension_id,
        limits,
        is_custom,
    ))
}

/// Update limits for an extension
#[tauri::command]
pub fn update_extension_limits(
    state: State<'_, AppState>,
    request: UpdateExtensionLimitsRequest,
) -> Result<ExtensionLimitsResponse, ExtensionError> {
    // Check if extension exists
    let _extension = state
        .extension_manager
        .get_extension(&request.extension_id)
        .ok_or_else(|| ExtensionError::ValidationError {
            reason: format!("Extension with ID {} not found", request.extension_id),
        })?;

    // Get current limits (or defaults)
    let current_limits = with_connection(&state.db, |conn| {
        state.limits.get_limits(conn, &request.extension_id)
    })?;

    // Merge with update request
    let new_query_timeout = request
        .query_timeout_ms
        .unwrap_or(current_limits.database.query_timeout_ms);
    let new_max_result_rows = request
        .max_result_rows
        .unwrap_or(current_limits.database.max_result_rows);
    let new_max_concurrent_queries = request
        .max_concurrent_queries
        .unwrap_or(current_limits.database.max_concurrent_queries);
    let new_max_query_size_bytes = request
        .max_query_size_bytes
        .unwrap_or(current_limits.database.max_query_size_bytes);

    // Validate limits
    if new_query_timeout < 1000 {
        return Err(ExtensionError::ValidationError {
            reason: "Query timeout must be at least 1000ms (1 second)".to_string(),
        });
    }
    if new_max_result_rows < 100 {
        return Err(ExtensionError::ValidationError {
            reason: "Max result rows must be at least 100".to_string(),
        });
    }
    if new_max_concurrent_queries < 1 {
        return Err(ExtensionError::ValidationError {
            reason: "Max concurrent queries must be at least 1".to_string(),
        });
    }
    if new_max_query_size_bytes < 1024 {
        return Err(ExtensionError::ValidationError {
            reason: "Max query size must be at least 1024 bytes (1KB)".to_string(),
        });
    }

    // Insert or update in database using CRDT executor
    with_connection(&state.db, |conn| {
        let tx = conn.transaction()?;

        let hlc_service = state.hlc.lock().map_err(|_| DatabaseError::MutexPoisoned {
            reason: "Failed to lock HLC service".to_string(),
        })?;

        // Check if record exists
        let exists: bool = tx
            .query_row(
                "SELECT 1 FROM haex_extension_limits WHERE extension_id = ? AND IFNULL(haex_tombstone, 0) = 0",
                [&request.extension_id],
                |_| Ok(true),
            )
            .unwrap_or(false);

        if exists {
            // Update existing record
            let sql = "UPDATE haex_extension_limits SET \
                query_timeout_ms = ?, \
                max_result_rows = ?, \
                max_concurrent_queries = ?, \
                max_query_size_bytes = ? \
                WHERE extension_id = ?";

            let params: Vec<serde_json::Value> = vec![
                serde_json::json!(new_query_timeout),
                serde_json::json!(new_max_result_rows),
                serde_json::json!(new_max_concurrent_queries),
                serde_json::json!(new_max_query_size_bytes),
                serde_json::json!(request.extension_id),
            ];

            SqlExecutor::execute_internal(&tx, &hlc_service, sql, &params)?;
        } else {
            // Insert new record
            let id = uuid::Uuid::new_v4().to_string();
            let sql = "INSERT INTO haex_extension_limits \
                (id, extension_id, query_timeout_ms, max_result_rows, max_concurrent_queries, max_query_size_bytes) \
                VALUES (?, ?, ?, ?, ?, ?)";

            let params: Vec<serde_json::Value> = vec![
                serde_json::json!(id),
                serde_json::json!(request.extension_id),
                serde_json::json!(new_query_timeout),
                serde_json::json!(new_max_result_rows),
                serde_json::json!(new_max_concurrent_queries),
                serde_json::json!(new_max_query_size_bytes),
            ];

            SqlExecutor::execute_internal(&tx, &hlc_service, sql, &params)?;
        }

        tx.commit()?;
        Ok::<(), DatabaseError>(())
    })?;

    // Return updated limits
    let updated_limits = with_connection(&state.db, |conn| {
        state.limits.get_limits(conn, &request.extension_id)
    })?;

    Ok(ExtensionLimitsResponse::from_limits(
        request.extension_id,
        updated_limits,
        true, // Now custom limits are set
    ))
}

/// Reset limits for an extension to defaults
#[tauri::command]
pub fn reset_extension_limits(
    state: State<'_, AppState>,
    extension_id: String,
) -> Result<ExtensionLimitsResponse, ExtensionError> {
    // Check if extension exists
    let _extension = state
        .extension_manager
        .get_extension(&extension_id)
        .ok_or_else(|| ExtensionError::ValidationError {
            reason: format!("Extension with ID {} not found", extension_id),
        })?;

    // Delete custom limits (soft delete via CRDT)
    with_connection(&state.db, |conn| {
        let tx = conn.transaction()?;

        let hlc_service = state.hlc.lock().map_err(|_| DatabaseError::MutexPoisoned {
            reason: "Failed to lock HLC service".to_string(),
        })?;

        // Soft delete existing record (DELETE is transformed to UPDATE haex_tombstone = 1)
        let sql = "DELETE FROM haex_extension_limits WHERE extension_id = ?";
        let params: Vec<serde_json::Value> = vec![serde_json::json!(extension_id)];

        SqlExecutor::execute_internal(&tx, &hlc_service, sql, &params)?;

        tx.commit()?;
        Ok::<(), DatabaseError>(())
    })?;

    // Return default limits
    let default_limits = with_connection(&state.db, |conn| {
        state.limits.get_limits(conn, &extension_id)
    })?;

    Ok(ExtensionLimitsResponse::from_limits(
        extension_id,
        default_limits,
        false, // Now using defaults
    ))
}
