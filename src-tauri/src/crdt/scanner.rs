//! Table scanner for outbound CRDT changes (unencrypted, for local sync).
//!
//! This is the Rust equivalent of `src/stores/sync/tableScanner.ts` (`scanTableForChangesAsync`).
//! It produces unencrypted column-level changes for local space sync over QUIC,
//! which provides transport encryption.

use crate::crdt::hlc::hlc_is_newer;
use crate::crdt::trigger::{get_table_schema, ColumnInfo, COLUMN_HLCS_COLUMN, HLC_TIMESTAMP_COLUMN};
use crate::database::core::{convert_value_ref_to_json, with_connection};
use crate::database::error::DatabaseError;
use crate::database::DbConnection;
use crate::table_names::TABLE_CRDT_DIRTY_TABLES;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::collections::HashMap;

/// Sync metadata columns to exclude from scanning (not user data).
const EXCLUDED_SYNC_COLUMNS: &[&str] = &[
    "last_push_hlc_timestamp",
    "last_pull_server_timestamp",
    "updated_at",
    "created_at",
];

/// Whitelist of CRDT tables that may be synchronised between peers of a
/// shared space. Everything else (identities, sync backends, vault settings,
/// pending invites, UCAN chains, extension tables …) is considered vault-
/// private and must **never** be shipped across a space-delivery stream.
///
/// The UCAN delegation chain itself travels inside each delegated token
/// (`proofs` field), so `haex_ucan_tokens` does not need to be synced either.
pub const SPACE_SCOPED_CRDT_TABLES: &[&str] = &[
    "haex_space_devices",
    "haex_space_members",
    "haex_peer_shares",
    "haex_mls_sync_keys",
    "haex_device_mls_enrollments",
];

/// Returns true if `table_name` may be synchronised as part of a shared space.
pub fn is_space_scoped_table(table_name: &str) -> bool {
    SPACE_SCOPED_CRDT_TABLES.contains(&table_name)
}

/// A column-level change ready for local transmission (no encryption).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalColumnChange {
    pub table_name: String,
    /// JSON string of PK values, e.g. `{"id":"abc-123"}`
    pub row_pks: String,
    pub column_name: String,
    pub hlc_timestamp: String,
    /// Plain value (not encrypted)
    pub value: JsonValue,
    pub device_id: String,
}

/// Splits a table schema into PK columns and syncable data columns.
///
/// Data columns exclude:
/// - PK columns
/// - CRDT metadata: `haex_hlc`, `haex_column_hlcs`
/// - Sync metadata: `last_push_hlc_timestamp`, `last_pull_server_timestamp`, `updated_at`, `created_at`
fn partition_columns(schema: &[ColumnInfo]) -> (Vec<&ColumnInfo>, Vec<&ColumnInfo>) {
    let pk_columns: Vec<&ColumnInfo> = schema.iter().filter(|c| c.is_pk).collect();
    let data_columns: Vec<&ColumnInfo> = schema
        .iter()
        .filter(|c| {
            !c.is_pk
                && c.name != HLC_TIMESTAMP_COLUMN
                && c.name != COLUMN_HLCS_COLUMN
                && !EXCLUDED_SYNC_COLUMNS.contains(&c.name.as_str())
        })
        .collect();
    (pk_columns, data_columns)
}

/// Scans a single table for column-level local changes newer than `after_hlc`.
///
/// For each row with `haex_hlc > after_hlc` (or all rows if `after_hlc` is `None`),
/// every data column whose individual HLC exceeds `after_hlc` is emitted as a
/// [`LocalColumnChange`].
pub fn scan_table_for_local_changes(
    conn: &Connection,
    table_name: &str,
    after_hlc: Option<&str>,
    device_id: &str,
) -> Result<Vec<LocalColumnChange>, DatabaseError> {
    scan_table_for_local_changes_scoped(conn, table_name, after_hlc, device_id, None)
}

