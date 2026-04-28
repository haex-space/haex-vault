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

/// Subset of [`SPACE_SCOPED_CRDT_TABLES`] that every member — including
/// read-only ones — must be able to push, because the rows describe the
/// member's own existence in the group:
///
/// - `haex_space_members`     — own membership row
/// - `haex_space_devices`     — own device registration
/// - `haex_mls_sync_keys`     — own MLS KeyPackages (so others can encrypt to us)
/// - `haex_device_mls_enrollments` — own MLS enrollment artifact
///
/// `haex_peer_shares` is intentionally **not** here: that table holds rows
/// like "I host folder X under endpoint Y" which is genuine user content.
/// A read-only member must not be able to publish shares.
///
/// The leader still re-injects `authored_by_did` from the UCAN audience in
/// `inbound_sync::validate_and_attribute`, so a read-only member cannot
/// forge a row claiming to belong to someone else.
pub const MEMBERSHIP_SYSTEM_TABLES: &[&str] = &[
    "haex_space_devices",
    "haex_space_members",
    "haex_mls_sync_keys",
    "haex_device_mls_enrollments",
];

/// Returns true if `table_name` may be synchronised as part of a shared space.
pub fn is_space_scoped_table(table_name: &str) -> bool {
    SPACE_SCOPED_CRDT_TABLES.contains(&table_name)
}

