//! Tests for the cross-batch delete-resurrection guard in
//! `apply_remote_changes_to_db`.
//!
//! Deletes are tracked in the delete-log table `haex_deleted_rows`, not via
//! tombstone columns. After the apply loop,
//! `propagate_deleted_rows_to_target_tables` issues DELETEs — but ONLY for
//! delete-log entries arriving in the CURRENT batch. If a delete-log entry for
//! a row was stored in a PRIOR batch (or the row's table did not exist yet, so
//! propagation was a no-op) and a later INSERT for that row arrives with an
//! OLDER-or-equal HLC, the row must NOT be (re)inserted — applying it would
//! resurrect a row a newer delete already killed.
//!
//! Symmetric with `should_propagate_delete`: the delete wins on an HLC tie.

use super::*;
use crate::crdt::trigger::DELETED_ROWS_TABLE;
use crate::database::DbConnection;
use crate::table_names::TABLE_CRDT_CONFIGS;
use rusqlite::params;
use serde_json::Value as JsonValue;
use std::sync::{Arc, Mutex};

fn setup_db() -> DbConnection {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    conn.execute_batch(&format!(
        "CREATE TABLE {TABLE_CRDT_CONFIGS} (key TEXT PRIMARY KEY, type TEXT, value TEXT);
         CREATE TABLE {DELETED_ROWS_TABLE} (
             id TEXT PRIMARY KEY, table_name TEXT NOT NULL, row_pks TEXT NOT NULL,
             haex_hlc TEXT, haex_column_hlcs TEXT NOT NULL DEFAULT '{{}}'
         );
         CREATE TABLE items (
             id TEXT PRIMARY KEY, name TEXT,
             haex_hlc TEXT, haex_column_hlcs TEXT NOT NULL DEFAULT '{{}}'
         );
         CREATE TABLE pair (
             a TEXT NOT NULL, b TEXT NOT NULL, val TEXT,
             haex_hlc TEXT, haex_column_hlcs TEXT NOT NULL DEFAULT '{{}}',
             PRIMARY KEY (a, b)
         );"
    ))
    .unwrap();
    DbConnection(Arc::new(Mutex::new(Some(conn))))
}

fn change(table: &str, row_pks: &str, col: &str, val: &str, hlc: &str) -> RemoteColumnChange {
    RemoteColumnChange {
        table_name: table.to_string(),
        row_pks: row_pks.to_string(),
        column_name: col.to_string(),
        hlc_timestamp: hlc.to_string(),
        decrypted_value: JsonValue::String(val.to_string()),
    }
}

fn seed_delete_log(db: &DbConnection, table: &str, row_pks: &str, hlc: &str) {
    let guard = db.0.lock().unwrap();
    let conn = guard.as_ref().unwrap();
    conn.execute(
        &format!(
            "INSERT INTO {DELETED_ROWS_TABLE} (id, table_name, row_pks, haex_hlc) VALUES (?, ?, ?, ?)"
        ),
        params![format!("del-{}", rand::random::<u32>()), table, row_pks, hlc],
    )
    .unwrap();
}

fn row_exists(db: &DbConnection, table: &str, where_sql: &str) -> bool {
    let guard = db.0.lock().unwrap();
    let conn = guard.as_ref().unwrap();
    conn.query_row(
        &format!("SELECT COUNT(*) FROM {table} WHERE {where_sql}"),
        [],
        |r| r.get::<_, i64>(0),
    )
    .unwrap()
        > 0
}

const HLC_OLD: &str = "5/aaaaaa";
const HLC_NEW: &str = "9/aaaaaa";