/// Like `scan_table_for_local_changes` but with an additional `space_id`
/// predicate. The caller passes the target space id; the scanner limits the
/// result to rows where `space_id = ?`. Used by the space-scoped sync path
/// to prevent leaking rows from other spaces.
pub fn scan_table_for_local_changes_scoped(
    conn: &Connection,
    table_name: &str,
    after_hlc: Option<&str>,
    device_id: &str,
    space_id_filter: Option<&str>,
) -> Result<Vec<LocalColumnChange>, DatabaseError> {
    let schema = get_table_schema(conn, table_name).map_err(DatabaseError::from)?;

    if schema.is_empty() {
        return Ok(Vec::new());
    }

    let (pk_columns, data_columns) = partition_columns(&schema);

    if pk_columns.is_empty() {
        return Err(DatabaseError::ExecutionError {
            sql: format!("PRAGMA table_info(\"{}\")", table_name),
            reason: format!("Table '{}' has no primary key", table_name),
            table: Some(table_name.to_string()),
        });
    }

    // If the caller asked to filter by space_id but the table has no
    // `space_id` column, treat that as "no matching rows" rather than
    // silently returning the whole table.
    let has_space_id_column = schema.iter().any(|c| c.name == "space_id");
    if space_id_filter.is_some() && !has_space_id_column {
        return Ok(Vec::new());
    }

    // Build column list: PKs + data columns + CRDT metadata
    let mut select_columns: Vec<&str> = Vec::new();
    for col in &pk_columns {
        select_columns.push(&col.name);
    }
    for col in &data_columns {
        select_columns.push(&col.name);
    }
    select_columns.push(HLC_TIMESTAMP_COLUMN);
    select_columns.push(COLUMN_HLCS_COLUMN);

    let column_list: String = select_columns
        .iter()
        .map(|c| format!("\"{}\"", c))
        .collect::<Vec<_>>()
        .join(", ");

    let mut where_clauses: Vec<String> = Vec::new();
    let mut params: Vec<String> = Vec::new();

    if let Some(hlc) = after_hlc {
        where_clauses.push(format!(
            "\"{}\" > ?{}",
            HLC_TIMESTAMP_COLUMN,
            where_clauses.len() + 1
        ));
        params.push(hlc.to_string());
    }
    if let Some(space_id) = space_id_filter {
        where_clauses.push(format!("\"space_id\" = ?{}", where_clauses.len() + 1));
        params.push(space_id.to_string());
    }

    let query = if where_clauses.is_empty() {
        format!("SELECT {} FROM \"{}\"", column_list, table_name)
    } else {
        format!(
            "SELECT {} FROM \"{}\" WHERE {}",
            column_list,
            table_name,
            where_clauses.join(" AND ")
        )
    };

    let mut stmt = conn.prepare(&query).map_err(DatabaseError::from)?;

    let param_refs: Vec<&dyn rusqlite::ToSql> =
        params.iter().map(|s| s as &dyn rusqlite::ToSql).collect();

    let mut rows = stmt.query(param_refs.as_slice()).map_err(DatabaseError::from)?;

    let mut changes: Vec<LocalColumnChange> = Vec::new();

    while let Some(row) = rows.next().map_err(DatabaseError::from)? {
        // Read all column values into a name -> JsonValue map
        let mut row_map: HashMap<&str, JsonValue> = HashMap::new();
        for (i, col_name) in select_columns.iter().enumerate() {
            let value_ref = row.get_ref(i).map_err(DatabaseError::from)?;
            let json_val = convert_value_ref_to_json(value_ref)?;
            row_map.insert(col_name, json_val);
        }

        // Parse haex_column_hlcs JSON
        let column_hlcs: HashMap<String, String> = match row_map.get(COLUMN_HLCS_COLUMN) {
            Some(JsonValue::String(s)) => serde_json::from_str(s).unwrap_or_default(),
            _ => HashMap::new(),
        };

        // Build PK JSON string
        let pk_map: serde_json::Map<String, JsonValue> = pk_columns
            .iter()
            .filter_map(|pk| {
                row_map
                    .get(pk.name.as_str())
                    .map(|v| (pk.name.clone(), v.clone()))
            })
            .collect();
        let pk_json = serde_json::to_string(&pk_map).unwrap_or_else(|_| "{}".to_string());

        // Row-level HLC as fallback
        let row_hlc = match row_map.get(HLC_TIMESTAMP_COLUMN) {
            Some(JsonValue::String(s)) => Some(s.as_str()),
            _ => None,
        };

        // For each data column, emit a change if its HLC > after_hlc
        for col in &data_columns {
            let col_hlc = column_hlcs.get(&col.name).map(|s| s.as_str());
            let hlc_to_use = col_hlc.or(row_hlc);

            let hlc_to_use = match hlc_to_use {
                Some(h) => h,
                None => continue, // no HLC at all — skip
            };

            // Check if this column's HLC is newer than after_hlc
            let should_include = match after_hlc {
                Some(threshold) => hlc_is_newer(hlc_to_use, threshold),
                None => true,
            };

            if should_include {
                let value = row_map
                    .get(col.name.as_str())
                    .cloned()
                    .unwrap_or(JsonValue::Null);

                changes.push(LocalColumnChange {
                    table_name: table_name.to_string(),
                    row_pks: pk_json.clone(),
                    column_name: col.name.clone(),
                    hlc_timestamp: hlc_to_use.to_string(),
                    value,
                    device_id: device_id.to_string(),
                });
            }
        }
    }

    Ok(changes)
}

