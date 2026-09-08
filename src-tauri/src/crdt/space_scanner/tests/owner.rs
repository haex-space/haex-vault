//! The `space_id`-column scoped filter and the two OWNER-ONLY unscoped
//! scanners.

use super::fixtures::{
    insert_private_row, insert_row, insert_scoped_row, setup_scoped_test_db, setup_test_db,
    setup_vault_private_test_db,
};
use crate::crdt::space_scanner::{
    scan_all_crdt_tables_for_owner, scan_single_column_for_owner,
    scan_table_for_local_changes_scoped,
};
use serde_json::Value as JsonValue;

#[test]
fn test_scoped_filter_returns_only_matching_space() {
    let conn = setup_scoped_test_db();
    insert_scoped_row(
        &conn,
        "r1",
        "space-A",
        "hello",
        "2025-01-01T00:00:00.000Z-0001-d1",
    );
    insert_scoped_row(
        &conn,
        "r2",
        "space-A",
        "world",
        "2025-01-01T00:00:00.000Z-0002-d1",
    );
    insert_scoped_row(
        &conn,
        "r3",
        "space-B",
        "leak",
        "2025-01-01T00:00:00.000Z-0003-d1",
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

    // 2 matching rows × 2 data columns (space_id, data) = 4 changes.
    assert_eq!(changes.len(), 4);
    assert!(changes.iter().all(|change| change.sig.is_some()));
    assert!(
        changes
            .iter()
            .all(|change| change.column_name != "haex_column_sigs_no_trigger"),
        "signature metadata must never be emitted as user data"
    );

    // No row from space-B may appear — this is the leak gate.
    for change in &changes {
        let pks: serde_json::Map<String, JsonValue> =
            serde_json::from_str(&change.row_pks).unwrap();
        let id = pks.get("id").and_then(|v| v.as_str()).unwrap();
        assert!(
            id == "r1" || id == "r2",
            "leaked row from other space: {id}"
        );
    }
}

#[test]
fn scan_all_crdt_tables_for_owner_includes_vault_private_and_space_tables() {
    // One vault-private table (no space_id, off the space whitelist) and
    // one space-scoped-like table sharing the same connection.
    let conn = setup_vault_private_test_db();
    conn.execute_batch(
        "CREATE TABLE scoped_items (
                id TEXT PRIMARY KEY,
                space_id TEXT NOT NULL,
                data TEXT,
                haex_hlc_no_trigger TEXT,
                haex_column_hlcs_no_trigger TEXT NOT NULL DEFAULT '{}',
                haex_column_sigs_no_trigger TEXT NOT NULL DEFAULT '{}'
            );",
    )
    .unwrap();

    // Distinct HLC timestamps so we can assert global ordering. Use known
    // monotonically increasing logical-clock values (no secret literals).
    let secret_a: u64 = rand::random();
    let secret_b: u64 = rand::random();
    insert_private_row(
        &conn,
        "p1",
        &format!("v{secret_a}"),
        "1000000000000000000/aabbccdd",
    );
    insert_private_row(
        &conn,
        "p2",
        &format!("v{secret_b}"),
        "3000000000000000000/aabbccdd",
    );
    insert_scoped_row(
        &conn,
        "s1",
        "space-A",
        "hello",
        "2000000000000000000/aabbccdd",
    );

    let table_names = vec!["haex_passwords".to_string(), "scoped_items".to_string()];
    let changes =
        scan_all_crdt_tables_for_owner(&conn, &table_names, None, "device-1", None).unwrap();

    // Rows from BOTH tables must appear — proving no space filter is
    // applied. The vault-private table is the leak-relevant one: a
    // space-scoped scan would never return it.
    let tables: std::collections::HashSet<&str> =
        changes.iter().map(|c| c.table_name.as_str()).collect();
    assert!(
        tables.contains("haex_passwords"),
        "owner scan dropped vault-private table"
    );
    assert!(
        tables.contains("scoped_items"),
        "owner scan dropped space-scoped table"
    );

    // Result must be globally HLC-ordered (non-decreasing), mirroring the
    // sibling fn's global sort.
    for pair in changes.windows(2) {
        assert_ne!(
            haex_crdt::compare_hlc_strings(&pair[0].hlc_timestamp, &pair[1].hlc_timestamp,),
            std::cmp::Ordering::Greater,
            "owner scan result is not globally HLC-ordered"
        );
    }
}

