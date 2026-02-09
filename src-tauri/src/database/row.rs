// src-tauri/src/database/row.rs
//!
//! Helper functions for parsing SQL query result rows
//!
//! Rows returned from select_with_crdt are Vec<JsonValue> where each row is an array.
//! These helpers provide type-safe access to row values by index.
//!

use serde_json::Value as JsonValue;

/// Get a string value from a row by index
pub fn get_string(row: &[JsonValue], idx: usize) -> String {
    row.get(idx)
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string()
}

/// Get a boolean value from a row by index (SQLite stores booleans as 0/1)
pub fn get_bool(row: &[JsonValue], idx: usize) -> bool {
    row.get(idx)
        .and_then(|v| v.as_i64())
        .map(|v| v != 0)
        .unwrap_or(false)
}