/// Scans only the dirty tables for column-level local changes.
///
/// Queries `haex_crdt_dirty_tables_no_sync` for table names, then delegates
/// to [`scan_table_for_local_changes`] for each.
pub fn scan_all_dirty_tables_for_local_changes(
    db: &DbConnection,
    after_hlc: Option<&str>,
    device_id: &str,
) -> Result<Vec<LocalColumnChange>, DatabaseError> {
    with_connection(db, |conn| {
        let mut stmt = conn
            .prepare(&format!(
                "SELECT table_name FROM {}",
                TABLE_CRDT_DIRTY_TABLES
            ))
            .map_err(DatabaseError::from)?;

        let table_names: Vec<String> = stmt
            .query_map([], |row| row.get(0))
            .map_err(DatabaseError::from)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(DatabaseError::from)?;

        let mut all_changes: Vec<LocalColumnChange> = Vec::new();
        for table_name in &table_names {
            let changes = scan_table_for_local_changes(conn, table_name, after_hlc, device_id)?;
            all_changes.extend(changes);
        }

        // Global sort by transaction-HLC ascending so downstream chunking can
        // respect HLC-group boundaries without further grouping logic.
        all_changes.sort_by(|a, b| {
            crate::crdt::hlc::compare_hlc_strings(&a.hlc_timestamp, &b.hlc_timestamp)
        });

        Ok(all_changes)
    })
}

/// Scans the whitelist of space-scoped CRDT tables for rows belonging to
/// `space_id`. This is the authoritative scanner for peer-to-peer SyncPull:
/// the caller guarantees that only these tables and only these rows cross
/// the wire, so peers cannot pull data from spaces they are not members of.
///
/// Tables outside [`SPACE_SCOPED_CRDT_TABLES`] are never scanned.
pub fn scan_space_scoped_tables_for_local_changes(
    db: &DbConnection,
    space_id: &str,
    after_hlc: Option<&str>,
    device_id: &str,
) -> Result<Vec<LocalColumnChange>, DatabaseError> {
    with_connection(db, |conn| {
        let mut all_changes: Vec<LocalColumnChange> = Vec::new();
        for table_name in SPACE_SCOPED_CRDT_TABLES {
            let changes = scan_table_for_local_changes_scoped(
                conn,
                table_name,
                after_hlc,
                device_id,
                Some(space_id),
            )?;
            all_changes.extend(changes);
        }

        // Global sort by transaction-HLC ascending — see rationale in
        // `scan_all_dirty_tables_for_local_changes`.
        all_changes.sort_by(|a, b| {
            crate::crdt::hlc::compare_hlc_strings(&a.hlc_timestamp, &b.hlc_timestamp)
        });

        Ok(all_changes)
    })
}

