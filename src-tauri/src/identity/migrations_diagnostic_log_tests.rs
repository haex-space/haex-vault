//! Migration tests for `0068_diagnostic_log` (spec 098 T004): the level table exists in a new vault
//! and in one from before the migration, and a write to it is no sync change.

// These tests read `pragma_*` directly and seed rows below the CRDT write path.
#![allow(clippy::disallowed_methods)]

use haex_crdt::rusqlite::params;

use super::migrations_extensions_tests::has_hlc_column;
use super::migrations_tests::{column_names, migration_source_before, open, table_exists};
use crate::identity::haex_migration_source;

const TABLE: &str = "haex_diagnostic_log_no_sync";

#[tokio::test]
async fn the_level_table_exists_in_a_new_vault() {
    tokio::task::spawn_blocking(|| {
        let dir = tempfile::tempdir().expect("tempdir");
        let db = open(dir.path(), "diagnostic-log", true, haex_migration_source());
        assert!(table_exists(&db, TABLE));
        assert_eq!(
            column_names(&db, TABLE),
            ["id", "level", "until_ms", "run_id"]
        );
        assert!(!has_hlc_column(&db, TABLE), "the table is not tracked");
    })
    .await
    .expect("join");
}

#[tokio::test]
async fn a_vault_from_before_the_migration_gets_the_table_and_keeps_its_data() {
    tokio::task::spawn_blocking(|| {
        let dir = tempfile::tempdir().expect("tempdir");
        let before = migration_source_before("0068_diagnostic_log");
        let db = open(dir.path(), "diagnostic-log", true, before);
        assert!(!table_exists(&db, TABLE));
        drop(db);

        let db = open(dir.path(), "diagnostic-log", false, haex_migration_source());
        assert!(table_exists(&db, TABLE));
    })
    .await
    .expect("join");
}

#[tokio::test]
async fn a_write_to_the_table_creates_no_sync_change() {
    tokio::task::spawn_blocking(|| {
        let dir = tempfile::tempdir().expect("tempdir");
        let db = open(dir.path(), "diagnostic-log", true, haex_migration_source());
        let dirty = |db: &haex_crdt::Database| -> i64 {
            db.with_connection(|conn| {
                Ok(conn.query_row(
                    "SELECT COUNT(*) FROM haex_crdt_dirty_tables_no_sync",
                    [],
                    |r| r.get(0),
                )?)
            })
            .expect("count")
        };
        let before = dirty(&db);
        db.with_connection(|conn| {
            conn.execute(
                "INSERT INTO haex_diagnostic_log_no_sync (id, level, until_ms, run_id) \
                 VALUES (1, 'verbose', ?1, 'run')",
                params![1_i64],
            )?;
            Ok(())
        })
        .expect("write");
        assert_eq!(dirty(&db), before);
        let triggers: i64 = db
            .with_connection(|conn| {
                Ok(conn.query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type = 'trigger' AND tbl_name = ?1",
                    params![TABLE],
                    |r| r.get(0),
                )?)
            })
            .expect("triggers");
        assert_eq!(triggers, 0, "no trigger tracks the table");
    })
    .await
    .expect("join");
}
