//! Guards on the seam between `haex_crdt`'s generic installer and vault's
//! shared-space layer.
//!
//! The crate installs a generic BEFORE-DELETE trigger on every table but
//! `haex_deleted_rows`. Vault's own per-space delete-log must not carry one
//! either (TRIGGER_VERSION v6), so the shared-space layer suppresses it.

use super::super::*;
use super::fixtures::{
    setup_business_delete_fixture, setup_register_delete_fixture, trigger_exists,
};

#[test]
fn install_on_shared_space_delete_log_suppresses_generic_delete_trigger() {
    let conn = setup_register_delete_fixture();

    let tx = conn.unchecked_transaction().unwrap();
    install_crdt_with_shared_space(&tx, SHARED_SPACE_DELETED_ROWS_TABLE, false)
        .expect("delete-log triggers");
    tx.commit().unwrap();

    assert!(
        !trigger_exists(&conn, "z_dirty_haex_shared_space_deleted_rows_delete"),
        "the per-space delete-log must not carry the crate's generic \
         BEFORE-DELETE trigger — retention pruning would re-log every \
         pruned row into haex_deleted_rows"
    );
    // The INSERT/UPDATE triggers the crate installs are wanted.
    assert!(trigger_exists(
        &conn,
        "z_dirty_haex_shared_space_deleted_rows_insert"
    ));
    assert!(trigger_exists(
        &conn,
        "z_dirty_haex_shared_space_deleted_rows_update"
    ));
}

#[test]
fn pruning_the_shared_space_delete_log_does_not_append_to_owner_delete_log() {
    // Behavioural half of the guard above: this models
    // `compaction_anchor::prune_shared_space_delete_log_and_advance_anchors`,
    // which hard-deletes with triggers_enabled = '1'.
    let conn = setup_register_delete_fixture();

    let tx = conn.unchecked_transaction().unwrap();
    install_crdt_with_shared_space(&tx, SHARED_SPACE_DELETED_ROWS_TABLE, false)
        .expect("delete-log triggers");
    tx.commit().unwrap();

    conn.execute(
        "INSERT INTO haex_shared_space_deleted_rows \
         (id, space_id, table_name, row_pks, haex_hlc_no_sync) \
         VALUES ('sig-1', 'SPACE_X', 'ext_notes_items', '{\"id\":\"note-1\"}', 'hlc-1')",
        [],
    )
    .unwrap();
    conn.execute("DELETE FROM haex_shared_space_deleted_rows", [])
        .unwrap();

    let owner_log_rows: i64 = conn
        .query_row("SELECT COUNT(*) FROM haex_deleted_rows", [], |r| r.get(0))
        .unwrap();
    assert_eq!(
        owner_log_rows, 0,
        "retention pruning of the per-space delete-log must not emit \
         owner-domain delete events"
    );
}

#[test]
fn drop_removes_both_the_generic_and_the_shared_space_triggers() {
    // Parity guard for the drop wrapper: it must clear the crate's three
    // and vault's shared-space triggers in one call.
    let conn = setup_business_delete_fixture();

    assert!(trigger_exists(&conn, "z_shared_space_delete_fanout"));
    assert!(trigger_exists(
        &conn,
        "z_shared_space_infra_emit_haex_peer_shares_delete"
    ));
    assert!(trigger_exists(
        &conn,
        "z_shared_space_register_cascade_ext_notes_items_delete"
    ));

    let tx = conn.unchecked_transaction().unwrap();
    drop_crdt_with_shared_space(&tx, "haex_shared_space_sync").unwrap();
    drop_crdt_with_shared_space(&tx, "haex_peer_shares").unwrap();
    drop_crdt_with_shared_space(&tx, "ext_notes_items").unwrap();
    tx.commit().unwrap();

    for name in [
        "z_shared_space_delete_fanout",
        "z_shared_space_infra_emit_haex_peer_shares_delete",
        "z_shared_space_register_cascade_ext_notes_items_delete",
        "z_dirty_haex_shared_space_sync_insert",
        "z_dirty_haex_peer_shares_update",
        "z_dirty_ext_notes_items_delete",
    ] {
        assert!(!trigger_exists(&conn, name), "{name} should be gone");
    }
}

#[test]
fn ensure_layers_the_register_cascade_onto_a_table_that_already_has_generic_triggers() {
    // The pre-composition `ensure_crdt_columns_and_triggers` short-
    // circuited on a single `z_dirty_{table}_insert` probe, so a table
    // that already had the generic triggers could never acquire its
    // register-cascade trigger. The composed wrapper applies the
    // shared-space layer unconditionally.
    let conn = setup_register_delete_fixture();
    conn.execute_batch(
        "CREATE TABLE ext_notes_items (
             id TEXT PRIMARY KEY NOT NULL,
             body TEXT,
             haex_hlc_no_sync TEXT,
             haex_column_hlcs_no_sync TEXT NOT NULL DEFAULT '{}',
             haex_column_sigs_no_sync TEXT NOT NULL DEFAULT '{}'
         );",
    )
    .unwrap();

    // Install only the crate's generic layer, so the cascade trigger is
    // absent while all three generic triggers are present.
    let tx = conn.unchecked_transaction().unwrap();
    haex_crdt::setup_triggers_for_table(&tx, "ext_notes_items", false).unwrap();
    tx.commit().unwrap();
    assert!(!trigger_exists(
        &conn,
        "z_shared_space_register_cascade_ext_notes_items_delete"
    ));

    let tx = conn.unchecked_transaction().unwrap();
    let (columns_added, _) = ensure_crdt_columns_and_triggers(&tx, "ext_notes_items").unwrap();
    tx.commit().unwrap();

    assert!(!columns_added, "the fixture table already has all three");
    assert!(
        trigger_exists(
            &conn,
            "z_shared_space_register_cascade_ext_notes_items_delete"
        ),
        "ensure must layer the shared-space cascade on regardless of the \
         generic triggers already being present"
    );
}

#[test]
fn ensure_on_nonexistent_table_does_not_install_shared_space_triggers() {
    let conn = setup_register_delete_fixture();

    let tx = conn.unchecked_transaction().unwrap();
    let (columns_added, triggers_created) =
        ensure_crdt_columns_and_triggers(&tx, "table_that_does_not_exist").unwrap();
    tx.commit().unwrap();

    assert!(!columns_added);
    assert!(!triggers_created);
    assert!(!trigger_exists(
        &conn,
        "z_shared_space_register_cascade_table_that_does_not_exist_delete"
    ));
}
