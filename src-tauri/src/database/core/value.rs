// src-tauri/src/database/core/value.rs

use crate::database::error::DatabaseError;
use base64::{engine::general_purpose::STANDARD, Engine as _};
use rusqlite::types::{Value as SqlValue, ValueRef};
use serde_json::Value as JsonValue;

pub struct ValueConverter;

impl ValueConverter {
    pub fn json_to_rusqlite_value(json_val: &JsonValue) -> Result<SqlValue, DatabaseError> {
        match json_val {
            JsonValue::Null => Ok(SqlValue::Null),
            JsonValue::Bool(b) => {
                // SQLite hat keinen Bool-Typ; verwende Integer 0/1
                Ok(SqlValue::Integer(if *b { 1 } else { 0 }))
            }
            JsonValue::Number(n) => {
                if let Some(i) = n.as_i64() {
                    Ok(SqlValue::Integer(i))
                } else if let Some(f) = n.as_f64() {
                    Ok(SqlValue::Real(f))
                } else {
                    // Fallback: als Text
                    Ok(SqlValue::Text(n.to_string()))
                }
            }
            JsonValue::String(s) => Ok(SqlValue::Text(s.clone())),
            JsonValue::Array(_) | JsonValue::Object(_) => {
                // Arrays/Objects als JSON-Text speichern
                serde_json::to_string(json_val)
                    .map(SqlValue::Text)
                    .map_err(|e| DatabaseError::SerializationError {
                        reason: format!("Failed to serialize JSON param: {e}"),
                    })
            }
        }
    }

    pub fn convert_params(params: &[JsonValue]) -> Result<Vec<SqlValue>, DatabaseError> {
        params.iter().map(Self::json_to_rusqlite_value).collect()
    }

    /// Converts an owned SqlValue to JSON by delegating to convert_value_ref_to_json.
    pub fn rusqlite_value_to_json(sql_value: &SqlValue) -> JsonValue {
        let value_ref = match sql_value {
            SqlValue::Null => ValueRef::Null,
            SqlValue::Integer(n) => ValueRef::Integer(*n),
            SqlValue::Real(f) => ValueRef::Real(*f),
            SqlValue::Text(s) => ValueRef::Text(s.as_bytes()),
            SqlValue::Blob(b) => ValueRef::Blob(b),
        };
        convert_value_ref_to_json(value_ref).unwrap_or(JsonValue::Null)
    }
}

/// Konvertiert rusqlite ValueRef zu JSON
pub fn convert_value_ref_to_json(value_ref: ValueRef) -> Result<JsonValue, DatabaseError> {
    let json_val = match value_ref {
        ValueRef::Null => JsonValue::Null,
        ValueRef::Integer(i) => JsonValue::Number(i.into()),
        ValueRef::Real(f) => JsonValue::Number(
            serde_json::Number::from_f64(f).unwrap_or_else(|| serde_json::Number::from(0)),
        ),
        ValueRef::Text(t) => {
            let s = String::from_utf8_lossy(t).to_string();
            JsonValue::String(s)
        }
        ValueRef::Blob(b) => {
            // BLOBs als Base64-String zurückgeben
            JsonValue::String(STANDARD.encode(b))
        }
    };
    Ok(json_val)
}
