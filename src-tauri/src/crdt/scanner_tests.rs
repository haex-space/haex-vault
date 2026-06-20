use super::*;
use rusqlite::Connection;

/// Test-only helper: unscoped single-table scan. Production code must use
/// `scan_table_for_local_changes_scoped` (or the space-scoped whitelist
/// entry point `scan_space_scoped_tables_for_local_changes`) — an unscoped
/// scan over a table shared by multiple spaces leaks cross-space rows.
pub fn scan_table_for_local_changes(
    conn: &Connection,
    table_name: &str,
    after_hlc: Option<&str>,
    device_id: &str,
) -> Result<Vec<LocalColumnChange>, DatabaseError> {
    scan_table_for_local_changes_scoped(conn, table_name, after_hlc, device_id, None, None)
}

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

    let changes = scan_table_for_local_changes(&conn, "with_meta", None, "device-1").unwrap();

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

    let changes = scan_table_for_local_changes(&conn, "test_items", None, "device-1").unwrap();

    // Both data columns should be emitted using the row-level HLC
    assert_eq!(changes.len(), 2);
    for change in &changes {
        assert_eq!(change.hlc_timestamp, "2025-01-01T00:00:00.000Z-0001-d1");
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
        "INSERT INTO test_items (id, name, value, haex_hlc, haex_column_hlcs)
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

fn insert_scoped_row(conn: &Connection, id: &str, space_id: &str, data: &str, hlc: &str) {
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

/// Creates a "vault-private-like" CRDT table that is NOT in
/// [`SPACE_SCOPED_CRDT_TABLES`] and carries no `space_id` column — the
/// shape of a per-vault private table (e.g. passwords). Used to prove the
/// owner scanner ships such tables, which a space-scoped scan never would.
fn setup_vault_private_test_db() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(
        "CREATE TABLE haex_passwords (
                id TEXT PRIMARY KEY,
                secret TEXT,
                haex_hlc TEXT,
                haex_column_hlcs TEXT NOT NULL DEFAULT '{}'
            );",
    )
    .unwrap();
    conn
}

fn insert_private_row(conn: &Connection, id: &str, secret: &str, hlc: &str) {
    let hlcs = format!("{{\"secret\":\"{hlc}\"}}");
    conn.execute(
        "INSERT INTO haex_passwords (id, secret, haex_hlc, haex_column_hlcs)
             VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params![id, secret, hlc, hlcs],
    )
    .unwrap();
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
                haex_hlc TEXT,
                haex_column_hlcs TEXT NOT NULL DEFAULT '{}'
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
            crate::crdt::hlc::compare_hlc_strings(&pair[0].hlc_timestamp, &pair[1].hlc_timestamp,),
            std::cmp::Ordering::Greater,
            "owner scan result is not globally HLC-ordered"
        );
    }
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
