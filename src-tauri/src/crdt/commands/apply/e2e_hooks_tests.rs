#![cfg(all(test, feature = "e2e-hooks"))]

use std::sync::{Arc, Mutex};

use rusqlite::Connection;

use super::e2e_hooks::{
    seed_shared_space_delete_log_entry_impl, TestPropagationOutcome, TestSeedDeleteLogReport,
};
use crate::database::DbConnection;

const TABLE: &str = "e2e_test_items";
const ROW_PKS_JSON: &str = r#"{"id":"row-a"}"#;
const SPACE_X: &str = "space-x";
const SPACE_Y: &str = "space-y";
const DELETE_HLC: &str = "20/aaa";

fn setup() -> DbConnection {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(
        "\
         CREATE TABLE haex_shared_space_deleted_rows (\
             id TEXT PRIMARY KEY,\
             space_id TEXT NOT NULL,\
             table_name TEXT NOT NULL,\
             row_pks TEXT NOT NULL,\
             haex_hlc TEXT\
         );\
         CREATE TABLE haex_shared_space_sync (\
             id TEXT PRIMARY KEY,\
             space_id TEXT NOT NULL,\
             table_name TEXT NOT NULL,\
             row_pks TEXT NOT NULL,\
             haex_hlc TEXT\
         );\
         CREATE TABLE e2e_test_items (\
             id TEXT PRIMARY KEY,\
             body TEXT,\
             haex_hlc TEXT\
         );",
    )
    .unwrap();
    DbConnection(Arc::new(Mutex::new(Some(conn))))
}

fn insert_business_row(db: &DbConnection, id: &str, body: &str, hlc: &str) {
    let guard = db.0.lock().unwrap();
    let conn = guard.as_ref().unwrap();
    conn.execute(
        "INSERT INTO e2e_test_items (id, body, haex_hlc) VALUES (?, ?, ?)",
        rusqlite::params![id, body, hlc],
    )
    .unwrap();
}

fn insert_register(db: &DbConnection, space_id: &str) {
    let guard = db.0.lock().unwrap();
    let conn = guard.as_ref().unwrap();
    let register_id = format!("reg-{space_id}");
    conn.execute(
        "INSERT INTO haex_shared_space_sync \
         (id, space_id, table_name, row_pks, haex_hlc) VALUES (?, ?, ?, ?, ?)",
        rusqlite::params![register_id, space_id, TABLE, ROW_PKS_JSON, "1/aaa"],
    )
    .unwrap();
}

fn seed(db: &DbConnection, propagate: bool) -> TestSeedDeleteLogReport {
    seed_shared_space_delete_log_entry_impl(db, SPACE_X, TABLE, ROW_PKS_JSON, DELETE_HLC, propagate)
        .expect("seed impl must succeed")
}

#[test]
fn not_run_leaves_state_untouched_and_outcome_notrun() {
    let db = setup();
    insert_register(&db, SPACE_X);
    insert_business_row(&db, "row-a", "hello", "1/aaa");

    let report = seed(&db, false);

    assert!(matches!(report.outcome, TestPropagationOutcome::NotRun));
    assert!(!report.propagated);
    // Business row + register survive because we never called propagate.
    assert!(report.after.business_row_exists);
    assert!(report.after.target_space_registered);
}

#[test]
fn positive_gate_hits_full_delete_when_only_target_space_registered() {
    let db = setup();
    insert_register(&db, SPACE_X);
    insert_business_row(&db, "row-a", "hello", "1/aaa");

    let report = seed(&db, true);

    assert!(matches!(
        report.outcome,
        TestPropagationOutcome::AppliedFullDelete
    ));
    assert!(!report.after.target_space_registered);
    assert!(!report.after.any_space_registered);
    assert!(!report.after.business_row_exists);
}

#[test]
fn positive_gate_hits_register_only_when_another_space_still_registered() {
    let db = setup();
    insert_register(&db, SPACE_X);
    insert_register(&db, SPACE_Y);
    insert_business_row(&db, "row-a", "hello", "1/aaa");

    let report = seed(&db, true);

    assert!(matches!(
        report.outcome,
        TestPropagationOutcome::AppliedRegisterOnly
    ));
    assert!(!report.after.target_space_registered);
    assert!(report.after.any_space_registered);
    assert!(report.after.business_row_exists);
}

#[test]
fn not_shared_in_space_forgery_keeps_row_and_other_space_register() {
    let db = setup();
    // Row registered in SPACE_Y only; forged delete claims SPACE_X.
    insert_register(&db, SPACE_Y);
    insert_business_row(&db, "row-a", "hello", "1/aaa");

    let report = seed(&db, true);

    assert!(matches!(
        report.outcome,
        TestPropagationOutcome::NotSharedInSpaceForgery
    ));
    assert!(!report.after.target_space_registered);
    assert!(report.after.any_space_registered);
    assert!(report.after.business_row_exists);
}

#[test]
fn unshare_race_keeps_business_row_when_no_register_exists() {
    let db = setup();
    // Business row exists but no register anywhere (already-unshared state).
    insert_business_row(&db, "row-a", "hello", "1/aaa");

    let report = seed(&db, true);

    assert!(matches!(
        report.outcome,
        TestPropagationOutcome::UnshareRace
    ));
    assert!(!report.after.target_space_registered);
    assert!(!report.after.any_space_registered);
    assert!(report.after.business_row_exists);
}

#[test]
fn resurrection_check_suppresses_when_business_row_has_newer_hlc() {
    let db = setup();
    insert_register(&db, SPACE_X);
    // Local business row has HLC 30/aaa; delete-log entry has DELETE_HLC=20/aaa.
    insert_business_row(&db, "row-a", "hello", "30/aaa");

    let report = seed(&db, true);

    assert!(matches!(
        report.outcome,
        TestPropagationOutcome::ResurrectionSuppressed
    ));
    assert!(report.after.target_space_registered);
    assert!(report.after.business_row_exists);
}
