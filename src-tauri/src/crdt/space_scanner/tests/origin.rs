//! Tests for the origin-node filter — the `origin_node_filter` parameter of
//! `scan_table_for_local_changes_scoped` that prevents push ping-pong:
//! inbound rows pulled from a peer carry that peer's HLC node-id and would
//! otherwise be re-scanned and pushed back.
//!
//! The fixture table here deliberately omits `haex_column_sigs_no_sync`,
//! so these also cover the no-sig-column shape.

use crate::crdt::space_scanner::scan_table_for_local_changes_scoped;
use haex_crdt::device_uuid_to_hlc_node;
use rusqlite::Connection;
use serde_json::Value as JsonValue;

fn setup_scoped_db() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(
        "CREATE TABLE scoped_items (
            id TEXT PRIMARY KEY,
            space_id TEXT NOT NULL,
            data TEXT,
            haex_hlc_no_sync TEXT,
            haex_column_hlcs_no_sync TEXT NOT NULL DEFAULT '{}'
        );",
    )
    .unwrap();
    conn
}

fn insert_row(conn: &Connection, id: &str, space_id: &str, data: &str, hlc: &str) {
    let hlcs = format!("{{\"space_id\":\"{hlc}\",\"data\":\"{hlc}\"}}");
    conn.execute(
        "INSERT INTO scoped_items (id, space_id, data, haex_hlc_no_sync, haex_column_hlcs_no_sync)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        rusqlite::params![id, space_id, data, hlc, hlcs],
    )
    .unwrap();
}

#[test]
fn drops_rows_from_other_node() {
    let conn = setup_scoped_db();
    let our_uuid = "01020304-0506-0708-090a-0b0c0d0e0f10";
    let other_uuid = "ffeeddcc-bbaa-9988-7766-554433221100";
    let our_node = device_uuid_to_hlc_node(our_uuid).unwrap();
    let other_node = device_uuid_to_hlc_node(other_uuid).unwrap();

    // Real uhlc strings the scanner would see in production.
    let our_hlc = format!("1000/{our_node:x}");
    let other_hlc = format!("2000/{other_node:x}");

    insert_row(&conn, "ours", "space-A", "keep", &our_hlc);
    insert_row(&conn, "theirs", "space-A", "drop", &other_hlc);

    let changes = scan_table_for_local_changes_scoped(
        &conn,
        "scoped_items",
        None,
        "device-1",
        Some("space-A"),
        Some(our_node),
    )
    .unwrap();

    // Only the row with our node-id may pass — without this filter the
    // ping-pong re-push of inbound rows would smuggle "theirs" back to
    // the leader.
    assert!(
        !changes.is_empty(),
        "expected at least one change for our row"
    );
    for change in &changes {
        let pks: serde_json::Map<String, JsonValue> =
            serde_json::from_str(&change.row_pks).unwrap();
        let id = pks.get("id").and_then(|v| v.as_str()).unwrap();
        assert_eq!(id, "ours", "leaked foreign-origin row: {id}");
    }
}

#[test]
fn filter_off_returns_all_origins() {
    // Sanity check: passing `None` keeps the legacy behaviour (no origin
    // gating) so callers like the leader's pull path still see every row.
    let conn = setup_scoped_db();
    let our_uuid = "01020304-0506-0708-090a-0b0c0d0e0f10";
    let other_uuid = "ffeeddcc-bbaa-9988-7766-554433221100";
    let our_node = device_uuid_to_hlc_node(our_uuid).unwrap();
    let other_node = device_uuid_to_hlc_node(other_uuid).unwrap();

    insert_row(
        &conn,
        "ours",
        "space-A",
        "keep",
        &format!("1000/{our_node:x}"),
    );
    insert_row(
        &conn,
        "theirs",
        "space-A",
        "keep",
        &format!("2000/{other_node:x}"),
    );

    let changes = scan_table_for_local_changes_scoped(
        &conn,
        "scoped_items",
        None,
        "device-1",
        Some("space-A"),
        None,
    )
    .unwrap();

    let ids: std::collections::HashSet<String> = changes
        .iter()
        .map(|c| {
            let pks: serde_json::Map<String, JsonValue> = serde_json::from_str(&c.row_pks).unwrap();
            pks.get("id").and_then(|v| v.as_str()).unwrap().to_string()
        })
        .collect();
    assert!(ids.contains("ours"));
    assert!(ids.contains("theirs"));
}
