// src-tauri/src/crdt/compaction_anchor_tests.rs
//
// Tests for the vault-owned compaction-anchor helpers. Included from
// `compaction_anchor.rs` via `#[path = "compaction_anchor_tests.rs"]` so
// production stays under the 500-LoC per-file cap.

use super::*;
use crate::crdt::trigger::{DELETED_ROWS_TABLE, SHARED_SPACE_DELETED_ROWS_TABLE};
use haex_crdt::table_names::TABLE_CRDT_CONFIGS;
use rusqlite::Connection;

/// Test-only stub for the `current_hlc()` SQL UDF. Production wires this to
/// `HlcService::new_timestamp()`; in-memory unit tests only need a
/// monotonically-increasing string so the compaction-anchor upsert can
/// succeed.
fn register_current_hlc_stub(conn: &Connection) {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(1);
    conn.create_scalar_function(
        "current_hlc",
        0,
        rusqlite::functions::FunctionFlags::SQLITE_UTF8,
        |_| {
            let n = COUNTER.fetch_add(1, Ordering::SeqCst);
            Ok(format!("{n}/testnode"))
        },
    )
    .expect("register current_hlc test stub");
}

fn setup_anchor_db() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    register_current_hlc_stub(&conn);
    // Mirror migration 0013's partial unique index so the atomic
    // `ON CONFLICT (key) WHERE device_id IS NULL` upsert in
    // `advance_owner_delete_log_anchor` can target it.
    conn.execute_batch(&format!(
        "CREATE TABLE haex_space_compaction_anchors (
             space_id TEXT PRIMARY KEY NOT NULL,
             min_valid_hlc TEXT NOT NULL DEFAULT '0',
             haex_hlc_no_trigger TEXT,
             haex_column_hlcs_no_trigger TEXT NOT NULL DEFAULT '{{}}',
             haex_column_sigs_no_trigger TEXT NOT NULL DEFAULT '{{}}'
         );
         CREATE TABLE haex_vault_settings (
             id TEXT PRIMARY KEY NOT NULL,
             key TEXT NOT NULL,
             value TEXT,
             device_id TEXT
         );
         CREATE UNIQUE INDEX haex_vault_settings_key_device_unique
             ON haex_vault_settings (key, device_id);
         CREATE UNIQUE INDEX idx_haex_vault_settings_owner_key
             ON haex_vault_settings (key) WHERE device_id IS NULL;
         CREATE TABLE {SHARED_SPACE_DELETED_ROWS_TABLE} (
             id TEXT PRIMARY KEY NOT NULL,
             space_id TEXT NOT NULL,
             table_name TEXT NOT NULL,
             row_pks TEXT NOT NULL,
             haex_hlc_no_trigger TEXT
         );
         CREATE TABLE {DELETED_ROWS_TABLE} (
             id TEXT PRIMARY KEY NOT NULL,
             table_name TEXT NOT NULL,
             row_pks TEXT NOT NULL,
             haex_hlc_no_trigger TEXT,
             haex_column_hlcs_no_trigger TEXT NOT NULL DEFAULT '{{}}'
         );"
    ))
    .unwrap();
    conn
}

