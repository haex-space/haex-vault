//! Tests for `VaultDb`, the database handle that carries a tracker token (data-model.md).

use std::sync::Arc;

use haex_crdt::Database;

use crate::identity::installation_id_path;
use crate::instances::vault_config::vault_config;
use crate::vault_gate::VaultGate;

fn open_throwaway() -> (tempfile::TempDir, Arc<Database>) {
    let tmp = tempfile::tempdir().expect("temp dir");
    let db = Database::open(vault_config(
        "vault-db-test-passphrase",
        &tmp.path().join("vault.db"),
        &installation_id_path(tmp.path()),
        true,
    ))
    .expect("open a throwaway database");
    (tmp, Arc::new(db))
}

#[test]
fn a_clone_keeps_the_tracker_busy_until_the_last_clone_drops() {
    let (_tmp, db) = open_throwaway();
    let gate = VaultGate::new();
    assert!(gate.is_idle(), "nothing uses the database yet");

    let first = gate.vault_db(db).expect("open gate");
    let second = first.clone();
    assert!(!gate.is_idle());

    drop(first);
    assert!(!gate.is_idle(), "one clone is still alive");
    drop(second);
    assert!(gate.is_idle(), "the tracker empties after the last drop");
}

#[test]
fn deref_reaches_the_database() {
    let (_tmp, db) = open_throwaway();
    let expected = db.device_id();
    let gate = VaultGate::new();
    let vault_db = gate.vault_db(db).expect("open gate");

    assert_eq!(vault_db.device_id(), expected);
    let database: &Database = &vault_db;
    assert_eq!(database.device_id(), expected);
}

#[test]
fn a_closing_gate_rejects_new_database_handles() {
    let (_tmp, db) = open_throwaway();
    let gate = VaultGate::new();
    gate.request_close();

    assert!(matches!(
        gate.vault_db(db),
        Err(crate::error::HaexError::VaultClosed)
    ));
}

const VAULT_SCOPE: &str = "00000000-0000-0000-0000-000000000000";

fn insert_pref(tx: &mut haex_crdt::CrdtTransaction<'_>, key: &str) -> haex_crdt::Result<usize> {
    tx.execute(
        "INSERT INTO preferences (vault_device_uuid, key, value) VALUES (?1, ?2, 'v')",
        haex_crdt::rusqlite::params![VAULT_SCOPE, key],
    )
}

fn pref_hlcs(vault_db: &crate::vault_gate::VaultDb) -> Vec<Option<String>> {
    use crate::storage::query::Query;
    vault_db
        .read_blocking(|r| {
            r.query_map(
                "SELECT haex_hlc_no_sync FROM preferences WHERE key LIKE 'test.%' ORDER BY key",
                &[],
                |row| row.get(0),
            )
        })
        .expect("read preferences")
}

#[tokio::test]
async fn one_write_commits_every_statement_under_one_hlc() {
    let (_tmp, db) = open_throwaway();
    let vault_db = VaultGate::new().vault_db(db).expect("open gate");

    vault_db
        .write(|tx| {
            insert_pref(tx, "test.a")?;
            insert_pref(tx, "test.b")
        })
        .await
        .expect("write");

    let hlcs = pref_hlcs(&vault_db);
    assert_eq!(hlcs.len(), 2);
    assert!(hlcs[0].is_some(), "the transformer stamps the row HLC");
    assert_eq!(hlcs[0], hlcs[1], "both rows carry the transaction HLC");
}

#[tokio::test]
async fn an_error_rolls_back_and_comes_back_as_the_haex_error() {
    let (_tmp, db) = open_throwaway();
    let vault_db = VaultGate::new().vault_db(db).expect("open gate");

    let err = vault_db
        .write(|tx| {
            insert_pref(tx, "test.a")?;
            Err::<(), _>(
                crate::error::HaexError::InvalidInput {
                    reason: "stop".to_string(),
                }
                .into(),
            )
        })
        .await
        .expect_err("the closure failed");

    assert!(matches!(
        err,
        crate::error::HaexError::InvalidInput { reason } if reason == "stop"
    ));
    assert!(pref_hlcs(&vault_db).is_empty(), "nothing was written");
}

#[tokio::test]
async fn a_commit_wakes_the_sync_service_and_a_rollback_does_not() {
    use futures::FutureExt;

    let (_tmp, db) = open_throwaway();
    let gate = VaultGate::new();
    let notify = gate.sync_notify();
    let vault_db = gate.vault_db(db).expect("open gate");

    let _ = vault_db
        .write(|_| Err::<(), _>(haex_crdt::Error::consumer("rollback")))
        .await;
    assert!(
        notify.notified().now_or_never().is_none(),
        "a rolled back write wakes nobody"
    );

    vault_db
        .write(|tx| insert_pref(tx, "test.a"))
        .await
        .expect("write");
    assert!(
        notify.notified().now_or_never().is_some(),
        "the commit left a wake-up"
    );
}

#[tokio::test]
async fn a_read_rejects_writes() {
    let (_tmp, db) = open_throwaway();
    let vault_db = VaultGate::new().vault_db(db).expect("open gate");

    let result = vault_db
        .read(|r| {
            use crate::storage::query::Query;
            r.query_row(
                "INSERT INTO preferences (vault_device_uuid, key, value) \
                 VALUES ('00000000-0000-0000-0000-000000000000', 'test.a', 'v') RETURNING key",
                &[],
                |row| row.get::<_, String>(0),
            )
        })
        .await;

    assert!(result.is_err(), "the read-only view refuses an INSERT");
    assert!(pref_hlcs(&vault_db).is_empty());
}
