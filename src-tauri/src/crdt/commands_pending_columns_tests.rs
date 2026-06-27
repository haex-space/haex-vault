//! Tests for the schema-skew / pending-column skip branch of
//! `apply_remote_changes_to_db`.
//!
//! When an inbound CRDT change names a column that does not exist in the
//! local schema (the local device is on an older schema), the apply path
//! records the column in `haex_crdt_pending_columns` and skips it. It must
//! NOT record an HLC for that never-applied column in the row's
//! `haex_column_hlcs`: `haex_column_hlcs` is the per-column HLC of the last
//! *applied* value. If a skipped column's original HLC `H` were persisted
//! there, the post-migration recovery re-fetch — which carries the SAME
//! original HLC `H` — would be gated out by the strict `hlc_is_newer(H, H)`
//! check (`H > H` is false) and silently no-op. The pending-columns table is
//! the tracker for skipped columns; the per-column HLC map must stay empty
//! for them so recovery applies the recovered value normally (`H > ""`).

use super::*;
use crate::crdt::trigger::DELETED_ROWS_TABLE;
use crate::database::DbConnection;
use crate::table_names::{TABLE_CRDT_CONFIGS, TABLE_CRDT_PENDING_COLUMNS};
use rusqlite::params;
use serde_json::Value as JsonValue;
use std::sync::{Arc, Mutex};

/// Minimal apply harness. A target CRDT table (`devices`) with two existing
/// columns (`name`, `nick`) alongside the CRDT bookkeeping columns, plus the
/// configs table (for the triggers-enabled toggle) and the pending-columns
/// table that the skip branch writes to.
///
/// The to-be-migrated-in column `bio` is deliberately absent at setup so it
/// hits the skip branch; tests that exercise recovery `ALTER TABLE` it in.
fn setup_db() -> DbConnection {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    conn.execute_batch(&format!(
        "CREATE TABLE {TABLE_CRDT_CONFIGS} (key TEXT PRIMARY KEY, type TEXT, value TEXT);
         CREATE TABLE {TABLE_CRDT_PENDING_COLUMNS} (
             table_name TEXT NOT NULL,
             column_name TEXT NOT NULL,
             row_pks TEXT NOT NULL,
             PRIMARY KEY(table_name, column_name, row_pks)
         );
         CREATE TABLE {DELETED_ROWS_TABLE} (
             id TEXT PRIMARY KEY,
             table_name TEXT NOT NULL,
             row_pks TEXT NOT NULL,
             haex_hlc TEXT,
             haex_column_hlcs TEXT NOT NULL DEFAULT '{{}}'
         );
         CREATE TABLE devices (
             id TEXT PRIMARY KEY,
             name TEXT,
             nick TEXT,
             haex_hlc TEXT,
             haex_column_hlcs TEXT NOT NULL DEFAULT '{{}}'
         );"
    ))
    .unwrap();
    DbConnection(Arc::new(Mutex::new(Some(conn))))
}

fn change(pk: &str, col: &str, val: &str, hlc: &str) -> RemoteColumnChange {
    RemoteColumnChange {
        table_name: "devices".to_string(),
        row_pks: pk.to_string(),
        column_name: col.to_string(),
        hlc_timestamp: hlc.to_string(),
        decrypted_value: JsonValue::String(val.to_string()),
    }
}

/// Insert a row directly (bypassing the apply path) so tests that need an
/// existing row can establish a known starting state.
fn seed_row(db: &DbConnection, id: &str, name: &str, column_hlcs: &str, row_hlc: &str) {
    let guard = db.0.lock().unwrap();
    let conn = guard.as_ref().unwrap();
    conn.execute(
        "INSERT INTO devices (id, name, haex_hlc, haex_column_hlcs) VALUES (?, ?, ?, ?)",
        params![id, name, row_hlc, column_hlcs],
    )
    .unwrap();
}

/// Read a single TEXT column from the `devices` row, returning `None` if the
/// stored value is SQL NULL (column never written).
fn read_col(db: &DbConnection, id: &str, col: &str) -> Option<String> {
    let guard = db.0.lock().unwrap();
    let conn = guard.as_ref().unwrap();
    conn.query_row(
        &format!("SELECT \"{col}\" FROM devices WHERE id = ?"),
        params![id],
        |r| r.get::<_, Option<String>>(0),
    )
    .unwrap()
}

