//! Storage helper for the `haex_column_sigs` JSON meta column.
//!
//! `haex_column_sigs` on each space-scoped table stores per-column,
//! per-space Ed25519 signatures. The JSON shape is:
//!
//! ```json
//! {
//!   "<column_name>": {
//!     "<space_id>": {
//!       "authorDid": "did:key:z6M…",
//!       "sig": "<base64-encoded 64-byte Ed25519 signature>",
//!       "storageClass": "text"
//!     }
//!   }
//! }
//! ```
//!
//! [`upsert_column_sigs`] merges a new `(column_name, space_id)` entry into
//! the JSON blob on the target row, preserving all other columns and other
//! spaces on the same column.
//!
//! ## Why the inner keys are camelCase
//!
//! The stored record is forwarded **verbatim** onto the sync wire: the TS
//! scanner (`src/stores/sync/tableScanner.ts`) parses this JSON, picks
//! `[column][space]` and attaches the object as `ColumnChange.sig` without
//! rewriting keys. The receiving end deserialises it into
//! [`crate::crdt::commands::apply::ColumnSig`], which is
//! `#[serde(rename_all = "camelCase")]`. A snake_case `author_did` here
//! therefore arrives as an unknown field and the DID reads back as
//! `undefined` in TS / fails to deserialise in Rust — see
//! [`AUTHOR_DID_KEY`]. Keep this key and the wire struct in lockstep.

use crate::crdt::column_sig::value_bytes::StorageClass;
use crate::crdt::trigger::{get_table_schema, is_safe_identifier};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use rusqlite::{params_from_iter, types::ToSqlOutput, Connection, Error as RusqliteError, ToSql};
use serde_json::{Map, Value};
use tracing::warn;

/// JSON key holding the signer DID inside a stored sig record.
///
/// Deliberately camelCase: the stored record travels to the wire unchanged
/// (see module docs). `SIG_KEY` is spelled the same in both worlds, but is
/// named here so both keys live next to each other.
pub const AUTHOR_DID_KEY: &str = "authorDid";
/// JSON key holding the base64 signature inside a stored sig record.
pub const SIG_KEY: &str = "sig";
/// JSON key holding the SQLite storage class covered by the signature.
pub const STORAGE_CLASS_KEY: &str = "storageClass";

/// A single per-column, per-space signature record.
///
/// Also used by write-time signing (`super::write::sign_column_for_spaces`).
#[derive(Debug, Clone)]
pub struct SigRecord {
    pub author_did: String,
    pub sig: [u8; 64],
    pub storage_class: StorageClass,
}

