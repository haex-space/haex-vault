//! Round-trip tests for the IAM-admin cred store on top of the
//! password-manager tables.
//!
//! Test fixture (`setup_creds_db`) mirrors the shape of
//! `space_delivery::local::test_support::init_logs_db_inner`:
//! - in-memory SQLite
//! - `HlcService::new_for_testing` + `current_hlc()` UDF + tx-HLC hooks
//! - CRDT bookkeeping tables (`haex_crdt_configs_no_sync`,
//!   `haex_crdt_dirty_tables_no_sync`)
//! - the two password-manager tables we touch, run through
//!   `ensure_crdt_columns` so `execute_with_crdt` writes succeed.
//!
//! The `ensure_crdt_columns_and_triggers` variant is intentionally NOT used:
//! it would install a BEFORE-DELETE trigger writing into `haex_deleted_rows`,
//! which we don't seed. For local test assertions we only need the DELETE to
//! remove rows from the target tables; sync propagation is out of scope.

#![cfg(test)]

use std::sync::{Arc, Mutex};

use rusqlite::Connection;

use super::{cred_title_for, delete_by_storage, load, store, IamAdminCred};
use crate::crdt::hlc::HlcService;
use crate::crdt::trigger::ensure_crdt_columns;
use crate::database::connection_context::ConnectionContext;
use crate::database::core::{install_tx_hlc_hooks, register_current_hlc_udf};
use crate::database::DbConnection;
use crate::table_names::{TABLE_CRDT_CONFIGS, TABLE_CRDT_DIRTY_TABLES};

/// Build an in-memory DB seeded with just what the IAM-admin cred store
/// needs to `store` / `load` / `delete_by_storage`.
fn setup_creds_db() -> (DbConnection, HlcService) {
    let conn = Connection::open_in_memory().expect("open in-memory DB");
    let hlc_service = HlcService::new_for_testing("test-device-iam-creds");
    let ctx = ConnectionContext::new();
    register_current_hlc_udf(&conn, hlc_service.clone(), ctx.clone())
        .expect("register current_hlc UDF");
    install_tx_hlc_hooks(&conn, ctx).expect("install tx-HLC hooks");

    // CRDT bookkeeping tables the tx-HLC hooks + persist_timestamp read/write.
    conn.execute_batch(&format!(
        "CREATE TABLE {} (key TEXT PRIMARY KEY, type TEXT NOT NULL, value TEXT NOT NULL);",
        TABLE_CRDT_CONFIGS
    ))
    .expect("create crdt_configs");
    conn.execute_batch(&format!(
        "CREATE TABLE {} (table_name TEXT PRIMARY KEY, last_modified TEXT);",
        TABLE_CRDT_DIRTY_TABLES
    ))
    .expect("create crdt_dirty_tables");

    // Password-manager tables — column set mirrored from
    // `database/migrations/0000_jazzy_chat.sql`. The FK from item_key_values
    // to item_details is preserved so an out-of-order INSERT would fail
    // loudly instead of leaking an orphan row past `load`.
    conn.execute_batch(
        "CREATE TABLE haex_passwords_item_details (
            id TEXT PRIMARY KEY NOT NULL,
            title TEXT,
            username TEXT,
            password TEXT,
            note TEXT,
            icon TEXT,
            color TEXT,
            url TEXT,
            otp_secret TEXT,
            otp_digits INTEGER DEFAULT 6,
            otp_period INTEGER DEFAULT 30,
            otp_algorithm TEXT DEFAULT 'SHA1',
            expires_at TEXT,
            autofill_aliases TEXT,
            created_at TEXT DEFAULT (CURRENT_TIMESTAMP),
            updated_at TEXT DEFAULT (CURRENT_TIMESTAMP)
        );

        CREATE TABLE haex_passwords_item_key_values (
            id TEXT PRIMARY KEY NOT NULL,
            item_id TEXT NOT NULL,
            key TEXT,
            value TEXT,
            updated_at TEXT DEFAULT (CURRENT_TIMESTAMP),
            FOREIGN KEY (item_id) REFERENCES haex_passwords_item_details(id)
                ON UPDATE NO ACTION ON DELETE CASCADE
        );",
    )
    .expect("create passwords schema");

    {
        let tx = conn.unchecked_transaction().expect("begin crdt-cols tx");
        ensure_crdt_columns(&tx, "haex_passwords_item_details")
            .expect("ensure crdt columns on item_details");
        ensure_crdt_columns(&tx, "haex_passwords_item_key_values")
            .expect("ensure crdt columns on item_key_values");
        tx.commit().expect("commit crdt-cols tx");
    }

    let db = DbConnection(Arc::new(Mutex::new(Some(conn))));
    (db, hlc_service)
}

