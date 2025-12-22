// src-tauri/src/extension/database/types.rs
//!
//! Types for extension database operations
//!

use serde::Serialize;
use serde_json::Value as JsonValue;

/// Result of applying extension migrations
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MigrationResult {
    pub applied_count: usize,
    pub already_applied_count: usize,
    pub applied_migrations: Vec<String>,
}

/// Result of a database query or execute operation
/// This format is used for both iframe (postMessage) and WebView (Tauri invoke) modes
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DatabaseQueryResult {
    /// The result rows (array of arrays for query, may be empty for execute)
    pub rows: Vec<Vec<JsonValue>>,
    /// Number of rows affected (for execute operations)
    pub rows_affected: usize,
    /// Last inserted row ID (if applicable)
    pub last_insert_id: Option<i64>,
}
