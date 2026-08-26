//! Tests for the cloud sync-rule / space-binding consistency check in
//! `create_provider("cloud", ...)`.
//!
//! Exercises `verify_cloud_space_binding` directly against an in-memory
//! `haex_shared_space_sync` table — mirrors the join
//! `remote_storage::share_command::find_existing_share` uses to recover a
//! shared backend's bound space (`row_pks[0]` holds the backend id).

use std::sync::{Arc, Mutex as StdMutex};

use rusqlite::{params, Connection};
use uuid::Uuid;

use crate::database::DbConnection;
use crate::file_sync::commands::{verify_cloud_space_binding, FileSyncCommandError};

fn setup_db() -> DbConnection {
    let conn = Connection::open_in_memory().expect("in-memory DB");
    conn.execute_batch(
        "CREATE TABLE haex_shared_space_sync (
            id TEXT PRIMARY KEY,
            table_name TEXT NOT NULL,
            row_pks TEXT NOT NULL,
            space_id TEXT NOT NULL
        );",
    )
    .expect("schema setup");
    DbConnection(Arc::new(StdMutex::new(Some(conn))))
}

fn seed_share(db: &DbConnection, backend_id: &str, space_id: &str) {
    let guard = db.0.lock().expect("db lock");
    let conn = guard.as_ref().expect("db open");
    let row_pks = serde_json::to_string(&vec![backend_id]).expect("serialize row_pks");
    conn.execute(
        "INSERT INTO haex_shared_space_sync (id, table_name, row_pks, space_id) \
         VALUES (?1, 'haex_s3_backends', ?2, ?3)",
        params![Uuid::new_v4().to_string(), row_pks, space_id],
    )
    .expect("seed share row");
}

// Fresh UUIDs per test — no literal ids, and isolates each case's rows from
// every other test's in-memory DB (each gets its own connection anyway, but
// this also keeps assertions readable without magic strings).
fn fresh_id() -> String {
    Uuid::new_v4().to_string()
}

#[test]
fn owner_only_backend_without_space_id_is_allowed() {
    let db = setup_db();
    let backend_id = fresh_id();
    assert!(verify_cloud_space_binding(&backend_id, None, &db).is_ok());
}

#[test]
fn owner_only_backend_with_space_id_is_rejected() {
    let db = setup_db();
    let backend_id = fresh_id();
    let space_id = fresh_id();
    let err = verify_cloud_space_binding(&backend_id, Some(&space_id), &db)
        .expect_err("owner-only backend + spaceId must be rejected");
    assert!(matches!(err, FileSyncCommandError::InvalidConfig(_)));
}

#[test]
fn shared_backend_with_matching_space_id_is_allowed() {
    let db = setup_db();
    let backend_id = fresh_id();
    let space_id = fresh_id();
    seed_share(&db, &backend_id, &space_id);
    assert!(verify_cloud_space_binding(&backend_id, Some(&space_id), &db).is_ok());
}

#[test]
fn shared_backend_with_mismatched_space_id_is_rejected() {
    let db = setup_db();
    let backend_id = fresh_id();
    let bound_space = fresh_id();
    let other_space = fresh_id();
    seed_share(&db, &backend_id, &bound_space);
    let err = verify_cloud_space_binding(&backend_id, Some(&other_space), &db)
        .expect_err("mismatched spaceId must be rejected");
    assert!(matches!(err, FileSyncCommandError::InvalidConfig(_)));
}

#[test]
fn shared_backend_without_space_id_is_rejected() {
    let db = setup_db();
    let backend_id = fresh_id();
    let bound_space = fresh_id();
    seed_share(&db, &backend_id, &bound_space);
    let err = verify_cloud_space_binding(&backend_id, None, &db)
        .expect_err("missing spaceId against a shared backend must be rejected");
    assert!(matches!(err, FileSyncCommandError::InvalidConfig(_)));
}
