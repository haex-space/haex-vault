//! Migration tests for `0070_chat_gate` (spec 099, data-model.md): a fresh vault and a vault from the
//! schema before end with the new columns, old rows keep NULL in them, and the notices table stays on
//! the device.

// These tests read `sqlite_master` and `pragma_*` directly and seed rows below the CRDT write path.
#![allow(clippy::disallowed_methods)]

use haex_crdt::rusqlite::params;
use haex_crdt::Database;

use super::migrations_extensions_tests::has_hlc_column;
use super::migrations_tests::{column_names, migration_source_before, open, table_exists};
use crate::identity::haex_migration_source;

const THREAD_COLUMNS: [&str; 3] = ["request_first_at", "request_last_at", "peer_blocked_at"];

fn assert_schema(db: &Database) {
    let threads = column_names(db, "chat_threads");
    for column in THREAD_COLUMNS {
        assert!(
            threads.iter().any(|name| name == column),
            "chat_threads.{column}"
        );
    }
    assert!(column_names(db, "haex_contacts_blocked_vaults")
        .iter()
        .any(|name| name == "card"));
    assert!(
        has_hlc_column(db, "chat_threads"),
        "chat_threads stays tracked"
    );
    assert!(
        has_hlc_column(db, "haex_contacts_blocked_vaults"),
        "the block list stays tracked"
    );
    assert!(table_exists(db, "chat_gate_notices_no_sync"));
    assert!(table_exists(db, "chat_gate_answer_sequences_no_sync"));
    assert!(table_exists(db, "chat_gate_received_answers_no_sync"));
    assert!(column_names(db, "chat_gate_notices_no_sync")
        .iter()
        .any(|name| name == "sequence"));
    assert!(
        !has_hlc_column(db, "chat_gate_notices_no_sync"),
        "the notices stay on the device"
    );
}

#[tokio::test]
async fn migration_0070_gives_a_fresh_vault_the_gate_schema() {
    tokio::task::spawn_blocking(|| {
        let dir = tempfile::tempdir().expect("tempdir");
        let db = open(dir.path(), "gate-fresh", true, haex_migration_source());
        assert_schema(&db);
    })
    .await
    .expect("join");
}

#[tokio::test]
async fn migration_0070_leaves_old_rows_without_request_times_or_card() {
    tokio::task::spawn_blocking(|| {
        let dir = tempfile::tempdir().expect("tempdir");
        let before = migration_source_before("0070_chat_gate");
        let db = open(dir.path(), "gate-upgrade", true, before);
        db.with_connection(|conn| {
            conn.execute(
                "INSERT INTO chat_threads (id, title, created_at, updated_at) \
                 VALUES ('t1', '', 1, 1)",
                [],
            )?;
            conn.execute(
                "INSERT INTO haex_contacts_blocked_vaults (vault_pubkey, blocked_at) \
                 VALUES ('aa', '2026-11-25T00:00:00Z')",
                [],
            )?;
            Ok(())
        })
        .expect("seed");
        drop(db);

        let db = open(dir.path(), "gate-upgrade", false, haex_migration_source());
        assert_schema(&db);
        let (first, last, blocked): (Option<i64>, Option<i64>, Option<i64>) = db
            .with_connection(|conn| {
                Ok(conn.query_row(
                    "SELECT request_first_at, request_last_at, peer_blocked_at \
                     FROM chat_threads WHERE id = ?1",
                    params!["t1"],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                )?)
            })
            .expect("read thread");
        assert_eq!((first, last, blocked), (None, None, None));
        let card: Option<Vec<u8>> = db
            .with_connection(|conn| {
                Ok(conn.query_row(
                    "SELECT card FROM haex_contacts_blocked_vaults WHERE vault_pubkey = 'aa'",
                    [],
                    |row| row.get(0),
                )?)
            })
            .expect("read block entry");
        assert_eq!(card, None);
    })
    .await
    .expect("join");
}
