//! Task 5 — Business-table DELETE cascade.
//!
//! Two mechanisms, one goal (per-space delete propagation, ADR 0002 §6.5):
//!
//! A. Space-scoped infra tables (haex_space_members, haex_peer_shares,
//!    haex_space_devices, haex_mls_sync_keys, haex_device_mls_enrollments)
//!    carry space_id NOT NULL and are denylisted from the register. A direct
//!    BEFORE-DELETE trigger emits into haex_shared_space_deleted_rows using
//!    OLD.space_id.
//!
//! B. Extension tables (anything else that isn't infra-of-infra) may live in
//!    multiple spaces via the register. A BEFORE-DELETE trigger cleans the
//!    register entries; the register-DELETE fanout from Task 4 then produces
//!    per-space signals.

use super::fixtures::{delete_log_rows, setup_business_delete_fixture};

#[test]
fn hard_delete_of_space_scoped_infra_row_emits_one_per_space_delete_log_entry() {
    // Path A: haex_peer_shares row lives in exactly one space via
    // OLD.space_id; DELETE emits exactly one delete-log signal.
    let conn = setup_business_delete_fixture();
    conn.execute(
        "INSERT INTO haex_peer_shares (id, space_id, name, haex_hlc_no_sync)
         VALUES ('share-1', 'SPACE_X', 'Folder', 'hlc-seed')",
        [],
    )
    .unwrap();

    conn.execute("DELETE FROM haex_peer_shares WHERE id = 'share-1'", [])
        .unwrap();

    let rows = delete_log_rows(&conn);
    assert_eq!(
        rows,
        vec![(
            "SPACE_X".to_string(),
            "haex_peer_shares".to_string(),
            r#"{"id":"share-1"}"#.to_string(),
        )],
        "hard delete of an infra row must emit exactly one per-space signal"
    );
}

#[test]
fn hard_delete_of_extension_row_shared_into_many_spaces_emits_one_per_space() {
    // Path B: an extension row lives in multiple spaces via register
    // entries. Hard-deleting the row must cascade to register cleanup;
    // the register-DELETE fanout (Task 4) then emits per-space signals.
    let conn = setup_business_delete_fixture();
    conn.execute(
        "INSERT INTO ext_notes_items (id, body, haex_hlc_no_sync)
         VALUES ('note-1', 'hello', 'hlc-seed')",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO haex_shared_space_sync (id, table_name, row_pks, space_id, haex_hlc_no_sync)
         VALUES ('reg-x', 'ext_notes_items', '{\"id\":\"note-1\"}', 'SPACE_X', 'hlc-1'),
                ('reg-y', 'ext_notes_items', '{\"id\":\"note-1\"}', 'SPACE_Y', 'hlc-2'),
                ('reg-z', 'ext_notes_items', '{\"id\":\"note-1\"}', 'SPACE_Z', 'hlc-3')",
        [],
    )
    .unwrap();

    conn.execute("DELETE FROM ext_notes_items WHERE id = 'note-1'", [])
        .unwrap();

    let rows = delete_log_rows(&conn);
    assert_eq!(
        rows.len(),
        3,
        "one signal per registered space, got {rows:?}"
    );
    let space_ids: Vec<&str> = rows.iter().map(|(s, _, _)| s.as_str()).collect();
    assert_eq!(space_ids, vec!["SPACE_X", "SPACE_Y", "SPACE_Z"]);
    for (_, table, pks) in &rows {
        assert_eq!(table, "ext_notes_items");
        assert_eq!(pks, r#"{"id":"note-1"}"#);
    }
    // Register itself is now empty for this row.
    let register_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM haex_shared_space_sync \
             WHERE table_name = 'ext_notes_items' AND row_pks = '{\"id\":\"note-1\"}'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        register_count, 0,
        "register entries must be gone after cascade"
    );
}

#[test]
fn hard_delete_of_infra_row_does_not_double_emit_from_register_cascade() {
    // Regression guard: infra tables are denylisted from the register.
    // The cascade DELETE FROM register must therefore find zero matching
    // register rows — the ONLY signal must come from Path A's direct
    // emit. If both fired we'd see two rows for the same delete.
    let conn = setup_business_delete_fixture();
    conn.execute(
        "INSERT INTO haex_peer_shares (id, space_id, name, haex_hlc_no_sync)
         VALUES ('share-1', 'SPACE_X', 'Folder', 'hlc-seed')",
        [],
    )
    .unwrap();

    conn.execute("DELETE FROM haex_peer_shares WHERE id = 'share-1'", [])
        .unwrap();

    let rows = delete_log_rows(&conn);
    assert_eq!(
        rows.len(),
        1,
        "exactly one signal for an infra delete, no double-emit; got {rows:?}"
    );
}