#[test]
fn advance_shared_space_anchor_sets_first_value() {
    let conn = setup_anchor_db();
    advance_shared_space_anchor(&conn, "SPACE_X", "100/aabb").unwrap();
    let v: String = conn
        .query_row(
            "SELECT min_valid_hlc FROM haex_space_compaction_anchors WHERE space_id = 'SPACE_X'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(v, "100/aabb");
}

#[test]
fn advance_shared_space_anchor_is_monotonic() {
    let conn = setup_anchor_db();
    advance_shared_space_anchor(&conn, "SPACE_X", "100/aabb").unwrap();
    advance_shared_space_anchor(&conn, "SPACE_X", "50/aabb").unwrap();
    let v: String = conn
        .query_row(
            "SELECT min_valid_hlc FROM haex_space_compaction_anchors WHERE space_id = 'SPACE_X'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(v, "100/aabb", "regression must be rejected (max-wins)");
}

#[test]
fn advance_shared_space_anchor_advances_on_newer() {
    let conn = setup_anchor_db();
    advance_shared_space_anchor(&conn, "SPACE_X", "100/aabb").unwrap();
    advance_shared_space_anchor(&conn, "SPACE_X", "200/aabb").unwrap();
    let v: String = conn
        .query_row(
            "SELECT min_valid_hlc FROM haex_space_compaction_anchors WHERE space_id = 'SPACE_X'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(v, "200/aabb");
}

#[test]
fn advance_owner_delete_log_anchor_inserts_when_absent() {
    let conn = setup_anchor_db();
    advance_owner_delete_log_anchor(&conn, "300/aabb").unwrap();
    let v: String = conn
        .query_row(
            "SELECT value FROM haex_vault_settings \
             WHERE key = ?1 AND device_id IS NULL",
            [OWNER_DELETE_LOG_ANCHOR_KEY],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(v, "300/aabb");
}

#[test]
fn advance_owner_delete_log_anchor_is_monotonic() {
    let conn = setup_anchor_db();
    advance_owner_delete_log_anchor(&conn, "300/aabb").unwrap();
    advance_owner_delete_log_anchor(&conn, "100/aabb").unwrap();
    let v: String = conn
        .query_row(
            "SELECT value FROM haex_vault_settings \
             WHERE key = ?1 AND device_id IS NULL",
            [OWNER_DELETE_LOG_ANCHOR_KEY],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(v, "300/aabb", "regression must be rejected");
}

#[test]
fn prune_shared_space_delete_log_time_based_advances_anchor_and_prunes() {
    // Seed: two old entries in SPACE_X that fall below the cutoff and one
    // new entry that stays. After the helper runs the old rows are gone and
    // SPACE_X's anchor has advanced to at least the pruned max.
    let mut conn = setup_anchor_db();
    conn.execute_batch(&format!(
        "CREATE TABLE IF NOT EXISTS {TABLE_CRDT_CONFIGS} (
             key TEXT PRIMARY KEY NOT NULL,
             value TEXT,
             type TEXT
         );
         INSERT INTO {TABLE_CRDT_CONFIGS} (key, value, type) \
         VALUES ('hlc_timestamp', '9223372036854775807/aabb', 'hlc');"
    ))
    .unwrap();
    conn.execute(
        &format!(
            "INSERT INTO {SHARED_SPACE_DELETED_ROWS_TABLE} \
             (id, space_id, table_name, row_pks, haex_hlc_no_trigger) VALUES \
             ('old-1', 'SPACE_X', 'ext_notes', '{{\"id\":\"a\"}}', '1/aabb'), \
             ('old-2', 'SPACE_X', 'ext_notes', '{{\"id\":\"b\"}}', '5/aabb'), \
             ('new-1', 'SPACE_X', 'ext_notes', '{{\"id\":\"c\"}}', '9223372036854775800/aabb')"
        ),
        [],
    )
    .unwrap();

    let tx = conn.transaction().unwrap();
    let pruned = prune_shared_space_delete_log_and_advance_anchors(
        &tx,
        haex_crdt::RetentionPolicy::TimeBasedDays { days: 30 },
    )
    .unwrap();
    tx.commit().unwrap();
    assert_eq!(pruned, 2, "old-1 and old-2 below cutoff");

    let anchor_hlc: Option<String> = conn
        .query_row(
            "SELECT min_valid_hlc FROM haex_space_compaction_anchors \
             WHERE space_id = 'SPACE_X'",
            [],
            |r| r.get(0),
        )
        .ok();
    assert!(
        matches!(anchor_hlc.as_deref(), Some(v) if haex_crdt::compare_hlc_strings(v, "5/aabb") != std::cmp::Ordering::Less),
        "anchor must advance to at least max HLC of pruned entries, got {anchor_hlc:?}"
    );

    // The newer entry survives, the two old ones are gone.
    let surviving: Vec<String> = conn
        .prepare(&format!(
            "SELECT id FROM {SHARED_SPACE_DELETED_ROWS_TABLE} ORDER BY id"
        ))
        .unwrap()
        .query_map([], |r| r.get::<_, String>(0))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(surviving, vec!["new-1".to_string()]);
}

#[test]
fn prune_shared_space_delete_log_all_advances_anchor_per_space() {
    // Two spaces, one entry each, policy=All → both anchors advance to
    // their own space's HLC and both rows are gone. NULL-HLC rows stay.
    let mut conn = setup_anchor_db();
    conn.execute(
        &format!(
            "INSERT INTO {SHARED_SPACE_DELETED_ROWS_TABLE} \
             (id, space_id, table_name, row_pks, haex_hlc_no_trigger) VALUES \
             ('x-1', 'SPACE_X', 'ext_notes', '{{\"id\":\"a\"}}', '100/aabb'), \
             ('y-1', 'SPACE_Y', 'ext_notes', '{{\"id\":\"b\"}}', '200/ccdd'), \
             ('z-null', 'SPACE_Z', 'ext_notes', '{{\"id\":\"c\"}}', NULL)"
        ),
        [],
    )
    .unwrap();

    let tx = conn.transaction().unwrap();
    let pruned =
        prune_shared_space_delete_log_and_advance_anchors(&tx, haex_crdt::RetentionPolicy::All)
            .unwrap();
    tx.commit().unwrap();
    assert_eq!(pruned, 2, "x-1 and y-1 pruned, z-null stays (NULL HLC)");

    let x_anchor: String = conn
        .query_row(
            "SELECT min_valid_hlc FROM haex_space_compaction_anchors WHERE space_id = 'SPACE_X'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(x_anchor, "100/aabb");
    let y_anchor: String = conn
        .query_row(
            "SELECT min_valid_hlc FROM haex_space_compaction_anchors WHERE space_id = 'SPACE_Y'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(y_anchor, "200/ccdd");
    let z_anchor: Option<String> = conn
        .query_row(
            "SELECT min_valid_hlc FROM haex_space_compaction_anchors WHERE space_id = 'SPACE_Z'",
            [],
            |r| r.get(0),
        )
        .optional()
        .unwrap();
    assert!(z_anchor.is_none(), "NULL-HLC row must not advance anchor");

    // Non-NULL rows are gone; the NULL-HLC row stays.
    let surviving: Vec<String> = conn
        .prepare(&format!(
            "SELECT id FROM {SHARED_SPACE_DELETED_ROWS_TABLE} ORDER BY id"
        ))
        .unwrap()
        .query_map([], |r| r.get::<_, String>(0))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(surviving, vec!["z-null".to_string()]);
}