/// Parse the row's `haex_column_hlcs` JSON map.
fn column_hlcs_map(db: &DbConnection, id: &str) -> serde_json::Map<String, JsonValue> {
    let guard = db.0.lock().unwrap();
    let conn = guard.as_ref().unwrap();
    let raw: String = conn
        .query_row(
            "SELECT haex_column_hlcs FROM devices WHERE id = ?",
            params![id],
            |r| r.get(0),
        )
        .unwrap();
    serde_json::from_str(&raw).unwrap_or_default()
}

fn row_hlc(db: &DbConnection, id: &str) -> Option<String> {
    let guard = db.0.lock().unwrap();
    let conn = guard.as_ref().unwrap();
    conn.query_row(
        "SELECT haex_hlc FROM devices WHERE id = ?",
        params![id],
        |r| r.get::<_, Option<String>>(0),
    )
    .unwrap()
}

fn pending_count(db: &DbConnection, table: &str, column: &str) -> i64 {
    let guard = db.0.lock().unwrap();
    let conn = guard.as_ref().unwrap();
    conn.query_row(
        &format!(
            "SELECT COUNT(*) FROM {TABLE_CRDT_PENDING_COLUMNS} \
             WHERE table_name = ? AND column_name = ?"
        ),
        params![table, column],
        |r| r.get(0),
    )
    .unwrap()
}

fn pending_row_pks(db: &DbConnection, table: &str, column: &str) -> Vec<String> {
    let guard = db.0.lock().unwrap();
    let conn = guard.as_ref().unwrap();
    let mut stmt = conn
        .prepare(&format!(
            "SELECT row_pks FROM {TABLE_CRDT_PENDING_COLUMNS} \
             WHERE table_name = ? AND column_name = ? ORDER BY row_pks"
        ))
        .unwrap();
    stmt.query_map(params![table, column], |r| r.get::<_, String>(0))
        .unwrap()
        .map(Result::unwrap)
        .collect()
}

fn devices_row_count(db: &DbConnection) -> i64 {
    let guard = db.0.lock().unwrap();
    let conn = guard.as_ref().unwrap();
    conn.query_row("SELECT COUNT(*) FROM devices", [], |r| r.get(0))
        .unwrap()
}

/// `ALTER TABLE` the previously-missing column in, simulating the migration
/// that lands the new schema. Now the recovery re-pull can apply it.
fn add_bio_column(db: &DbConnection) {
    let guard = db.0.lock().unwrap();
    let conn = guard.as_ref().unwrap();
    conn.execute("ALTER TABLE devices ADD COLUMN bio TEXT", [])
        .unwrap();
}

const HLC_OLD: &str = "5/abcdef";
const HLC_MID: &str = "7/abcdef";
const HLC_NEW: &str = "9/abcdef";

// ---------------------------------------------------------------------------
// 1. Skip does not record the skipped column's HLC (core fix).
// ---------------------------------------------------------------------------

