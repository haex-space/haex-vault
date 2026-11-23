//! Migration tests for `0069_agent_memory` (spec 097, data-model.md, T002): a fresh vault and a vault
//! from the schema before end with `haex_agent_memory_pages` and the two new columns of
//! `chat_threads`; the table is tracked by haex-crdt, carries neither a UNIQUE constraint nor a
//! foreign key, and a write and a delete reach the sync.

// These tests read `sqlite_master` and `pragma_*` directly, which the CRDT write path does not
// expose.
#![allow(clippy::disallowed_methods)]

use haex_crdt::rusqlite::params;
use haex_crdt::Database;

use super::migrations_extensions_tests::{has_hlc_column, unique_indexes};
use super::migrations_tests::{column_names, migration_source_before, open, table_exists};
use crate::identity::haex_migration_source;

const TABLE: &str = "haex_agent_memory_pages";

/// The columns of data-model.md, without the columns haex-crdt adds.
const COLUMNS: [&str; 14] = [
    "id",
    "title",
    "description",
    "body",
    "author_kind",
    "author_key",
    "author_name",
    "author_device",
    "created_at",
    "changed_kind",
    "changed_key",
    "changed_name",
    "changed_device",
    "changed_at",
];

fn foreign_keys(db: &Database, table: &str) -> i64 {
    db.with_connection(|conn| {
        Ok(conn.query_row(
            "SELECT COUNT(*) FROM pragma_foreign_key_list(?1)",
            params![table],
            |row| row.get(0),
        )?)
    })
    .expect("read")
}

fn assert_schema(db: &Database) {
    assert!(table_exists(db, TABLE), "{TABLE} exists");
    assert!(has_hlc_column(db, TABLE), "{TABLE} is tracked by haex-crdt");
    assert_eq!(
        unique_indexes(db, TABLE),
        0,
        "a UNIQUE constraint would halt the sync"
    );
    assert_eq!(foreign_keys(db, TABLE), 0, "{TABLE} has no foreign key");
    let found = column_names(db, TABLE);
    for column in COLUMNS {
        assert!(found.iter().any(|c| c == column), "{TABLE}.{column}");
    }
    let threads = column_names(db, "chat_threads");
    for column in ["memory_toc", "memory_foreign_at"] {
        assert!(threads.iter().any(|c| c == column), "chat_threads.{column}");
    }
    assert!(
        has_hlc_column(db, "chat_threads"),
        "chat_threads stays tracked by haex-crdt"
    );
}

#[tokio::test]
async fn migration_0069_gives_a_fresh_vault_the_memory() {
    tokio::task::spawn_blocking(|| {
        let dir = tempfile::tempdir().expect("tempdir");
        let db = open(dir.path(), "memory-fresh", true, haex_migration_source());
        assert_schema(&db);
    })
    .await
    .expect("join");
}

#[tokio::test]
async fn migration_0069_upgrades_a_vault_and_keeps_its_threads() {
    tokio::task::spawn_blocking(|| {
        let dir = tempfile::tempdir().expect("tempdir");
        let old = open(
            dir.path(),
            "memory-upgrade",
            true,
            migration_source_before("0069_agent_memory"),
        );
        assert!(!table_exists(&old, TABLE));
        old.write(|tx| {
            tx.execute(
                "INSERT INTO chat_threads (id, title, created_at, updated_at) \
                 VALUES ('t1', 'Alt', 1, 1)",
                params![],
            )?;
            Ok(())
        })
        .expect("seed a thread");
        drop(old);

        let db = open(dir.path(), "memory-upgrade", false, haex_migration_source());
        assert_schema(&db);
        let (title, toc, foreign): (String, Option<String>, Option<i64>) = db
            .with_connection(|conn| {
                Ok(conn.query_row(
                    "SELECT title, memory_toc, memory_foreign_at FROM chat_threads WHERE id = 't1'",
                    [],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                )?)
            })
            .expect("read the thread");
        assert_eq!((title.as_str(), toc, foreign), ("Alt", None, None));
    })
    .await
    .expect("join");
}

#[tokio::test]
async fn a_page_write_and_delete_reach_the_sync() {
    tokio::task::spawn_blocking(|| {
        let dir = tempfile::tempdir().expect("tempdir");
        let db = open(dir.path(), "memory-tracked", true, haex_migration_source());
        db.write(|tx| {
            tx.execute(
                "INSERT INTO haex_agent_memory_pages \
                 (id, title, description, body, author_kind, author_name, author_device, created_at) \
                 VALUES ('p', 'Bernd Krause', 'Steuerberater', 'Text', 'user', '', 'd', 1)",
                params![],
            )?;
            Ok(())
        })
        .expect("write a page");
        let tracked: i64 = db
            .with_connection(|conn| {
                Ok(conn.query_row(
                    "SELECT COUNT(*) FROM haex_agent_memory_pages WHERE haex_hlc_no_sync IS NOT NULL",
                    [],
                    |row| row.get(0),
                )?)
            })
            .expect("read");
        assert_eq!(tracked, 1, "the write is stamped for the sync");

        db.write(|tx| {
            tx.execute("DELETE FROM haex_agent_memory_pages WHERE id = 'p'", params![])?;
            Ok(())
        })
        .expect("delete the page");
        let markers: i64 = db
            .with_connection(|conn| {
                Ok(conn.query_row(
                    "SELECT COUNT(*) FROM haex_deleted_rows WHERE table_name = ?1",
                    params![TABLE],
                    |row| row.get(0),
                )?)
            })
            .expect("read markers");
        assert!(markers >= 1, "a delete leaves a marker");
    })
    .await
    .expect("join");
}
