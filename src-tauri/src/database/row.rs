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

/// Get an optional string value from a row by index
pub fn get_optional_string(row: &[JsonValue], idx: usize) -> Option<String> {
    let s = get_string(row, idx);
    if s.is_empty() {
        None
    } else {
        Some(s)
    }
}

/// Get a boolean value from a row by index (SQLite stores booleans as 0/1)
pub fn get_bool(row: &[JsonValue], idx: usize) -> bool {
    row.get(idx)
        .and_then(|v| v.as_i64())
        .map(|v| v != 0)
        .unwrap_or(false)
}

/// Get an i64 value from a row by index
pub fn get_i64(row: &[JsonValue], idx: usize) -> i64 {
    row.get(idx).and_then(|v| v.as_i64()).unwrap_or(0)
}

/// Get a u64 value from a row by index
pub fn get_u64(row: &[JsonValue], idx: usize) -> u64 {
    row.get(idx).and_then(|v| v.as_i64()).unwrap_or(0) as u64
}

/// Get an i32 value from a row by index
pub fn get_i32(row: &[JsonValue], idx: usize) -> i32 {
    row.get(idx).and_then(|v| v.as_i64()).unwrap_or(0) as i32
}

/// Get an f64 value from a row by index
pub fn get_f64(row: &[JsonValue], idx: usize) -> f64 {
    row.get(idx).and_then(|v| v.as_f64()).unwrap_or(0.0)
}
