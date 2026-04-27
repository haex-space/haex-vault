//! Low-level SQL helpers shared across the inbound-sync authorisation
//! pipeline. Kept separate from the validation/ownership/space-scope
//! modules so that the choke-point logic stays small and the SQL
//! identifier-safety story lives in one place.

use std::collections::HashMap;

use rusqlite::params_from_iter;
use serde_json::Value as JsonValue;

use crate::database::core::with_connection;
use crate::database::DbConnection;
use crate::space_delivery::local::error::DeliveryError;

/// Reads a single column value from an existing CRDT row, identified by
/// its primary-key JSON (`row_pks`). Returns `Ok(None)` when the row does
/// not exist (a brand-new insert).
///
/// Identifiers (`table`, `column`, PK keys) come from the wire and must
/// be filtered before string-interpolating into SQL — only `[a-z0-9_]` is
/// allowed; everything in the production whitelists conforms to that.
pub(super) fn read_existing_column(
    db: &DbConnection,
    table: &str,
    row_pks_json: &str,
    column: &str,
) -> Result<Option<JsonValue>, DeliveryError> {
    let pks: HashMap<String, JsonValue> =
        serde_json::from_str(row_pks_json).map_err(|e| DeliveryError::ProtocolError {
            reason: format!("malformed row_pks JSON {row_pks_json:?}: {e}"),
        })?;
    if pks.is_empty() {
        return Err(DeliveryError::ProtocolError {
            reason: format!("empty row_pks JSON for table {table}"),
        });
    }

    let safe = |s: &str| s.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_');
    if !safe(table) || !safe(column) || !pks.keys().all(|k| safe(k)) {
        return Err(DeliveryError::ProtocolError {
            reason: format!("identifier contains unsafe characters: table={table} column={column}"),
        });
    }

    let where_clause = pks
        .keys()
        .map(|k| format!("{k} = ?"))
        .collect::<Vec<_>>()
        .join(" AND ");
    let sql = format!("SELECT {column} FROM {table} WHERE {where_clause} LIMIT 1");

    let pk_values: Vec<JsonValue> = pks.values().cloned().collect();

    with_connection(db, |conn| {
        let mut stmt = conn.prepare(&sql)?;
        let mut rows = stmt.query(params_from_iter(pk_values.iter().map(|v| match v {
            JsonValue::String(s) => rusqlite::types::Value::Text(s.clone()),
            JsonValue::Number(n) if n.is_i64() => rusqlite::types::Value::Integer(n.as_i64().unwrap()),
            JsonValue::Number(n) => rusqlite::types::Value::Real(n.as_f64().unwrap_or_default()),
            JsonValue::Null => rusqlite::types::Value::Null,
            other => rusqlite::types::Value::Text(other.to_string()),
        })))?;

        if let Some(row) = rows.next()? {
            let raw: rusqlite::types::Value = row.get(0)?;
            let json = match raw {
                rusqlite::types::Value::Null => JsonValue::Null,
                rusqlite::types::Value::Integer(i) => JsonValue::Number(i.into()),
                rusqlite::types::Value::Real(r) => serde_json::Number::from_f64(r)
                    .map(JsonValue::Number)
                    .unwrap_or(JsonValue::Null),
                rusqlite::types::Value::Text(s) => JsonValue::String(s),
                rusqlite::types::Value::Blob(_) => JsonValue::Null,
            };
            Ok(Some(json))
        } else {
            Ok(None)
        }
    })
    .map_err(|e| DeliveryError::Database {
        reason: format!("read_existing_column({table}.{column}): {e}"),
    })
}