// `scan_all_crdt_tables_for_local_changes` used to scan every CRDT table
// without a space filter. That function powered the old peer SyncPull and
// was the root of a cross-space data leak — a peer asking for space X
// would receive rows from every space the leader was in. It has been
// removed. Use `scan_space_scoped_tables_for_local_changes` for peer sync.

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    /// Helper: create an in-memory DB with a CRDT-enabled table and return the connection.
    fn setup_test_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE test_items (
                id TEXT PRIMARY KEY,
                name TEXT,
                value INTEGER,
                haex_hlc TEXT,
                haex_column_hlcs TEXT NOT NULL DEFAULT '{}'
            );",
        )
        .unwrap();
        conn
    }

    fn insert_row(conn: &Connection, id: &str, name: &str, value: i64, hlc: &str) {
        let hlcs = format!("{{\"name\":\"{hlc}\",\"value\":\"{hlc}\"}}");
        conn.execute(
            "INSERT INTO test_items (id, name, value, haex_hlc, haex_column_hlcs)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![id, name, value, hlc, hlcs],
        )
        .unwrap();
    }

    #[test]
    fn test_scan_empty_table_returns_no_changes() {
        let conn = setup_test_db();
        let changes =
            scan_table_for_local_changes(&conn, "test_items", None, "device-1").unwrap();
        assert!(changes.is_empty());
    }

    #[test]
    fn test_scan_full_returns_all_columns() {
        let conn = setup_test_db();
        insert_row(&conn, "row-1", "hello", 42, "2025-01-01T00:00:00.000Z-0001-device1");

        let changes =
            scan_table_for_local_changes(&conn, "test_items", None, "device-1").unwrap();

        // 2 data columns: name, value
        assert_eq!(changes.len(), 2);

        let names: Vec<&str> = changes.iter().map(|c| c.column_name.as_str()).collect();
        assert!(names.contains(&"name"));
        assert!(names.contains(&"value"));

        // Verify PK JSON
        for change in &changes {
            assert_eq!(change.table_name, "test_items");
            assert_eq!(change.device_id, "device-1");
            let pks: serde_json::Map<String, JsonValue> =
                serde_json::from_str(&change.row_pks).unwrap();
            assert_eq!(pks.get("id").unwrap(), "row-1");
        }
    }

    #[test]
    fn test_scan_with_after_hlc_filters_old_rows() {
        let conn = setup_test_db();
        insert_row(&conn, "old", "old", 1, "1000000000000000000/aabbccdd");
        insert_row(&conn, "new", "new", 2, "3000000000000000000/aabbccdd");

        let changes = scan_table_for_local_changes(
            &conn,
            "test_items",
            Some("2000000000000000000/aabbccdd"),
            "device-1",
        )
        .unwrap();

        // Only the "new" row should be present (2 data columns: name, value)
        assert_eq!(changes.len(), 2);
        for change in &changes {
            let pks: serde_json::Map<String, JsonValue> =
                serde_json::from_str(&change.row_pks).unwrap();
            assert_eq!(pks.get("id").unwrap(), "new");
        }
    }

    #[test]
    fn test_scan_excludes_metadata_columns() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE with_meta (
                id TEXT PRIMARY KEY,
                data TEXT,
                last_push_hlc_timestamp TEXT,
                last_pull_server_timestamp TEXT,
                updated_at TEXT,
                created_at TEXT,
                haex_hlc TEXT,
                haex_column_hlcs TEXT NOT NULL DEFAULT '{}'
            );",
        )
        .unwrap();

        conn.execute(
            "INSERT INTO with_meta (id, data, haex_hlc, haex_column_hlcs)
             VALUES ('r1', 'test', '2025-01-01T00:00:00.000Z-0001-d1',
                     '{\"data\":\"2025-01-01T00:00:00.000Z-0001-d1\"}')",
            [],
        )
        .unwrap();

        let changes =
            scan_table_for_local_changes(&conn, "with_meta", None, "device-1").unwrap();

        let col_names: Vec<&str> = changes.iter().map(|c| c.column_name.as_str()).collect();
        // Only "data" should remain; all metadata/CRDT columns filtered out
        assert!(col_names.contains(&"data"));
        assert!(!col_names.contains(&"last_push_hlc_timestamp"));
        assert!(!col_names.contains(&"last_pull_server_timestamp"));
        assert!(!col_names.contains(&"updated_at"));
        assert!(!col_names.contains(&"created_at"));
        assert!(!col_names.contains(&"haex_hlc"));
        assert!(!col_names.contains(&"haex_column_hlcs"));
    }

    #[test]
    fn test_scan_uses_row_hlc_as_fallback() {
        let conn = setup_test_db();
        // Insert a row where haex_column_hlcs is empty — row-level HLC should be used
        conn.execute(
            "INSERT INTO test_items (id, name, value, haex_hlc, haex_column_hlcs)
             VALUES ('r1', 'test', 10, '2025-01-01T00:00:00.000Z-0001-d1', '{}')",
            [],
        )
        .unwrap();

        let changes =
            scan_table_for_local_changes(&conn, "test_items", None, "device-1").unwrap();

        // Both data columns should be emitted using the row-level HLC
        assert_eq!(changes.len(), 2);
        for change in &changes {
            assert_eq!(
                change.hlc_timestamp,
                "2025-01-01T00:00:00.000Z-0001-d1"
            );
        }
    }

    #[test]
    fn test_column_level_hlc_filtering() {
        let conn = setup_test_db();
        // Insert a row where 'name' has a newer HLC but 'value' has an older one
        let hlcs = r#"{"name":"3000000000000000000/aabbccdd","value":"1000000000000000000/aabbccdd"}"#;
        conn.execute(
            "INSERT INTO test_items (id, name, value, haex_hlc, haex_column_hlcs)
             VALUES ('r1', 'updated', 10, '3000000000000000000/aabbccdd', ?1)",
            [hlcs],
        )
        .unwrap();

        let changes = scan_table_for_local_changes(
            &conn,
            "test_items",
            Some("2000000000000000000/aabbccdd"),
            "device-1",
        )
        .unwrap();

        // Only 'name' should pass the per-column HLC filter
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].column_name, "name");
    }

    #[test]
    fn test_scan_composite_pk() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE composite_pk (
                group_id TEXT NOT NULL,
                item_id TEXT NOT NULL,
                data TEXT,
                haex_hlc TEXT,
                haex_column_hlcs TEXT NOT NULL DEFAULT '{}',
                PRIMARY KEY (group_id, item_id)
            );",
        )
        .unwrap();

        let hlcs = r#"{"data":"2025-01-01T00:00:00.000Z-0001-d1"}"#;
        conn.execute(
            "INSERT INTO composite_pk (group_id, item_id, data, haex_hlc, haex_column_hlcs)
             VALUES ('g1', 'i1', 'hello', '2025-01-01T00:00:00.000Z-0001-d1', ?1)",
            [hlcs],
        )
        .unwrap();

        let changes =
            scan_table_for_local_changes(&conn, "composite_pk", None, "device-1").unwrap();

        assert_eq!(changes.len(), 1); // data only

        let pks: serde_json::Map<String, JsonValue> =
            serde_json::from_str(&changes[0].row_pks).unwrap();
        assert_eq!(pks.get("group_id").unwrap(), "g1");
        assert_eq!(pks.get("item_id").unwrap(), "i1");
    }

    #[test]
    fn test_scan_null_value() {
        let conn = setup_test_db();
        let hlcs = r#"{"name":"2025-01-01T00:00:00.000Z-0001-d1","value":"2025-01-01T00:00:00.000Z-0001-d1"}"#;
        conn.execute(
            "INSERT INTO test_items (id, name, value, haex_hlc, haex_column_hlcs)
             VALUES ('r1', NULL, NULL, '2025-01-01T00:00:00.000Z-0001-d1', ?1)",
            [hlcs],
        )
        .unwrap();

        let changes =
            scan_table_for_local_changes(&conn, "test_items", None, "device-1").unwrap();

        // NULL values should still produce changes for both data columns
        assert_eq!(changes.len(), 2);
        let name_change = changes.iter().find(|c| c.column_name == "name").unwrap();
        assert_eq!(name_change.value, JsonValue::Null);
    }

    #[test]
    fn test_scan_nonexistent_table_returns_empty() {
        let conn = Connection::open_in_memory().unwrap();
        let changes =
            scan_table_for_local_changes(&conn, "nonexistent", None, "device-1").unwrap();
        assert!(changes.is_empty());
    }
}
