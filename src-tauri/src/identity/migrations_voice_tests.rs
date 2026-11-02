//! Migration tests for `0050_voice_conversations` (spec 066, data-model.md, T039, T124): a fresh
//! vault and a vault from the schema before end with `chat_threads.origin` and the transcript table;
//! the transcript is tracked by haex-crdt without a UNIQUE constraint, and every conversation from
//! before keeps its messages and counts as typed (`origin` NULL).

// These tests read `sqlite_master` and `pragma_*` directly and seed rows below the CRDT write path.
#![allow(clippy::disallowed_methods)]

use haex_crdt::rusqlite::params;
use haex_crdt::Database;

use super::migrations_extensions_tests::{has_hlc_column, unique_indexes};
use super::migrations_tests::{column_names, migration_source_before, open, table_exists};
use crate::identity::holzi_migration_source;

const TRANSCRIPT: &str = "voice_transcript_entries";

fn assert_schema(db: &Database) {
    assert!(table_exists(db, TRANSCRIPT), "{TRANSCRIPT} exists");
    assert!(
        has_hlc_column(db, TRANSCRIPT),
        "{TRANSCRIPT} is tracked by haex-crdt"
    );
    assert_eq!(
        unique_indexes(db, TRANSCRIPT),
        0,
        "equal seq values from two devices must not halt the sync"
    );
    let columns = column_names(db, TRANSCRIPT);
    for column in [
        "id",
        "thread_id",
        "seq",
        "spoken_at",
        "speaker",
        "kind",
        "text",
        "spoken_until",
        "outcome",
        "ref_id",
        "created_at",
    ] {
        assert!(
            columns.iter().any(|name| name == column),
            "{TRANSCRIPT}.{column}"
        );
    }
    assert!(
        column_names(db, "chat_threads")
            .iter()
            .any(|name| name == "origin"),
        "chat_threads.origin"
    );
}

#[tokio::test]
async fn migration_0050_gives_a_fresh_vault_origin_and_the_transcript() {
    tokio::task::spawn_blocking(|| {
        let dir = tempfile::tempdir().expect("tempdir");
        let db = open(dir.path(), "voice-fresh", true, holzi_migration_source());
        assert_schema(&db);
    })
    .await
    .expect("join");
}

#[tokio::test]
async fn migration_0050_keeps_conversations_as_typed() {
    tokio::task::spawn_blocking(|| {
        let dir = tempfile::tempdir().expect("tempdir");
        let before = migration_source_before("0050_voice_conversations");
        let db = open(dir.path(), "voice-upgrade", true, before);
        assert!(
            !table_exists(&db, TRANSCRIPT),
            "{TRANSCRIPT} is not there yet"
        );
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

        let db = open(dir.path(), "voice-upgrade", false, holzi_migration_source());
        assert_schema(&db);
        let (origin, content): (Option<String>, String) = db
            .with_connection(|conn| {
                Ok(conn.query_row(
                    "SELECT t.origin, m.content FROM chat_threads t \
                     JOIN chat_messages m ON m.thread_id = t.id WHERE t.id = ?1",
                    params!["t1"],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )?)
            })
            .expect("read thread");
        assert_eq!(origin, None);
        assert_eq!(content, "Wie lange kocht ein Ei?");
    })
    .await
    .expect("join");
}