#[test]
fn insert_shadowed_by_newer_delete_is_suppressed() {
    let db = setup_db();
    seed_delete_log(&db, "items", r#"{"id":"r1"}"#, HLC_NEW);
    apply_remote_changes_to_db(
        &db,
        vec![change(
            "items",
            r#"{"id":"r1"}"#,
            "name",
            "resurrected",
            HLC_OLD,
        )],
        None,
        None,
    )
    .unwrap();
    assert!(
        !row_exists(&db, "items", "id = 'r1'"),
        "older insert shadowed by newer delete must NOT resurrect"
    );
}

#[test]
fn insert_newer_than_delete_is_applied() {
    let db = setup_db();
    seed_delete_log(&db, "items", r#"{"id":"r1"}"#, HLC_OLD);
    apply_remote_changes_to_db(
        &db,
        vec![change(
            "items",
            r#"{"id":"r1"}"#,
            "name",
            "reinserted",
            HLC_NEW,
        )],
        None,
        None,
    )
    .unwrap();
    assert!(
        row_exists(&db, "items", "id = 'r1' AND name = 'reinserted'"),
        "strictly newer insert must win (legitimate re-insert)"
    );
}

#[test]
fn insert_equal_hlc_to_delete_is_suppressed() {
    let db = setup_db();
    seed_delete_log(&db, "items", r#"{"id":"r1"}"#, HLC_NEW);
    apply_remote_changes_to_db(
        &db,
        vec![change("items", r#"{"id":"r1"}"#, "name", "tie", HLC_NEW)],
        None,
        None,
    )
    .unwrap();
    assert!(
        !row_exists(&db, "items", "id = 'r1'"),
        "equal HLC: delete wins (symmetric with should_propagate_delete)"
    );
}

#[test]
fn insert_without_delete_log_entry_is_applied() {
    let db = setup_db();
    apply_remote_changes_to_db(
        &db,
        vec![change("items", r#"{"id":"r1"}"#, "name", "fresh", HLC_OLD)],
        None,
        None,
    )
    .unwrap();
    assert!(
        row_exists(&db, "items", "id = 'r1'"),
        "no delete-log entry: normal insert"
    );
}

// Behavioural backstop for the per-table shadow-deletes cache: many absent
// rows on the SAME table in one batch must still each be evaluated against the
// table's full delete-log (the cache reuses one read for the whole table, so a
// regression would either over- or under-suppress).
#[test]
fn many_absent_rows_same_table_each_check_against_full_delete_log() {
    let db = setup_db();
    // Delete-log has two distinct rows of `items`. Incoming batch carries
    // older inserts for BOTH — both must be suppressed off the same cache.
    seed_delete_log(&db, "items", r#"{"id":"r1"}"#, HLC_NEW);
    seed_delete_log(&db, "items", r#"{"id":"r2"}"#, HLC_NEW);
    apply_remote_changes_to_db(
        &db,
        vec![
            change("items", r#"{"id":"r1"}"#, "name", "ghost1", HLC_OLD),
            change("items", r#"{"id":"r2"}"#, "name", "ghost2", HLC_OLD),
        ],
        None,
        None,
    )
    .unwrap();
    assert!(
        !row_exists(&db, "items", "id IN ('r1','r2')"),
        "both shadowed older inserts must be suppressed"
    );
}

#[test]
fn composite_pk_match_is_order_agnostic() {
    // delete-log stored with keys in one order, incoming change in another →
    // must still match via parsed-map compare.
    let db = setup_db();
    seed_delete_log(&db, "pair", r#"{"b":"2","a":"1"}"#, HLC_NEW);
    apply_remote_changes_to_db(
        &db,
        vec![change(
            "pair",
            r#"{"a":"1","b":"2"}"#,
            "val",
            "resurrected",
            HLC_OLD,
        )],
        None,
        None,
    )
    .unwrap();
    assert!(
        !row_exists(&db, "pair", "a = '1' AND b = '2'"),
        "composite-PK row must match regardless of key order in stored row_pks"
    );
}

// Pure helper tests
#[test]
fn delete_shadows_insert_truth_table() {
    assert!(
        super::delete_shadows_insert(HLC_NEW, HLC_OLD),
        "older insert is shadowed"
    );
    assert!(
        super::delete_shadows_insert(HLC_NEW, HLC_NEW),
        "equal: shadowed (delete wins)"
    );
    assert!(
        !super::delete_shadows_insert(HLC_OLD, HLC_NEW),
        "strictly newer insert is NOT shadowed"
    );
}

#[test]
fn insert_suppressed_by_deletes_matches_on_parsed_map() {
    let insert_pks: serde_json::Map<String, JsonValue> =
        serde_json::from_str(r#"{"a":"1","b":"2"}"#).unwrap();
    let shadow = (
        serde_json::from_str::<serde_json::Map<String, JsonValue>>(r#"{"b":"2","a":"1"}"#).unwrap(),
        HLC_NEW.to_string(),
    );
    let other = (
        serde_json::from_str::<serde_json::Map<String, JsonValue>>(r#"{"a":"9"}"#).unwrap(),
        HLC_NEW.to_string(),
    );
    assert!(super::insert_suppressed_by_deletes(
        &insert_pks,
        HLC_OLD,
        &[shadow.clone()]
    ));
    assert!(!super::insert_suppressed_by_deletes(
        &insert_pks,
        HLC_OLD,
        &[other]
    ));
    // matching row but the insert is strictly newer → not suppressed
    assert!(!super::insert_suppressed_by_deletes(
        &insert_pks,
        HLC_NEW,
        &[(shadow.0.clone(), HLC_OLD.to_string())]
    ));
}
