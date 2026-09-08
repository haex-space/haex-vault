//! Per-column emission semantics: what the scanner emits for one table, and
//! how it resolves the per-column vs row-level HLC.

use super::fixtures::{insert_row, scan_table_for_local_changes, setup_test_db};
use rusqlite::Connection;
use serde_json::Value as JsonValue;

#[test]
fn test_scan_empty_table_returns_no_changes() {
    let conn = setup_test_db();
    let changes = scan_table_for_local_changes(&conn, "test_items", None, "device-1").unwrap();
    assert!(changes.is_empty());
}

#[test]
fn test_scan_full_returns_all_columns() {
    let conn = setup_test_db();
    insert_row(
        &conn,
        "row-1",
        "hello",
        42,
        "2025-01-01T00:00:00.000Z-0001-device1",
    );

    let changes = scan_table_for_local_changes(&conn, "test_items", None, "device-1").unwrap();

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
                last_push_hlc_timestamp_no_sync TEXT,
                last_pull_server_timestamp_no_sync TEXT,
                updated_at_no_sync TEXT,
                created_at_no_sync TEXT,
                haex_hlc_no_trigger TEXT,
                haex_column_hlcs_no_trigger TEXT NOT NULL DEFAULT '{}'
            );",
    )
    .unwrap();

    conn.execute(
        "INSERT INTO with_meta (id, data, haex_hlc_no_trigger, haex_column_hlcs_no_trigger)
             VALUES ('r1', 'test', '2025-01-01T00:00:00.000Z-0001-d1',
                     '{\"data\":\"2025-01-01T00:00:00.000Z-0001-d1\"}')",
        [],
    )
    .unwrap();

    let changes = scan_table_for_local_changes(&conn, "with_meta", None, "device-1").unwrap();

    let col_names: Vec<&str> = changes.iter().map(|c| c.column_name.as_str()).collect();
    // Only "data" should remain; all metadata/CRDT columns filtered out
    assert!(col_names.contains(&"data"));
    assert!(!col_names.contains(&"last_push_hlc_timestamp_no_sync"));
    assert!(!col_names.contains(&"last_pull_server_timestamp_no_sync"));
    assert!(!col_names.contains(&"updated_at_no_sync"));
    assert!(!col_names.contains(&"created_at_no_sync"));
    assert!(!col_names.contains(&"haex_hlc_no_trigger"));
    assert!(!col_names.contains(&"haex_column_hlcs_no_trigger"));
}

#[test]
fn test_scan_uses_row_hlc_as_fallback() {
    let conn = setup_test_db();
    // Insert a row where haex_column_hlcs_no_trigger is empty — row-level HLC should be used
    conn.execute(
        "INSERT INTO test_items (id, name, value, haex_hlc_no_trigger, haex_column_hlcs_no_trigger)
             VALUES ('r1', 'test', 10, '2025-01-01T00:00:00.000Z-0001-d1', '{}')",
        [],
    )
    .unwrap();

    let changes = scan_table_for_local_changes(&conn, "test_items", None, "device-1").unwrap();

    // Both data columns should be emitted using the row-level HLC
    assert_eq!(changes.len(), 2);
    for change in &changes {
        assert_eq!(change.hlc_timestamp, "2025-01-01T00:00:00.000Z-0001-d1");
    }
}

#[test]
fn test_scan_empty_column_hlc_falls_back_to_row_hlc() {
    // Regression: a corrupt/legacy row can carry an empty-string per-column
    // HLC. It must be treated as absent (fall back to the row HLC), never
    // emitted as `hlc_timestamp = ""`. An empty HLC would feed
    // `compare_hlc_strings("")` on every apply (the `[HLC] cannot parse time
    // component of ""` flood) and could never converge (`"" > x` is false).
    let conn = setup_test_db();
    conn.execute(
        "INSERT INTO test_items (id, name, value, haex_hlc_no_trigger, haex_column_hlcs_no_trigger)
             VALUES ('r1', 'test', 10, '2025-01-01T00:00:00.000Z-0001-d1', '{\"name\":\"\",\"value\":\"\"}')",
        [],
    )
    .unwrap();

    let changes = scan_table_for_local_changes(&conn, "test_items", None, "device-1").unwrap();

    assert_eq!(changes.len(), 2);
    for change in &changes {
        assert_eq!(
            change.hlc_timestamp, "2025-01-01T00:00:00.000Z-0001-d1",
            "empty per-column HLC must fall back to the row HLC, never stay \"\""
        );
        assert!(!change.hlc_timestamp.is_empty());
    }
}