/// Merges a signature into the `haex_column_sigs` JSON column of the row
/// identified by `row_pks_json` on `table_name`.
///
/// - Preserves signatures for other columns and other `space_id`s on the
///   same row.
/// - Replaces the entry for `(column_name, space_id)` if one already exists.
/// - `sig` bytes are Base64-STANDARD-encoded.
///
/// Returns [`RusqliteError::InvalidParameterName`] for unsafe `table_name`
/// (SQL-injection guard delegated to [`is_safe_identifier`]) or when
/// `row_pks_json` cannot be parsed / lacks a required PK column, or when the
/// row does not exist.
pub fn upsert_column_sigs(
    conn: &Connection,
    table_name: &str,
    row_pks_json: &str,
    column_name: &str,
    space_id: &str,
    sig: &SigRecord,
) -> Result<(), RusqliteError> {
    if !is_safe_identifier(table_name) {
        return Err(RusqliteError::InvalidParameterName(format!(
            "Invalid or unsafe table name provided: {table_name}"
        )));
    }

    // Resolve PK columns via the same helper that other CRDT code uses so we
    // stay consistent with WITHOUT ROWID + composite-PK tables.
    let schema = get_table_schema(conn, table_name)?;
    let pk_columns: Vec<String> = schema
        .into_iter()
        .filter(|c| c.is_pk)
        .map(|c| c.name)
        .collect();
    if pk_columns.is_empty() {
        return Err(RusqliteError::InvalidParameterName(format!(
            "Table has no primary key columns: {table_name}"
        )));
    }

    let pks_map: Map<String, Value> = match serde_json::from_str::<Value>(row_pks_json) {
        Ok(Value::Object(m)) => m,
        _ => {
            return Err(RusqliteError::InvalidParameterName(format!(
                "row_pks_json is not a JSON object: {row_pks_json}"
            )));
        }
    };

    // Ordered PK-values matching pk_columns order, for the WHERE clause.
    let mut pk_values: Vec<Value> = Vec::with_capacity(pk_columns.len());
    for col in &pk_columns {
        match pks_map.get(col) {
            Some(v) => pk_values.push(v.clone()),
            None => {
                return Err(RusqliteError::InvalidParameterName(format!(
                    "row_pks_json is missing primary-key column '{col}'"
                )));
            }
        }
    }

    let where_clause: String = pk_columns
        .iter()
        .map(|c| format!("\"{}\" = ?", c))
        .collect::<Vec<_>>()
        .join(" AND ");

    // Load current JSON, defaulting to '{}' if the row somehow holds NULL.
    let select_sql = format!(
        "SELECT haex_column_sigs FROM \"{}\" WHERE {}",
        table_name, where_clause
    );
    let current_raw: String = {
        let mut stmt = conn.prepare(&select_sql)?;
        let params: Vec<JsonParam> = pk_values.iter().cloned().map(JsonParam).collect();
        stmt.query_row(params_from_iter(params.iter()), |row| {
            let v: Option<String> = row.get(0)?;
            Ok(v.unwrap_or_else(|| "{}".to_string()))
        })?
    };

    let mut root: Map<String, Value> = match serde_json::from_str::<Value>(&current_raw) {
        Ok(Value::Object(m)) => m,
        _ => {
            // Silent reset would drop ALL other (column, space) sigs on this row.
            // In prod this JSON is only ever produced by this module, so an
            // unexpected shape is a bug elsewhere. Log loudly (F#1).
            warn!(
                target: "column_sig",
                table = table_name,
                column = column_name,
                "haex_column_sigs root is not a JSON object — resetting to empty map"
            );
            Map::new()
        }
    };

    // Merge: root[column_name][space_id] = { authorDid, sig }
    let column_entry = root
        .entry(column_name.to_string())
        .or_insert_with(|| Value::Object(Map::new()));
    let column_map = match column_entry {
        Value::Object(m) => m,
        other => {
            // Same story: dropping non-Object entry drops ALL other spaces'
            // sigs on this column. Log loudly and reset (F#1).
            warn!(
                target: "column_sig",
                table = table_name,
                column = column_name,
                "haex_column_sigs[column] is not a JSON object — refusing to clobber; resetting to empty map"
            );
            *other = Value::Object(Map::new());
            match other {
                Value::Object(m) => m,
                _ => unreachable!(),
            }
        }
    };
    let mut entry = Map::with_capacity(2);
    entry.insert(
        AUTHOR_DID_KEY.to_string(),
        Value::String(sig.author_did.clone()),
    );
    entry.insert(SIG_KEY.to_string(), Value::String(BASE64.encode(sig.sig)));
    entry.insert(
        STORAGE_CLASS_KEY.to_string(),
        serde_json::to_value(sig.storage_class).map_err(|e| {
            RusqliteError::ToSqlConversionFailure(Box::new(std::io::Error::other(e.to_string())))
        })?,
    );
    column_map.insert(space_id.to_string(), Value::Object(entry));

    let new_raw = serde_json::to_string(&Value::Object(root)).map_err(|e| {
        RusqliteError::ToSqlConversionFailure(Box::new(std::io::Error::other(e.to_string())))
    })?;

    let update_sql = format!(
        "UPDATE \"{}\" SET haex_column_sigs = ? WHERE {}",
        table_name, where_clause
    );
    let mut stmt = conn.prepare(&update_sql)?;
    let mut params: Vec<Box<dyn ToSql>> = Vec::with_capacity(pk_values.len() + 1);
    params.push(Box::new(new_raw));
    for v in pk_values {
        params.push(Box::new(JsonParam(v)));
    }
    let params_refs: Vec<&dyn ToSql> = params.iter().map(|b| b.as_ref()).collect();
    let n = stmt.execute(&*params_refs)?;
    if n == 0 {
        return Err(RusqliteError::QueryReturnedNoRows);
    }
    Ok(())
}

/// Thin ToSql wrapper that projects `serde_json::Value` primitives onto
/// SQLite storage classes. Used only for PK-column binding.
struct JsonParam(Value);

impl ToSql for JsonParam {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
        Ok(match &self.0 {
            Value::Null => ToSqlOutput::Owned(rusqlite::types::Value::Null),
            Value::Bool(b) => ToSqlOutput::Owned(rusqlite::types::Value::Integer(i64::from(*b))),
            Value::Number(n) => {
                if let Some(i) = n.as_i64() {
                    ToSqlOutput::Owned(rusqlite::types::Value::Integer(i))
                } else if let Some(f) = n.as_f64() {
                    ToSqlOutput::Owned(rusqlite::types::Value::Real(f))
                } else {
                    ToSqlOutput::Owned(rusqlite::types::Value::Null)
                }
            }
            Value::String(s) => ToSqlOutput::Owned(rusqlite::types::Value::Text(s.clone())),
            Value::Array(_) | Value::Object(_) => {
                ToSqlOutput::Owned(rusqlite::types::Value::Text(self.0.to_string()))
            }
        })
    }
}