/// Returns true if a push targeting `table_name` only requires the caller to
/// hold any valid space capability (Read is enough). See the doc on
/// [`MEMBERSHIP_SYSTEM_TABLES`] for the rationale.
pub fn is_membership_system_table(table_name: &str) -> bool {
    MEMBERSHIP_SYSTEM_TABLES.contains(&table_name)
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

/// Test-only helper: unscoped single-table scan. Production code must use
/// `scan_table_for_local_changes_scoped` (or the space-scoped whitelist
/// entry point `scan_space_scoped_tables_for_local_changes`) — an unscoped
/// scan over a table shared by multiple spaces leaks cross-space rows.
#[cfg(test)]
pub fn scan_table_for_local_changes(
    conn: &Connection,
    table_name: &str,
    after_hlc: Option<&str>,
    device_id: &str,
) -> Result<Vec<LocalColumnChange>, DatabaseError> {
    scan_table_for_local_changes_scoped(conn, table_name, after_hlc, device_id, None, None)
}

/// Like `scan_table_for_local_changes` but with two additional predicates:
///
/// * `space_id_filter` — restricts the scan to rows where `space_id = ?`. Used
///   by the space-scoped sync path to prevent leaking rows from other spaces.
/// * `origin_node_filter` — when `Some`, the scanner emits a column change only
///   if its HLC's node-id matches the given `u128`. This stops "ping-pong"
///   re-pushes: rows freshly pulled from a peer carry that peer's HLC node-id
///   and would otherwise be re-scanned and pushed back on the next cycle.
pub fn scan_table_for_local_changes_scoped(
    conn: &Connection,
    table_name: &str,
    after_hlc: Option<&str>,
    device_id: &str,
    space_id_filter: Option<&str>,
    origin_node_filter: Option<u128>,
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
            let passes_hlc = match after_hlc {
                Some(threshold) => hlc_is_newer(hlc_to_use, threshold),
                None => true,
            };

            // If the caller asked for origin filtering, only emit columns we
            // wrote ourselves. Rows applied from inbound sync carry the
            // remote peer's node-id and must not be pushed back.
            let passes_origin = match origin_node_filter {
                Some(our_node) => crate::crdt::hlc::hlc_is_from_node(hlc_to_use, our_node),
                None => true,
            };

            if passes_hlc && passes_origin {
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

/// Scans the whitelist of space-scoped CRDT tables for rows belonging to
/// `space_id`. This is the authoritative scanner for peer-to-peer SyncPull:
/// the caller guarantees that only these tables and only these rows cross
/// the wire, so peers cannot pull data from spaces they are not members of.
///
/// `origin_node` (when `Some`) restricts the result to rows whose HLC was
/// originally written by this node — see the doc on
/// [`scan_table_for_local_changes_scoped`] for the rationale.
///
/// Tables outside [`SPACE_SCOPED_CRDT_TABLES`] are never scanned.
pub fn scan_space_scoped_tables_for_local_changes(
    db: &DbConnection,
    space_id: &str,
    after_hlc: Option<&str>,
    device_id: &str,
    origin_node: Option<u128>,
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
                origin_node,
            )?;
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

/// Like [`scan_space_scoped_tables_for_local_changes`] but restricted to
/// [`MEMBERSHIP_SYSTEM_TABLES`] only. Use this for the push phase when the
/// member holds a `space/read` UCAN: those tables may be pushed with Read
/// capability, whereas `haex_peer_shares` (the only other space-scoped table)
/// requires Write. Including peer_shares in a Read-only push batch causes the
/// leader to reject the entire batch, leaving the push cursor stuck at t=0.
pub fn scan_membership_tables_for_local_changes(
    db: &DbConnection,
    space_id: &str,
    after_hlc: Option<&str>,
    device_id: &str,
    origin_node: Option<u128>,
) -> Result<Vec<LocalColumnChange>, DatabaseError> {
    scan_space_scoped_tables_for_local_changes(db, space_id, after_hlc, device_id, origin_node)
        .map(|changes| {
            changes
                .into_iter()
                .filter(|c| MEMBERSHIP_SYSTEM_TABLES.contains(&c.table_name.as_str()))
                .collect()
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

    #[test]
    fn test_is_space_scoped_table_whitelist() {
        for t in SPACE_SCOPED_CRDT_TABLES {
            assert!(
                is_space_scoped_table(t),
                "whitelist member not recognised: {t}"
            );
        }
        // Private per-vault tables must NOT be space-scoped.
        assert!(!is_space_scoped_table("haex_identities"));
        assert!(!is_space_scoped_table("haex_ucan_tokens"));
        assert!(!is_space_scoped_table("haex_vault_settings"));
        assert!(!is_space_scoped_table("haex_sync_backends"));
        // Extension / unknown tables default to private.
        assert!(!is_space_scoped_table("some_extension_table"));
    }

    #[test]
    fn test_membership_system_tables_are_subset_of_space_scoped() {
        for t in MEMBERSHIP_SYSTEM_TABLES {
            assert!(
                is_space_scoped_table(t),
                "membership-system table not in sync whitelist: {t}"
            );
            assert!(
                is_membership_system_table(t),
                "membership-system table not recognised by helper: {t}"
            );
        }
        // peer_shares must NOT be in the membership-system set: it is
        // user-authored content (a device declaring it hosts a folder),
        // and a read-only member must not be able to push entries here.
        assert!(!is_membership_system_table("haex_peer_shares"));
        // Off-whitelist tables are obviously not membership-system either.
        assert!(!is_membership_system_table("haex_identities"));
        assert!(!is_membership_system_table("some_extension_table"));
    }

    /// Creates a CRDT table that carries a `space_id` discriminator, used to
    /// exercise the scoped-filter path.
    fn setup_scoped_test_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE scoped_items (
                id TEXT PRIMARY KEY,
                space_id TEXT NOT NULL,
                data TEXT,
                haex_hlc TEXT,
                haex_column_hlcs TEXT NOT NULL DEFAULT '{}'
            );",
        )
        .unwrap();
        conn
    }

    fn insert_scoped_row(
        conn: &Connection,
        id: &str,
        space_id: &str,
        data: &str,
        hlc: &str,
    ) {
        let hlcs = format!("{{\"space_id\":\"{hlc}\",\"data\":\"{hlc}\"}}");
        conn.execute(
            "INSERT INTO scoped_items (id, space_id, data, haex_hlc, haex_column_hlcs)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![id, space_id, data, hlc, hlcs],
        )
        .unwrap();
    }

    #[test]
    fn test_scoped_filter_returns_only_matching_space() {
        let conn = setup_scoped_test_db();
        insert_scoped_row(&conn, "r1", "space-A", "hello", "2025-01-01T00:00:00.000Z-0001-d1");
        insert_scoped_row(&conn, "r2", "space-A", "world", "2025-01-01T00:00:00.000Z-0002-d1");
        insert_scoped_row(&conn, "r3", "space-B", "leak", "2025-01-01T00:00:00.000Z-0003-d1");

        let changes = scan_table_for_local_changes_scoped(
            &conn,
            "scoped_items",
            None,
            "device-1",
            Some("space-A"),
            None,
        )
        .unwrap();

        // 2 matching rows × 2 data columns (space_id, data) = 4 changes.
        assert_eq!(changes.len(), 4);

        // No row from space-B may appear — this is the leak gate.
        for change in &changes {
            let pks: serde_json::Map<String, JsonValue> =
                serde_json::from_str(&change.row_pks).unwrap();
            let id = pks.get("id").and_then(|v| v.as_str()).unwrap();
            assert!(id == "r1" || id == "r2", "leaked row from other space: {id}");
        }
    }

    #[test]
    fn test_scoped_filter_on_table_without_space_id_returns_empty() {
        // `test_items` (from setup_test_db) has no space_id column. A scoped
        // filter on such a table must return zero rows rather than the whole
        // table, otherwise vault-private CRDT tables would leak through any
        // peer SyncPull that misconfigures its filter.
        let conn = setup_test_db();
        insert_row(&conn, "r1", "hello", 42, "2025-01-01T00:00:00.000Z-0001-d1");

        let changes = scan_table_for_local_changes_scoped(
            &conn,
            "test_items",
            None,
            "device-1",
            Some("any-space"),
            None,
        )
        .unwrap();

        assert!(changes.is_empty());
    }
}
