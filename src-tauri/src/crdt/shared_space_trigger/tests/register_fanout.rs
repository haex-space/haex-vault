//! Task 4 — Register-DELETE fanout trigger.
//!
//! Deleting a row from `haex_shared_space_sync` (register) must produce a
//! per-space signal in `haex_shared_space_deleted_rows` so other members of
//! the space converge on the removal (unshare or hard-delete). Owner-domain
//! sync continues to receive its signal via the standard `haex_deleted_rows`
//! trigger installed by the crate's installer.
//!
//! Aus ADR 0002 §6.5 (revised 2026-07-29).

use super::fixtures::setup_register_delete_fixture;

#[test]
fn deleting_register_entry_writes_shared_space_delete_log_row() {
    let conn = setup_register_delete_fixture();

    // Seed a register row saying "table T row {id:R} shared into SPACE_X".
    conn.execute(
        "INSERT INTO haex_shared_space_sync (id, table_name, row_pks, space_id, haex_hlc_no_sync)
         VALUES ('reg-1', 'haex_peer_shares', '{\"id\":\"R\"}', 'SPACE_X', 'hlc-seed')",
        [],
    )
    .unwrap();

    // Delete the register row (models an unshare or the cascade from a
    // business-table DELETE — see Task 5).
    conn.execute("DELETE FROM haex_shared_space_sync WHERE id = 'reg-1'", [])
        .unwrap();

    // Assert: a delete-log row landed with the correct per-space info.
    let rows: Vec<(String, String, String)> = conn
        .prepare("SELECT space_id, table_name, row_pks FROM haex_shared_space_deleted_rows")
        .unwrap()
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();

    assert_eq!(
        rows.len(),
        1,
        "exactly one shared-space-delete-log row expected, got {rows:?}"
    );
    assert_eq!(
        rows[0],
        (
            "SPACE_X".to_string(),
            "haex_peer_shares".to_string(),
            r#"{"id":"R"}"#.to_string()
        )
    );
}

#[test]
fn deleting_register_entry_gated_by_triggers_enabled_flag() {
    // When triggers_enabled=0 the fanout must NOT fire — this is the
    // apply-path gate that lets the receiver clear the register without
    // re-emitting a delete-log entry (which would loop).
    let conn = setup_register_delete_fixture();
    conn.execute(
        "UPDATE haex_crdt_configs_no_sync SET value = '0' WHERE key = 'triggers_enabled'",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO haex_shared_space_sync (id, table_name, row_pks, space_id, haex_hlc_no_sync)
         VALUES ('reg-1', 'haex_peer_shares', '{\"id\":\"R\"}', 'SPACE_X', 'hlc-seed')",
        [],
    )
    .unwrap();
    conn.execute("DELETE FROM haex_shared_space_sync WHERE id = 'reg-1'", [])
        .unwrap();

    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM haex_shared_space_deleted_rows",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        count, 0,
        "delete-log must not receive a row when triggers_enabled=0"
    );
}