#[test]
fn skip_does_not_record_column_hlc_for_missing_column() {
    let db = setup_db();
    // Existing row already carries `name` at HLC_OLD.
    seed_row(
        &db,
        "dev-1",
        "old-name",
        &format!(r#"{{"name":"{HLC_OLD}"}}"#),
        HLC_OLD,
    );

    let new_name = format!("name-{}", rand::random::<u32>());
    let bio_val = format!("bio-{}", rand::random::<u32>());

    // Batch touches an EXISTING column `name` (newer HLC) and a MISSING
    // column `bio`. The co-applied `name` forces the row's haex_column_hlcs
    // to be rewritten — which is exactly when the buggy code leaked `bio`'s
    // HLC into the persisted map.
    let changes = vec![
        change(r#"{"id":"dev-1"}"#, "name", &new_name, HLC_NEW),
        change(r#"{"id":"dev-1"}"#, "bio", &bio_val, HLC_MID),
    ];

    let result = apply_remote_changes_to_db(&db, changes, None, None);
    assert!(result.is_ok(), "{result:?}");

    // bio is tracked as pending...
    assert_eq!(
        pending_count(&db, "devices", "bio"),
        1,
        "missing column must be recorded in pending-columns table"
    );

    // ...but its HLC must NOT appear in the row's per-column HLC map.
    let hlcs = column_hlcs_map(&db, "dev-1");
    assert!(
        !hlcs.contains_key("bio"),
        "skipped (never-applied) column must NOT have an HLC persisted in \
         haex_column_hlcs, else post-migration recovery is gated out by the \
         strict hlc_is_newer check; got {hlcs:?}"
    );

    // The existing column applied normally and DID record its HLC.
    assert_eq!(
        read_col(&db, "dev-1", "name").as_deref(),
        Some(new_name.as_str()),
        "existing column with a newer HLC must apply"
    );
    assert_eq!(
        hlcs.get("name").and_then(|v| v.as_str()),
        Some(HLC_NEW),
        "applied column's HLC must be recorded"
    );
}

// ---------------------------------------------------------------------------
// 2. Recovery LANDS after migration (the previously-broken scenario).
// ---------------------------------------------------------------------------

#[test]
fn recovery_applies_skipped_value_after_migration() {
    let db = setup_db();
    seed_row(
        &db,
        "dev-1",
        "old-name",
        &format!(r#"{{"name":"{HLC_OLD}"}}"#),
        HLC_OLD,
    );

    let bio_val = format!("bio-{}", rand::random::<u32>());

    // --- Pre-migration sync: `bio` arrives but the column doesn't exist. ---
    let pre = vec![
        change(r#"{"id":"dev-1"}"#, "name", "name-v2", HLC_NEW),
        change(r#"{"id":"dev-1"}"#, "bio", &bio_val, HLC_MID),
    ];
    apply_remote_changes_to_db(&db, pre, None, None).unwrap();

    // Precondition: skip must not have leaked bio's HLC (else this test would
    // pass for the wrong reason).
    assert!(
        !column_hlcs_map(&db, "dev-1").contains_key("bio"),
        "precondition: skipped column's HLC must be absent before recovery"
    );

    // --- Migration adds the column. ---
    add_bio_column(&db);

    // --- Recovery re-pull: SAME original HLC (HLC_MID) for bio. ---
    let recovery = vec![change(r#"{"id":"dev-1"}"#, "bio", &bio_val, HLC_MID)];
    let result = apply_remote_changes_to_db(&db, recovery, None, None);
    assert!(result.is_ok(), "{result:?}");

    // The recovered value MUST now be written to the row...
    assert_eq!(
        read_col(&db, "dev-1", "bio").as_deref(),
        Some(bio_val.as_str()),
        "recovery must write the previously-skipped value to the now-existing column"
    );
    // ...and its (original) HLC must now be recorded as the applied HLC.
    assert_eq!(
        column_hlcs_map(&db, "dev-1")
            .get("bio")
            .and_then(|v| v.as_str()),
        Some(HLC_MID),
        "recovery must record the recovered column's HLC as the applied HLC"
    );
}

// ---------------------------------------------------------------------------
// 3. Skip-only row (no co-applied column): track pending, no spurious write.
// ---------------------------------------------------------------------------

#[test]
fn skip_only_row_tracks_pending_without_writing_row() {
    let db = setup_db();
    // No existing row, batch touches ONLY the missing column.
    let bio_val = format!("bio-{}", rand::random::<u32>());
    let changes = vec![change(r#"{"id":"ghost"}"#, "bio", &bio_val, HLC_MID)];

    let result = apply_remote_changes_to_db(&db, changes, None, None);
    assert!(result.is_ok(), "skip-only batch must not error: {result:?}");

    assert_eq!(
        pending_count(&db, "devices", "bio"),
        1,
        "missing column must still be tracked as pending"
    );
    assert_eq!(
        devices_row_count(&db),
        0,
        "a row whose only change is a skipped column must not be inserted"
    );
}

// ---------------------------------------------------------------------------
// 4. Regression — normal LWW for an EXISTING column is unchanged.
// ---------------------------------------------------------------------------

#[test]
fn existing_column_newer_hlc_applies_equal_or_older_does_not() {
    // (a) newer HLC applies.
    {
        let db = setup_db();
        seed_row(
            &db,
            "dev-1",
            "old-name",
            &format!(r#"{{"name":"{HLC_OLD}"}}"#),
            HLC_OLD,
        );
        let changes = vec![change(r#"{"id":"dev-1"}"#, "name", "newer", HLC_NEW)];
        apply_remote_changes_to_db(&db, changes, None, None).unwrap();
        assert_eq!(
            read_col(&db, "dev-1", "name").as_deref(),
            Some("newer"),
            "strictly-newer HLC must overwrite"
        );
        assert_eq!(
            column_hlcs_map(&db, "dev-1")
                .get("name")
                .and_then(|v| v.as_str()),
            Some(HLC_NEW)
        );
        assert_eq!(
            row_hlc(&db, "dev-1").as_deref(),
            Some(HLC_NEW),
            "row haex_hlc must advance to the applied change's HLC"
        );
    }

    // (b) equal HLC does NOT apply (strict-newer gate preserved).
    {
        let db = setup_db();
        seed_row(
            &db,
            "dev-1",
            "keep-me",
            &format!(r#"{{"name":"{HLC_MID}"}}"#),
            HLC_MID,
        );
        let changes = vec![change(
            r#"{"id":"dev-1"}"#,
            "name",
            "should-not-win",
            HLC_MID,
        )];
        apply_remote_changes_to_db(&db, changes, None, None).unwrap();
        assert_eq!(
            read_col(&db, "dev-1", "name").as_deref(),
            Some("keep-me"),
            "equal HLC must NOT overwrite (LWW gate is strict-greater)"
        );
    }

    // (c) older HLC does NOT apply.
    {
        let db = setup_db();
        seed_row(
            &db,
            "dev-1",
            "keep-me",
            &format!(r#"{{"name":"{HLC_NEW}"}}"#),
            HLC_NEW,
        );
        let changes = vec![change(r#"{"id":"dev-1"}"#, "name", "stale", HLC_OLD)];
        apply_remote_changes_to_db(&db, changes, None, None).unwrap();
        assert_eq!(
            read_col(&db, "dev-1", "name").as_deref(),
            Some("keep-me"),
            "older HLC must NOT overwrite"
        );
    }
}

// ---------------------------------------------------------------------------
// 5. Pending tracking is row-aware: the same (column,row) twice is idempotent,
//    but distinct rows of one column are each tracked.
// ---------------------------------------------------------------------------

#[test]
fn skipping_same_row_column_twice_is_idempotent_but_distinct_rows_each_track() {
    let db = setup_db();
    // Same (column,row) twice → one marker (INSERT OR IGNORE on the triple PK).
    apply_remote_changes_to_db(
        &db,
        vec![change(r#"{"id":"a"}"#, "bio", "v1", HLC_OLD)],
        None,
        None,
    )
    .unwrap();
    apply_remote_changes_to_db(
        &db,
        vec![change(r#"{"id":"a"}"#, "bio", "v2", HLC_NEW)],
        None,
        None,
    )
    .unwrap();
    assert_eq!(
        pending_row_pks(&db, "devices", "bio"),
        vec![r#"{"id":"a"}"#.to_string()]
    );
    // A different row of the same column → a SECOND marker (row-aware).
    apply_remote_changes_to_db(
        &db,
        vec![change(r#"{"id":"b"}"#, "bio", "v3", HLC_NEW)],
        None,
        None,
    )
    .unwrap();
    assert_eq!(
        pending_row_pks(&db, "devices", "bio"),
        vec![r#"{"id":"a"}"#.to_string(), r#"{"id":"b"}"#.to_string()],
        "distinct rows of one column are tracked separately now"
    );
}

// ---------------------------------------------------------------------------
// 6. Skip records the owed row's PKs in the marker (row-aware contract).
// ---------------------------------------------------------------------------

#[test]
fn skip_records_owed_row_pks() {
    let db = setup_db();
    let changes = vec![change(r#"{"id":"dev-1"}"#, "bio", "v1", HLC_MID)];
    apply_remote_changes_to_db(&db, changes, None, None).unwrap();
    assert_eq!(
        pending_row_pks(&db, "devices", "bio"),
        vec![r#"{"id":"dev-1"}"#.to_string()],
        "skip must record the owed row's PKs in the marker"
    );
}
