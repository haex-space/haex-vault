//! Tests for [`super::read_max_ucan_chain_depth`].
//!
//! Each case installs a minimal in-memory `haex_vault_settings` table with
//! just the columns the reader actually touches. That keeps the tests
//! independent of the full vault migration set — which pulls in every
//! CRDT trigger — and reproducible in isolation.

use rusqlite::Connection;

use super::{
    read_max_ucan_chain_depth, MAX_UCAN_CHAIN_DEPTH_DEFAULT, MAX_UCAN_CHAIN_DEPTH_KEY,
    MAX_UCAN_CHAIN_DEPTH_MAX, MAX_UCAN_CHAIN_DEPTH_MIN,
};

/// Minimal `haex_vault_settings` shape that matches production schema for
/// the columns the reader queries. `id` is required by the real table's
/// PRIMARY KEY but the reader never selects it, so a UUID-shaped stand-in
/// is enough.
fn setup() -> Connection {
    let conn = Connection::open_in_memory().expect("open in-memory sqlite");
    conn.execute_batch(
        "CREATE TABLE haex_vault_settings ( \
             id TEXT PRIMARY KEY, \
             key TEXT NOT NULL, \
             value TEXT, \
             device_id TEXT \
         );",
    )
    .expect("create haex_vault_settings");
    conn
}

fn insert_row(
    conn: &Connection,
    id: &str,
    key: &str,
    value: Option<&str>,
    device_id: Option<&str>,
) {
    conn.execute(
        "INSERT INTO haex_vault_settings (id, key, value, device_id) VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params![id, key, value, device_id],
    )
    .expect("insert settings row");
}

#[test]
fn missing_row_returns_default() {
    let conn = setup();
    let depth = read_max_ucan_chain_depth(&conn);
    assert_eq!(depth, MAX_UCAN_CHAIN_DEPTH_DEFAULT);
}

#[test]
fn valid_in_range_value_is_returned() {
    let conn = setup();
    insert_row(&conn, "row-1", MAX_UCAN_CHAIN_DEPTH_KEY, Some("3"), None);

    let depth = read_max_ucan_chain_depth(&conn);
    assert_eq!(depth, 3);
}

#[test]
fn unparseable_value_falls_back_to_default() {
    let conn = setup();
    insert_row(&conn, "row-1", MAX_UCAN_CHAIN_DEPTH_KEY, Some("abc"), None);

    let depth = read_max_ucan_chain_depth(&conn);
    assert_eq!(depth, MAX_UCAN_CHAIN_DEPTH_DEFAULT);
}

#[test]
fn zero_is_below_range_and_falls_back_to_default() {
    let conn = setup();
    insert_row(&conn, "row-1", MAX_UCAN_CHAIN_DEPTH_KEY, Some("0"), None);

    let depth = read_max_ucan_chain_depth(&conn);
    assert_eq!(depth, MAX_UCAN_CHAIN_DEPTH_DEFAULT);
}

#[test]
fn above_max_falls_back_to_default() {
    let conn = setup();
    let too_high = (MAX_UCAN_CHAIN_DEPTH_MAX + 1).to_string();
    insert_row(
        &conn,
        "row-1",
        MAX_UCAN_CHAIN_DEPTH_KEY,
        Some(&too_high),
        None,
    );

    let depth = read_max_ucan_chain_depth(&conn);
    assert_eq!(depth, MAX_UCAN_CHAIN_DEPTH_DEFAULT);
}

#[test]
fn null_value_falls_back_to_default() {
    let conn = setup();
    insert_row(&conn, "row-1", MAX_UCAN_CHAIN_DEPTH_KEY, None, None);

    let depth = read_max_ucan_chain_depth(&conn);
    assert_eq!(depth, MAX_UCAN_CHAIN_DEPTH_DEFAULT);
}

#[test]
fn device_scoped_row_is_ignored_when_vault_row_absent() {
    // Only a per-device row is present. The reader must not match it —
    // the depth cap is intentionally vault-global.
    let conn = setup();
    insert_row(
        &conn,
        "row-1",
        MAX_UCAN_CHAIN_DEPTH_KEY,
        Some("7"),
        Some("did:key:zAlice"),
    );

    let depth = read_max_ucan_chain_depth(&conn);
    assert_eq!(depth, MAX_UCAN_CHAIN_DEPTH_DEFAULT);
}

#[test]
fn vault_row_wins_over_coexisting_device_row() {
    let conn = setup();
    // Wrong-device row must not shadow the real vault-global row.
    insert_row(
        &conn,
        "row-1",
        MAX_UCAN_CHAIN_DEPTH_KEY,
        Some("15"),
        Some("did:key:zBob"),
    );
    insert_row(&conn, "row-2", MAX_UCAN_CHAIN_DEPTH_KEY, Some("4"), None);

    let depth = read_max_ucan_chain_depth(&conn);
    assert_eq!(depth, 4);
}

#[test]
fn min_boundary_is_accepted() {
    let conn = setup();
    let min_str = MAX_UCAN_CHAIN_DEPTH_MIN.to_string();
    insert_row(
        &conn,
        "row-1",
        MAX_UCAN_CHAIN_DEPTH_KEY,
        Some(&min_str),
        None,
    );

    let depth = read_max_ucan_chain_depth(&conn);
    assert_eq!(depth, MAX_UCAN_CHAIN_DEPTH_MIN);
}

#[test]
fn max_boundary_is_accepted() {
    let conn = setup();
    let max_str = MAX_UCAN_CHAIN_DEPTH_MAX.to_string();
    insert_row(
        &conn,
        "row-1",
        MAX_UCAN_CHAIN_DEPTH_KEY,
        Some(&max_str),
        None,
    );

    let depth = read_max_ucan_chain_depth(&conn);
    assert_eq!(depth, MAX_UCAN_CHAIN_DEPTH_MAX);
}