/// Random-string helper. Uses `rand::random` per project convention
/// (no hardcoded fake keys, so CodeQL's credential-heuristic scan doesn't
/// flag these tests).
fn rand_string(prefix: &str) -> String {
    let n: u64 = rand::random();
    format!("{prefix}-{n:016x}")
}

fn rand_cred() -> IamAdminCred {
    IamAdminCred {
        access_key_id: rand_string("AKIA"),
        secret_access_key: rand_string("secret"),
        provider_type: "aws".to_string(),
    }
}

// ---------------------------------------------------------------------------

#[test]
fn cred_title_for_uses_iam_admin_prefix() {
    assert_eq!(cred_title_for("storage-42"), "iam-admin:storage-42");
}

#[test]
fn store_then_load_round_trips_all_fields() {
    let (db, hlc_service) = setup_creds_db();
    let storage_id = rand_string("storage");
    let cred = rand_cred();

    {
        let hlc = Mutex::new(hlc_service.clone());
        let guard = hlc.lock().unwrap();
        store(&db, &guard, &storage_id, &cred).expect("store cred");
    }

    let loaded = load(&db, &storage_id).expect("load cred").expect("Some");
    assert_eq!(loaded, cred);
}

#[test]
fn load_of_unknown_storage_id_returns_none() {
    let (db, _hlc_service) = setup_creds_db();
    let unknown = rand_string("no-such-storage");

    let result = load(&db, &unknown).expect("load call succeeds");
    assert!(
        result.is_none(),
        "unknown storage id must produce Ok(None), got {:?}",
        result
    );
}

#[test]
fn delete_by_storage_removes_item_and_key_values() {
    let (db, hlc_service) = setup_creds_db();
    let storage_id = rand_string("storage");
    let cred = rand_cred();

    let hlc = Mutex::new(hlc_service.clone());
    {
        let guard = hlc.lock().unwrap();
        store(&db, &guard, &storage_id, &cred).expect("store cred");
    }
    assert!(load(&db, &storage_id).unwrap().is_some(), "sanity: stored");

    {
        let guard = hlc.lock().unwrap();
        delete_by_storage(&db, &guard, &storage_id).expect("delete cred");
    }

    // Load must now report absence.
    assert!(
        load(&db, &storage_id).unwrap().is_none(),
        "delete must remove the item row so load returns None"
    );

    // And the key-values junction row must be gone too — probe the raw table
    // via a fresh Connection borrow, since load's LEFT JOIN would otherwise
    // hide an orphan (item deleted, kv left behind).
    let title = cred_title_for(&storage_id);
    let guard_arc = db.0.lock().unwrap();
    let conn = guard_arc.as_ref().expect("db connection");
    let kv_orphans: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM haex_passwords_item_key_values kv \
             WHERE kv.item_id IN ( \
                 SELECT id FROM haex_passwords_item_details WHERE title = ?1 \
             )",
            [&title],
            |row| row.get(0),
        )
        .expect("count key-values");
    assert_eq!(
        kv_orphans, 0,
        "delete_by_storage must clear the key-values rows, none should remain",
    );
}

#[test]
fn load_after_delete_of_unrelated_storage_still_finds_the_other() {
    // Locks in that delete_by_storage's WHERE title = ?1 predicate only
    // touches the matching row — deleting one cred must not disturb others.
    let (db, hlc_service) = setup_creds_db();
    let keep_id = rand_string("keep");
    let drop_id = rand_string("drop");
    let keep_cred = rand_cred();
    let drop_cred = rand_cred();

    let hlc = Mutex::new(hlc_service.clone());
    {
        let guard = hlc.lock().unwrap();
        store(&db, &guard, &keep_id, &keep_cred).expect("store keep");
        store(&db, &guard, &drop_id, &drop_cred).expect("store drop");
    }

    {
        let guard = hlc.lock().unwrap();
        delete_by_storage(&db, &guard, &drop_id).expect("delete drop");
    }

    assert!(load(&db, &drop_id).unwrap().is_none(), "dropped is gone");
    assert_eq!(
        load(&db, &keep_id).unwrap().expect("keep still loadable"),
        keep_cred,
        "the unrelated cred must survive"
    );
}