#[test]
fn scan_all_crdt_tables_for_owner_strips_per_space_sigs() {
    // `scoped_items` rows carry per-space signatures in
    // `haex_column_sigs_no_trigger`. Owner-vault sync is unscoped — it has no
    // space to key a signature by — so the mapper runs with
    // `sig_space_id = None` and every change must ship unsigned. That is the
    // property `LocalColumnChange::sig`'s doc comment claims; a sig leaking
    // onto this path would change the owner-vault wire format and hand a
    // receiver a signature it cannot scope.
    let conn = setup_scoped_test_db();
    insert_scoped_row(
        &conn,
        "r1",
        "space-A",
        "hello",
        "2025-01-01T00:00:00.000Z-0001-d1",
    );

    let changes = scan_all_crdt_tables_for_owner(
        &conn,
        &["scoped_items".to_string()],
        None,
        "device-1",
        None,
    )
    .unwrap();

    assert!(
        !changes.is_empty(),
        "the fixture must produce changes for the sig assertion to be meaningful"
    );
    assert!(
        changes.iter().all(|change| change.sig.is_none()),
        "owner-vault sync must ship unsigned changes even when the row carries \
         per-space sigs: {changes:?}"
    );
}

#[test]
fn scan_all_crdt_tables_for_owner_empty_table_list_returns_empty() {
    let conn = setup_vault_private_test_db();
    insert_private_row(&conn, "p1", "x", "1000000000000000000/aabbccdd");

    let changes = scan_all_crdt_tables_for_owner(&conn, &[], None, "device-1", None).unwrap();
    assert!(changes.is_empty());
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

#[test]
fn scan_single_column_for_owner_returns_only_requested_column() {
    // `test_items` has two data columns: `name` and `value`. Scanning for
    // `name` must never return a `value` change, and vice versa.
    let conn = setup_test_db();
    insert_row(&conn, "r1", "hello", 42, "1000000000000000000/aabbccdd");
    insert_row(&conn, "r2", "world", 99, "2000000000000000000/aabbccdd");

    let changes = scan_single_column_for_owner(&conn, "test_items", "name", "device-1").unwrap();

    // Two rows, one `name` change each — and nothing for `value`.
    assert_eq!(changes.len(), 2);
    assert!(changes.iter().all(|c| c.column_name == "name"));
    assert!(
        changes.iter().all(|c| c.table_name == "test_items"),
        "table name must be carried through"
    );
}

#[test]
fn scan_single_column_for_owner_full_dump_ignores_hlc_threshold() {
    // Recovery has no cursor: every row's value for the column must come back,
    // even rows whose HLC would be "old" relative to any threshold. There is
    // no `after_hlc` parameter, so all rows are returned regardless of age.
    let conn = setup_test_db();
    insert_row(&conn, "ancient", "a", 1, "1000000000000000000/aabbccdd");
    insert_row(&conn, "recent", "b", 2, "9000000000000000000/aabbccdd");

    let changes = scan_single_column_for_owner(&conn, "test_items", "value", "device-1").unwrap();

    // Both rows present — the "ancient" one is NOT filtered out.
    assert_eq!(changes.len(), 2);
    let pks: std::collections::HashSet<String> =
        changes.iter().map(|c| c.row_pks.clone()).collect();
    assert!(
        pks.contains("{\"id\":\"ancient\"}"),
        "full dump must include the old row"
    );
    assert!(
        pks.contains("{\"id\":\"recent\"}"),
        "full dump must include the new row"
    );
}

#[test]
fn scan_single_column_for_owner_does_not_origin_filter() {
    // Rows authored by OTHER devices carry a different node-id in the HLC
    // suffix. Recovery wants the COMPLETE column state, so those rows must
    // still be returned — the opposite of the origin-filtered push path.
    let conn = setup_test_db();
    // Two distinct HLC node-id suffixes => two distinct authoring nodes.
    insert_row(&conn, "mine", "x", 1, "1000000000000000000/aabbccdd");
    insert_row(&conn, "theirs", "y", 2, "2000000000000000000/11223344");

    let changes = scan_single_column_for_owner(&conn, "test_items", "name", "device-1").unwrap();

    // Sanity: the two rows genuinely carry different node-ids.
    let mine = haex_crdt::parse_hlc_node_hex("aabbccdd").unwrap();
    let theirs = haex_crdt::parse_hlc_node_hex("11223344").unwrap();
    assert_ne!(mine, theirs);

    // Both rows returned despite differing authoring nodes => no origin filter.
    assert_eq!(changes.len(), 2);
    let suffixes: std::collections::HashSet<Option<&str>> = changes
        .iter()
        .map(|c| haex_crdt::hlc_node_id_suffix(&c.hlc_timestamp))
        .collect();
    assert!(suffixes.contains(&Some("aabbccdd")));
    assert!(suffixes.contains(&Some("11223344")));
}

#[test]
fn scan_single_column_for_owner_nonexistent_table_or_column_is_empty() {
    let conn = setup_test_db();
    insert_row(&conn, "r1", "hello", 42, "1000000000000000000/aabbccdd");

    // Nonexistent table => empty, no error.
    let no_table =
        scan_single_column_for_owner(&conn, "does_not_exist", "name", "device-1").unwrap();
    assert!(no_table.is_empty());

    // Existing table, but a column no row has => empty, no error.
    let no_column =
        scan_single_column_for_owner(&conn, "test_items", "nonexistent_col", "device-1").unwrap();
    assert!(no_column.is_empty());
}
