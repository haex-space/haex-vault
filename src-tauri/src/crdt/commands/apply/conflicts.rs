//! UNIQUE-constraint conflict-entry recording, moved out of
//! `delete_propagation.rs`. Fixes a pre-existing bug found while moving this
//! code: the INSERT targeted the literal `haex_crdt_conflicts` instead of
//! the real schema's `haex_crdt_conflicts_no_sync` (`TABLE_CRDT_CONFLICTS`),
//! so conflict records were silently failing to persist — the caller logged
//! and swallowed the write failure rather than surfacing it.

use rusqlite::params;
use serde_json::Value as JsonValue;
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

use crate::crdt::shared_space_trigger::{is_safe_identifier, ColumnInfo};
use crate::database::core::ValueConverter;
use crate::database::error::DatabaseError;
use crate::table_names::TABLE_CRDT_CONFLICTS;

use super::super::helpers::json_values_to_sql_params;

/// Creates a conflict entry when a UNIQUE constraint is violated.
/// Stores remote data + both PKs (local and remote differ due to UNIQUE conflict).
pub(super) fn create_conflict_entry(
    tx: &rusqlite::Transaction,
    table_name: &str,
    error_msg: &str,
    remote_row_data: &serde_json::Map<String, JsonValue>,
    remote_timestamp: &str,
    schema: &[ColumnInfo],
) -> Result<(), DatabaseError> {
    // Extract the conflicting columns from error message
    // Example: "UNIQUE constraint failed: haex_settings.device_id, haex_settings.key"
    let conflict_key = if let Some(cols) = error_msg.strip_prefix("UNIQUE constraint failed: ") {
        cols.to_string()
    } else {
        error_msg.to_string()
    };

    // Serialize remote row data
    let remote_row_json =
        serde_json::to_string(remote_row_data).map_err(|e| DatabaseError::SerializationError {
            reason: format!("Failed to serialize remote row: {}", e),
        })?;

    // Extract PKs from schema
    let pk_columns: Vec<_> = schema.iter().filter(|col| col.is_pk).collect();

    // Build remote PK JSON
    let remote_pk: serde_json::Map<String, JsonValue> = pk_columns
        .iter()
        .filter_map(|pk_col| {
            remote_row_data
                .get(&pk_col.name)
                .map(|v| (pk_col.name.clone(), v.clone()))
        })
        .collect();
    let remote_pk_json = serde_json::to_string(&remote_pk).unwrap_or_else(|_| "{}".to_string());

    // Find local row PK by querying on the conflicting columns parsed from the error message.
    // conflict_key has the shape "<table>.<col>[, <table>.<col>]*" — strip table prefix from each part.
    let schema_col_names: std::collections::HashSet<&str> =
        schema.iter().map(|c| c.name.as_str()).collect();
    let conflict_cols: Vec<String> = conflict_key
        .split(", ")
        .filter_map(|part| {
            let col = part.rsplit('.').next().unwrap_or("").trim();
            if is_safe_identifier(col) && schema_col_names.contains(col) {
                Some(col.to_string())
            } else {
                None
            }
        })
        .collect();

    let pk_select = pk_columns
        .iter()
        .map(|col| format!("\"{}\"", col.name))
        .collect::<Vec<_>>()
        .join(", ");

    // Build a targeted WHERE clause when we have valid conflict columns AND the remote
    // row carries values for all of them; otherwise fall back to "{}".
    let conflict_values: Option<Vec<JsonValue>> = if conflict_cols.is_empty() {
        None
    } else {
        let vals: Vec<JsonValue> = conflict_cols
            .iter()
            .filter_map(|c| remote_row_data.get(c).cloned())
            .collect();
        if vals.len() == conflict_cols.len() {
            Some(vals)
        } else {
            None
        }
    };

    let local_pk_json = match conflict_values {
        None => "{}".to_string(),
        Some(values) => match json_values_to_sql_params(&values) {
            // Conversion failure degrades to the fallback — the conflict
            // entry must still be recorded below.
            Err(_) => "{}".to_string(),
            Ok(sql_params) => {
                let where_clause = conflict_cols
                    .iter()
                    .enumerate()
                    .map(|(i, c)| format!("\"{}\" = ?{}", c, i + 1))
                    .collect::<Vec<_>>()
                    .join(" AND ");
                let query_sql = format!(
                    "SELECT {} FROM \"{}\" WHERE {} LIMIT 1",
                    pk_select, table_name, where_clause
                );
                let param_refs: Vec<&dyn rusqlite::ToSql> = sql_params
                    .iter()
                    .map(|v| v as &dyn rusqlite::ToSql)
                    .collect();
                tx.query_row(&query_sql, param_refs.as_slice(), |row| {
                    let mut local_pk = serde_json::Map::new();
                    for (i, pk_col) in pk_columns.iter().enumerate() {
                        let val = row.get::<_, rusqlite::types::Value>(i)?;
                        local_pk.insert(
                            pk_col.name.clone(),
                            ValueConverter::rusqlite_value_to_json(&val),
                        );
                    }
                    Ok(serde_json::to_string(&local_pk).unwrap_or_else(|_| "{}".to_string()))
                })
                .unwrap_or_else(|_| "{}".to_string())
            }
        },
    };

    // Generate conflict ID and timestamp
    let conflict_id = Uuid::new_v4().to_string();
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let detected_at = format!("{}", timestamp);

    tx.execute(
        &format!(
            "INSERT INTO {TABLE_CRDT_CONFLICTS} (
            id, table_name, conflict_type, local_row_id, remote_row_id,
            local_row_data, remote_row_data, local_timestamp, remote_timestamp,
            conflict_key, detected_at, resolved
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)"
        ),
        params![
            &conflict_id,
            table_name,
            "UNIQUE",
            &local_pk_json,
            &remote_pk_json,
            "", // UI fetches full row via local_row_id
            &remote_row_json,
            "", // UI fetches local timestamp
            remote_timestamp,
            &conflict_key,
            &detected_at,
            false,
        ],
    )
    .map_err(DatabaseError::from)?;

    eprintln!(
        "[SYNC RUST] Created conflict entry {} for table {}",
        conflict_id, table_name
    );

    Ok(())
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;
    use rusqlite::Connection;

    /// Build a minimal in-memory DB with:
    ///  - `t`: table with UUID pk `id` and UNIQUE(device_id, key)
    ///  - `haex_crdt_conflicts_no_sync`: the REAL production table name
    ///    (`TABLE_CRDT_CONFLICTS`) — regression coverage for the table-name
    ///    bug fixed while moving this code: a prior version targeted the
    ///    literal `haex_crdt_conflicts`, which does not exist in the real
    ///    schema, so every conflict-entry write silently failed and the
    ///    caller logged-and-swallowed the error.
    fn setup_conflict_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(&format!(
            "CREATE TABLE t (
                id TEXT PRIMARY KEY,
                device_id TEXT NOT NULL,
                key TEXT NOT NULL,
                UNIQUE(device_id, key)
             );
             CREATE TABLE {TABLE_CRDT_CONFLICTS} (
                id TEXT PRIMARY KEY,
                table_name TEXT NOT NULL,
                conflict_type TEXT NOT NULL,
                local_row_id TEXT,
                remote_row_id TEXT,
                local_row_data TEXT,
                remote_row_data TEXT,
                local_timestamp TEXT,
                remote_timestamp TEXT,
                conflict_key TEXT,
                detected_at TEXT,
                resolved INTEGER
             );"
        ))
        .unwrap();
        conn
    }

    fn schema_for_t() -> Vec<ColumnInfo> {
        vec![
            ColumnInfo {
                name: "id".to_string(),
                column_type: "TEXT".to_string(),
                is_pk: true,
            },
            ColumnInfo {
                name: "device_id".to_string(),
                column_type: "TEXT".to_string(),
                is_pk: false,
            },
            ColumnInfo {
                name: "key".to_string(),
                column_type: "TEXT".to_string(),
                is_pk: false,
            },
        ]
    }

    /// The concrete regression test for the table-name bug: a real UNIQUE
    /// violation against the REAL table name must actually persist a
    /// conflict row — before the fix this INSERT targeted a table that
    /// doesn't exist in production and the write silently failed.
    #[test]
    fn conflict_entry_actually_persists_against_the_real_table_name() {
        let conn = setup_conflict_db();
        conn.execute(
            "INSERT INTO t (id, device_id, key) VALUES ('local-id-1', 'dev-abc', 'mykey')",
            [],
        )
        .unwrap();

        let mut remote_row: serde_json::Map<String, JsonValue> = serde_json::Map::new();
        remote_row.insert(
            "id".to_string(),
            JsonValue::String("remote-id-999".to_string()),
        );
        remote_row.insert(
            "device_id".to_string(),
            JsonValue::String("dev-abc".to_string()),
        );
        remote_row.insert("key".to_string(), JsonValue::String("mykey".to_string()));

        let tx = conn.unchecked_transaction().unwrap();
        create_conflict_entry(
            &tx,
            "t",
            "UNIQUE constraint failed: t.device_id, t.key",
            &remote_row,
            "1/abc",
            &schema_for_t(),
        )
        .unwrap();
        tx.commit().unwrap();

        let count: i64 = conn
            .query_row(
                &format!("SELECT COUNT(*) FROM {TABLE_CRDT_CONFLICTS}"),
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(
            count, 1,
            "conflict entry must actually land in the real conflicts table"
        );
    }

    #[test]
    fn conflict_entry_records_the_conflicting_row() {
        let conn = setup_conflict_db();
        // Insert a local row with a known id
        conn.execute(
            "INSERT INTO t (id, device_id, key) VALUES ('local-id-1', 'dev-abc', 'mykey')",
            [],
        )
        .unwrap();

        let mut remote_row: serde_json::Map<String, JsonValue> = serde_json::Map::new();
        remote_row.insert(
            "id".to_string(),
            JsonValue::String("remote-id-999".to_string()),
        );
        remote_row.insert(
            "device_id".to_string(),
            JsonValue::String("dev-abc".to_string()),
        );
        remote_row.insert("key".to_string(), JsonValue::String("mykey".to_string()));

        let tx = conn.unchecked_transaction().unwrap();
        create_conflict_entry(
            &tx,
            "t",
            "UNIQUE constraint failed: t.device_id, t.key",
            &remote_row,
            "1/abc",
            &schema_for_t(),
        )
        .unwrap();
        tx.commit().unwrap();

        let local_row_id: String = conn
            .query_row(
                &format!("SELECT local_row_id FROM {TABLE_CRDT_CONFLICTS} LIMIT 1"),
                [],
                |row| row.get(0),
            )
            .unwrap();

        let parsed: serde_json::Value = serde_json::from_str(&local_row_id).unwrap();
        assert_eq!(
            parsed["id"],
            JsonValue::String("local-id-1".to_string()),
            "local_row_id should contain the LOCAL row's id, got: {local_row_id}"
        );
    }

    #[test]
    fn conflict_entry_preserves_integer_local_primary_key() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(&format!(
            "CREATE TABLE t (
                id INTEGER PRIMARY KEY,
                device_id TEXT NOT NULL,
                key TEXT NOT NULL,
                UNIQUE(device_id, key)
             );
             CREATE TABLE {TABLE_CRDT_CONFLICTS} (
                id TEXT PRIMARY KEY,
                table_name TEXT NOT NULL,
                conflict_type TEXT NOT NULL,
                local_row_id TEXT,
                remote_row_id TEXT,
                local_row_data TEXT,
                remote_row_data TEXT,
                local_timestamp TEXT,
                remote_timestamp TEXT,
                conflict_key TEXT,
                detected_at TEXT,
                resolved INTEGER
             );"
        ))
        .unwrap();
        conn.execute(
            "INSERT INTO t (id, device_id, key) VALUES (42, 'dev-abc', 'mykey')",
            [],
        )
        .unwrap();

        let mut remote_row = serde_json::Map::new();
        remote_row.insert("id".to_string(), JsonValue::Number(99.into()));
        remote_row.insert(
            "device_id".to_string(),
            JsonValue::String("dev-abc".to_string()),
        );
        remote_row.insert("key".to_string(), JsonValue::String("mykey".to_string()));
        let schema = vec![
            ColumnInfo {
                name: "id".to_string(),
                column_type: "INTEGER".to_string(),
                is_pk: true,
            },
            ColumnInfo {
                name: "device_id".to_string(),
                column_type: "TEXT".to_string(),
                is_pk: false,
            },
            ColumnInfo {
                name: "key".to_string(),
                column_type: "TEXT".to_string(),
                is_pk: false,
            },
        ];

        let tx = conn.unchecked_transaction().unwrap();
        create_conflict_entry(
            &tx,
            "t",
            "UNIQUE constraint failed: t.device_id, t.key",
            &remote_row,
            "1/abc",
            &schema,
        )
        .unwrap();
        tx.commit().unwrap();

        let local_row_id: String = conn
            .query_row(
                &format!("SELECT local_row_id FROM {TABLE_CRDT_CONFLICTS} LIMIT 1"),
                [],
                |row| row.get(0),
            )
            .unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&local_row_id).unwrap();
        assert_eq!(parsed["id"], JsonValue::Number(42.into()));
    }

    #[test]
    fn conflict_entry_falls_back_on_unparseable_key() {
        let conn = setup_conflict_db();
        let remote_row: serde_json::Map<String, JsonValue> = serde_json::Map::new();

        let tx = conn.unchecked_transaction().unwrap();
        let result = create_conflict_entry(
            &tx,
            "t",
            "some other error without the UNIQUE prefix",
            &remote_row,
            "1/abc",
            &schema_for_t(),
        );
        tx.commit().unwrap();
        assert!(result.is_ok());

        let local_row_id: String = conn
            .query_row(
                &format!("SELECT local_row_id FROM {TABLE_CRDT_CONFLICTS} LIMIT 1"),
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(local_row_id, "{}", "should fall back to empty map");
    }

    #[test]
    fn conflict_entry_falls_back_when_remote_lacks_conflict_values() {
        let conn = setup_conflict_db();
        // Remote row data does NOT contain device_id / key
        let mut remote_row: serde_json::Map<String, JsonValue> = serde_json::Map::new();
        remote_row.insert(
            "id".to_string(),
            JsonValue::String("remote-only-id".to_string()),
        );

        let tx = conn.unchecked_transaction().unwrap();
        let result = create_conflict_entry(
            &tx,
            "t",
            "UNIQUE constraint failed: t.device_id, t.key",
            &remote_row,
            "1/abc",
            &schema_for_t(),
        );
        tx.commit().unwrap();
        assert!(result.is_ok());

        let local_row_id: String = conn
            .query_row(
                &format!("SELECT local_row_id FROM {TABLE_CRDT_CONFLICTS} LIMIT 1"),
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(
            local_row_id, "{}",
            "should fall back to empty map when remote values missing"
        );
    }
}
