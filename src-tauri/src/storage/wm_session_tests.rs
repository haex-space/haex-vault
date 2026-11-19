//! Tests for `wm_session` (spec 022-session-restore, research R1/R2). The
//! table is `_no_sync`, so haex-crdt stamps no HLC, but the schema only
//! exists on a vault opened with holzi's migrations — these open a real
//! `Database` like the other storage tests.

use std::sync::Arc;

use haex_crdt::rusqlite::params;
use haex_crdt::{Database, DatabaseConfig, NoopSignatureProvider, SqlCipherKey};
use serde_json::json;
use uuid::Uuid;

use super::query;
use super::wm_session::{delete, load, save, WmSessionError, MAX_SESSION_BYTES};
use crate::identity::{
    haex_migration_source, installation_id_path, HaexBootstrap, HAEX_TRIGGER_VERSION,
};

fn open_test_vault(passphrase: &str) -> (tempfile::TempDir, Database) {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = Database::open(DatabaseConfig {
        path: dir.path().join("vault.db"),
        key: SqlCipherKey::new(passphrase),
        create_if_missing: true,
        bootstrap: Arc::new(HaexBootstrap::new(installation_id_path(dir.path()))),
        signature_provider: Arc::new(NoopSignatureProvider),
        migration_source: haex_migration_source(),
        trigger_version: HAEX_TRIGGER_VERSION,
        max_transaction_bytes: haex_crdt::MAX_CRDT_TRANSACTION_BYTES,
        max_value_bytes: haex_crdt::MAX_VALUE_BYTES,
    })
    .expect("open test vault");
    (dir, db)
}

fn session(marker: &str) -> serde_json::Value {
    json!({ "version": 1, "workspaces": [], "windows": [], "activeWorkspaceId": marker })
}

fn row_count(db: &Database) -> i64 {
    use super::query::Query;
    query::read(db, |r| {
        r.query_row("SELECT COUNT(*) FROM wm_sessions_no_sync", &[], |row| {
            row.get(0)
        })
    })
    .expect("count rows")
    .unwrap_or(0)
}

fn load_session(db: &Database, device: Uuid) -> Result<Option<serde_json::Value>, WmSessionError> {
    query::read(db, |r| Ok(load(r, device))).expect("run load")
}

fn save_session(
    db: &Database,
    device: Uuid,
    value: &serde_json::Value,
) -> Result<(), WmSessionError> {
    let mut outcome = None;
    let _ = db.write(|tx| {
        let result = save(tx, device, value);
        let failed = result.is_err();
        outcome = Some(result);
        if failed {
            Err(haex_crdt::Error::consumer("roll back the failed save"))
        } else {
            Ok(())
        }
    });
    outcome.expect("the closure ran")
}

#[test]
fn save_inserts_then_overwrites_the_single_row_of_a_device() {
    let (_dir, db) = open_test_vault("wm-session-overwrite");
    let device = Uuid::new_v4();
    save_session(&db, device, &session("first")).expect("first save");
    save_session(&db, device, &session("second")).expect("second save");
    assert_eq!(row_count(&db), 1);
    let loaded = load_session(&db, device).expect("load");
    assert_eq!(loaded, Some(session("second")));
}

#[test]
fn load_returns_none_without_a_saved_session() {
    let (_dir, db) = open_test_vault("wm-session-empty");
    let loaded = load_session(&db, Uuid::new_v4()).expect("load");
    assert_eq!(loaded, None);
}

#[test]
fn delete_removes_only_the_own_device_row() {
    let (_dir, db) = open_test_vault("wm-session-delete");
    let own = Uuid::new_v4();
    let other = Uuid::new_v4();
    save_session(&db, own, &session("own")).expect("save own");
    save_session(&db, other, &session("other")).expect("save other");
    db.write(|tx| delete(tx, own)).expect("delete own");
    let own_loaded = load_session(&db, own).expect("own");
    let other_loaded = load_session(&db, other).expect("other");
    assert_eq!(own_loaded, None);
    assert_eq!(other_loaded, Some(session("other")));
}

#[test]
fn save_rejects_anything_but_a_json_object() {
    let (_dir, db) = open_test_vault("wm-session-not-object");
    let device = Uuid::new_v4();
    let result = save_session(&db, device, &json!([1, 2, 3]));
    assert!(matches!(result, Err(WmSessionError::NotAnObject)));
    assert_eq!(row_count(&db), 0);
}

#[test]
fn save_rejects_a_session_above_the_size_limit() {
    let (_dir, db) = open_test_vault("wm-session-too-large");
    let device = Uuid::new_v4();
    let big = json!({ "version": 1, "padding": "x".repeat(MAX_SESSION_BYTES) });
    let result = save_session(&db, device, &big);
    assert!(matches!(result, Err(WmSessionError::TooLarge { .. })));
    assert_eq!(row_count(&db), 0);
}

#[test]
fn a_corrupt_stored_row_loads_as_an_error_not_a_panic() {
    let (_dir, db) = open_test_vault("wm-session-corrupt");
    let device = Uuid::new_v4();
    db.write(|tx| {
        tx.execute(
            "INSERT INTO wm_sessions_no_sync (vault_device_uuid, session_json, updated_at) \
             VALUES (?1, 'not json', '2026-09-26T00:00:00Z')",
            params![device.to_string()],
        )
    })
    .expect("insert a corrupt row");
    let result = load_session(&db, device);
    assert!(matches!(result, Err(WmSessionError::Json(_))));
}
