//! Migration tests for `0049_chat_contacts` (spec 072, data-model.md): a fresh vault and a vault from
//! the schema before end with the new tables and `chat_threads` columns; synced tables are tracked by
//! haex-crdt without a UNIQUE constraint, device tables are not tracked, and every agent conversation
//! from before keeps its messages and becomes `kind = 'agent'`, `state = 'active'` (SC-001).

// These tests read `sqlite_master` and `pragma_*` directly and seed rows below the CRDT write path.
#![allow(clippy::disallowed_methods)]

use haex_crdt::rusqlite::params;
use haex_crdt::Database;

use super::migrations_chat_contacts::{DEVICE_TABLES, SYNCED_TABLES, THREAD_COLUMNS};
use super::migrations_extensions_tests::{has_hlc_column, unique_indexes};
use super::migrations_tests::{column_names, migration_source_before, open, table_exists};
use crate::identity::holzi_migration_source;

fn assert_schema(db: &Database) {
    for table in SYNCED_TABLES {
        assert!(table_exists(db, table), "{table} exists");
        assert!(has_hlc_column(db, table), "{table} is tracked by haex-crdt");
        assert_eq!(
            unique_indexes(db, table),
            0,
            "{table}: a UNIQUE constraint would halt the sync"
        );
    }
    for table in DEVICE_TABLES {
        assert!(table_exists(db, table), "{table} exists");
        assert!(!has_hlc_column(db, table), "{table} stays on the device");
    }
    let columns = column_names(db, "chat_threads");
    for column in THREAD_COLUMNS {
        assert!(
            columns.iter().any(|name| name == column),
            "chat_threads.{column}"
        );
    }
}

#[tokio::test]
async fn migration_0049_gives_a_fresh_vault_the_chat_tables() {
    tokio::task::spawn_blocking(|| {
        let dir = tempfile::tempdir().expect("tempdir");
        let db = open(dir.path(), "chat-fresh", true, holzi_migration_source());
        assert_schema(&db);
    })
    .await
    .expect("join");
}

#[tokio::test]
async fn migration_0049_keeps_agent_conversations_and_marks_them_agent() {
    tokio::task::spawn_blocking(|| {
        let dir = tempfile::tempdir().expect("tempdir");
        let before = migration_source_before("0049_chat_contacts");
        let db = open(dir.path(), "chat-upgrade", true, before);
        for table in SYNCED_TABLES.iter().chain(DEVICE_TABLES.iter()) {
            assert!(!table_exists(&db, table), "{table} is not there yet");
        }
        db.with_connection(|conn| {
            conn.execute(
                "INSERT INTO chat_threads (id, title, created_at, updated_at) \
                 VALUES ('t1', 'Rezept', 1, 2)",
                [],
            )?;
            conn.execute(
                "INSERT INTO chat_messages (id, thread_id, role, content, created_at) \
                 VALUES ('m1', 't1', 'user', 'Wie lange kocht ein Ei?', 1)",
                [],
            )?;
            Ok(())
        })
        .expect("seed");
        drop(db);

        let db = open(dir.path(), "chat-upgrade", false, holzi_migration_source());
        assert_schema(&db);
        let (kind, state, muted, peer): (String, String, i64, Option<String>) = db
            .with_connection(|conn| {
                Ok(conn.query_row(
                    "SELECT kind, state, muted, peer_vault FROM chat_threads WHERE id = ?1",
                    params!["t1"],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
                )?)
            })
            .expect("read thread");
        assert_eq!(kind, "agent");
        assert_eq!(state, "active");
        assert_eq!(muted, 0);
        assert_eq!(peer, None);
        let content: String = db
            .with_connection(|conn| {
                Ok(conn.query_row(
                    "SELECT content FROM chat_messages WHERE id = 'm1'",
                    [],
                    |row| row.get(0),
                )?)
            })
            .expect("read message");
        assert_eq!(content, "Wie lange kocht ein Ei?");
    })
    .await
    .expect("join");
}
