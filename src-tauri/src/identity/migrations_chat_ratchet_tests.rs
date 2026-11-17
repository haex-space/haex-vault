//! Migration tests for `0063_chat_ratchet` (spec 095, data-model.md): a fresh vault and a vault from
//! the schema before end with the ratchet tables, none of them tracked by haex-crdt, and the outbox
//! keeps its rows with the new columns at their defaults.

// These tests read `sqlite_master` and `pragma_*` directly and seed rows below the CRDT write path.
#![allow(clippy::disallowed_methods)]

use haex_crdt::rusqlite::params;
use haex_crdt::Database;

use super::migrations_chat_ratchet::DEVICE_TABLES;
use super::migrations_extensions_tests::has_hlc_column;
use super::migrations_tests::{column_names, migration_source_before, open, table_exists};
use crate::identity::holzi_migration_source;

fn assert_schema(db: &Database) {
    for table in DEVICE_TABLES {
        assert!(table_exists(db, table), "{table} exists");
        assert!(!has_hlc_column(db, table), "{table} stays on the device");
    }
    let outbox = column_names(db, "chat_outbox_no_sync");
    for column in ["awaiting", "accepted_at", "resends"] {
        assert!(
            outbox.iter().any(|name| name == column),
            "chat_outbox_no_sync.{column}"
        );
    }
    assert!(!has_hlc_column(db, "chat_outbox_no_sync"));
    let sessions = column_names(db, "chat_ratchet_sessions_no_sync");
    for column in [
        "installation_uuid",
        "session",
        "peer_identity_key",
        "renewed_at",
    ] {
        assert!(
            sessions.iter().any(|name| name == column),
            "chat_ratchet_sessions_no_sync.{column}"
        );
    }
}

#[tokio::test]
async fn migration_0063_gives_a_fresh_vault_the_ratchet_tables() {
    tokio::task::spawn_blocking(|| {
        let dir = tempfile::tempdir().expect("tempdir");
        let db = open(dir.path(), "ratchet-fresh", true, holzi_migration_source());
        assert_schema(&db);
    })
    .await
    .expect("join");
}

#[tokio::test]
async fn migration_0063_keeps_the_outbox_rows() {
    tokio::task::spawn_blocking(|| {
        let dir = tempfile::tempdir().expect("tempdir");
        let before = migration_source_before("0063_chat_ratchet");
        let db = open(dir.path(), "ratchet-upgrade", true, before);
        for table in DEVICE_TABLES {
            assert!(!table_exists(&db, table), "{table} is not there yet");
        }
        db.with_connection(|conn| {
            conn.execute(
                "INSERT INTO chat_outbox_no_sync (id, thread_id, message_id, rumor, pending, \
                 attempts, next_at, created_at) VALUES ('o1', 't1', 'm1', '{}', '[\"aa\"]', 2, 5, 1)",
                [],
            )?;
            Ok(())
        })
        .expect("seed");
        drop(db);

        let db = open(dir.path(), "ratchet-upgrade", false, holzi_migration_source());
        assert_schema(&db);
        let row: (String, i64, String, Option<i64>, i64) = db
            .with_connection(|conn| {
                Ok(conn.query_row(
                    "SELECT pending, attempts, awaiting, accepted_at, resends \
                     FROM chat_outbox_no_sync WHERE id = ?1",
                    params!["o1"],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?)),
                )?)
            })
            .expect("read outbox");
        assert_eq!(row, ("[\"aa\"]".to_owned(), 2, "[]".to_owned(), None, 0));
    })
    .await
    .expect("join");
}