#[test]
fn test_scan_skips_row_when_all_hlcs_empty() {
    // Regression: when BOTH the per-column HLC and the row HLC are empty the
    // column has no usable timestamp and must be skipped. Emitting `""` is what
    // produced the empty-HLC log flood and a row that never synced.
    let conn = setup_test_db();
    conn.execute(
        "INSERT INTO test_items (id, name, value, haex_hlc_no_trigger, haex_column_hlcs_no_trigger)
             VALUES ('r1', 'test', 10, '', '{\"name\":\"\",\"value\":\"\"}')",
        [],
    )
    .unwrap();

    let changes = scan_table_for_local_changes(&conn, "test_items", None, "device-1").unwrap();

    assert!(
        changes.is_empty(),
        "rows with no usable HLC must not emit empty-string timestamps"
    );
}

#[test]
fn test_incremental_scan_admits_empty_row_hlc_with_valid_column_hlc() {
    // Regression: an incremental scan must not drop a corrupt/legacy row whose
    // row-level HLC is empty (`haex_hlc_no_trigger = ''`) but which still carries a valid,
    // newer per-column HLC. The SQL prefilter (`"haex_hlc_no_trigger" > after_hlc`) would
    // otherwise reject such a row before the per-column fallback could emit the
    // valid change, so the column would only ever converge on a full scan.
    let conn = setup_test_db();
    // Empty row HLC, but `name` has a per-column HLC newer than the cursor while
    // `value` stays at the old one.
    let hlcs = r#"{"name":"3000000000000000000/aabbccdd","value":"1000000000000000000/aabbccdd"}"#;
    conn.execute(
        "INSERT INTO test_items (id, name, value, haex_hlc_no_trigger, haex_column_hlcs_no_trigger)
             VALUES ('r1', 'updated', 10, '', ?1)",
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

    // Only `name` passes the per-column threshold, and it is emitted despite the
    // empty row HLC.
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].column_name, "name");
    assert_eq!(changes[0].hlc_timestamp, "3000000000000000000/aabbccdd");
}

#[test]
fn test_column_level_hlc_filtering() {
    let conn = setup_test_db();
    // Insert a row where 'name' has a newer HLC but 'value' has an older one
    let hlcs = r#"{"name":"3000000000000000000/aabbccdd","value":"1000000000000000000/aabbccdd"}"#;
    conn.execute(
        "INSERT INTO test_items (id, name, value, haex_hlc_no_trigger, haex_column_hlcs_no_trigger)
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
                haex_hlc_no_trigger TEXT,
                haex_column_hlcs_no_trigger TEXT NOT NULL DEFAULT '{}',
                PRIMARY KEY (group_id, item_id)
            );",
    )
    .unwrap();

    let hlcs = r#"{"data":"2025-01-01T00:00:00.000Z-0001-d1"}"#;
    conn.execute(
        "INSERT INTO composite_pk (group_id, item_id, data, haex_hlc_no_trigger, haex_column_hlcs_no_trigger)
             VALUES ('g1', 'i1', 'hello', '2025-01-01T00:00:00.000Z-0001-d1', ?1)",
        [hlcs],
    )
    .unwrap();

    let changes = scan_table_for_local_changes(&conn, "composite_pk", None, "device-1").unwrap();

    assert_eq!(changes.len(), 1); // data only

    let pks: serde_json::Map<String, JsonValue> =
        serde_json::from_str(&changes[0].row_pks).unwrap();
    assert_eq!(pks.get("group_id").unwrap(), "g1");
    assert_eq!(pks.get("item_id").unwrap(), "i1");
}

#[test]
fn test_scan_null_value() {
    let conn = setup_test_db();
    let hlcs =
        r#"{"name":"2025-01-01T00:00:00.000Z-0001-d1","value":"2025-01-01T00:00:00.000Z-0001-d1"}"#;
    conn.execute(
        "INSERT INTO test_items (id, name, value, haex_hlc_no_trigger, haex_column_hlcs_no_trigger)
             VALUES ('r1', NULL, NULL, '2025-01-01T00:00:00.000Z-0001-d1', ?1)",
        [hlcs],
    )
    .unwrap();

    let changes = scan_table_for_local_changes(&conn, "test_items", None, "device-1").unwrap();

    // NULL values should still produce changes for both data columns
    assert_eq!(changes.len(), 2);
    let name_change = changes.iter().find(|c| c.column_name == "name").unwrap();
    assert_eq!(name_change.value, JsonValue::Null);
}

#[test]
fn test_scan_nonexistent_table_returns_empty() {
    let conn = Connection::open_in_memory().unwrap();
    let changes = scan_table_for_local_changes(&conn, "nonexistent", None, "device-1").unwrap();
    assert!(changes.is_empty());
}
