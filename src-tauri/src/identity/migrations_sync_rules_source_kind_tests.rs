//! Migration tests for `0060_sync_rules_source_kind` (spec 074): a vault from before it keeps every row of
//! the five sync rule tables, each rule is a folder rule, and the migration runs once; a fresh vault has the
//! column with its default; the Rust reader reads the kind a rule has.

// These tests read `sqlite_master` and write rows directly, which the CRDT write path does not expose.
#![allow(clippy::disallowed_methods)]

use haex_crdt::rusqlite::params;
use haex_crdt::Database;

use super::migrations_sync_rules::TABLES;
use super::migrations_sync_rules_tests::fill;
use super::migrations_tests::{column_names, device_of, migration_source_before, open};
use crate::file_sync::model::SyncRuleSourceKind;
use crate::file_sync::store;
use crate::identity::holzi_migration_source;

const PASSPHRASE: &str = "sync-rules-source-kind";
const MIGRATION: &str = "0060_sync_rules_source_kind";

fn count(db: &Database, table: &str) -> i64 {
    db.with_connection(|conn| {
        Ok(conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))?)
    })
    .expect("count")
}

/// The stored kind of every rule, in the order SQLite stores them.
fn kinds(db: &Database) -> Vec<String> {
    db.with_connection(|conn| {
        let mut statement =
            conn.prepare("SELECT source_kind FROM haex_sync_rules_no_sync ORDER BY rowid")?;
        let rows = statement.query_map([], |r| r.get::<_, String>(0))?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    })
    .expect("kinds")
}

/// The rules of `device` as the Rust reader finds them.
fn rules(db: &Database, device: &str) -> Vec<crate::file_sync::model::SyncRule> {
    crate::storage::query::read(db, |r| Ok(store::rules(r, device)))
        .expect("read")
        .expect("rules")
}

#[tokio::test]
async fn migration_0060_keeps_every_row_and_makes_each_rule_a_folder_rule() {
    tokio::task::spawn_blocking(|| {
        let dir = tempfile::tempdir().expect("tempdir");
        let old = open(
            dir.path(),
            PASSPHRASE,
            true,
            migration_source_before(MIGRATION),
        );
        let device = device_of(&old, dir.path()).to_string();
        fill(&old, &device);
        let counts: Vec<i64> = TABLES.iter().map(|table| count(&old, table)).collect();
        assert!(
            !column_names(&old, "haex_sync_rules_no_sync")
                .iter()
                .any(|c| c == "source_kind"),
            "the old schema has no kind"
        );
        drop(old);

        let db = open(dir.path(), PASSPHRASE, false, holzi_migration_source());
        let after: Vec<i64> = TABLES.iter().map(|table| count(&db, table)).collect();
        assert_eq!(after, counts, "no table lost or gained a row");
        assert_eq!(kinds(&db), ["folder"], "an old rule is a folder rule");

        let read = rules(&db, &device);
        assert_eq!(read.len(), 1, "the rule is still read");
        assert_eq!(read[0].source_kind, SyncRuleSourceKind::Folder);
        assert_eq!(read[0].name, "Fotos");
        assert_eq!(read[0].source.path, "/a");
        assert_eq!(read[0].target.path, "/b");
        drop(db);

        // The migration runs once: opening the migrated vault again changes nothing.
        let again = open(dir.path(), PASSPHRASE, false, holzi_migration_source());
        let again_counts: Vec<i64> = TABLES.iter().map(|table| count(&again, table)).collect();
        assert_eq!(again_counts, counts);
        assert_eq!(kinds(&again), ["folder"]);
    })
    .await
    .expect("join");
}

#[tokio::test]
async fn migration_0060_gives_a_fresh_vault_the_column_with_its_default() {
    tokio::task::spawn_blocking(|| {
        let dir = tempfile::tempdir().expect("tempdir");
        let db = open(
            dir.path(),
            "sync-rules-source-kind-fresh",
            true,
            holzi_migration_source(),
        );
        assert!(
            column_names(&db, "haex_sync_rules_no_sync")
                .iter()
                .any(|c| c == "source_kind"),
            "source_kind exists"
        );
        let device = device_of(&db, dir.path()).to_string();
        // A row written without the column gets the default.
        db.with_connection(|conn| {
            conn.execute(
                "INSERT INTO haex_sync_rules_no_sync (id, vault_device_uuid, name, enabled, \
                 source_location, source_path, target_location, target_path, direction, \
                 delete_mode, interval_secs, created_at, updated_at) \
                 VALUES ('r1', ?1, 'Fotos', 1, 'device', '/a', 'device', '/b', 'sourceToTarget', \
                 'keep', 900, '2026-11-09T00:00:00Z', '2026-11-09T00:00:00Z')",
                params![device],
            )?;
            Ok(())
        })
        .expect("insert");
        assert_eq!(kinds(&db), ["folder"]);
    })
    .await
    .expect("join");
}

#[tokio::test]
async fn the_reader_reads_the_kind_a_rule_has_and_skips_a_kind_it_does_not_know() {
    tokio::task::spawn_blocking(|| {
        let dir = tempfile::tempdir().expect("tempdir");
        let db = open(
            dir.path(),
            "sync-rules-source-kind-reader",
            true,
            holzi_migration_source(),
        );
        let device = device_of(&db, dir.path()).to_string();
        fill(&db, &device);
        assert_eq!(
            rules(&db, &device)[0].source_kind,
            SyncRuleSourceKind::Folder
        );

        db.with_connection(|conn| {
            conn.execute(
                "UPDATE haex_sync_rules_no_sync SET source_kind = 'file'",
                [],
            )?;
            Ok(())
        })
        .expect("set file");
        assert_eq!(rules(&db, &device)[0].source_kind, SyncRuleSourceKind::File);

        db.with_connection(|conn| {
            conn.execute(
                "UPDATE haex_sync_rules_no_sync SET source_kind = 'something-newer'",
                [],
            )?;
            Ok(())
        })
        .expect("set unknown");
        assert!(
            rules(&db, &device).is_empty(),
            "a rule with a kind this version does not know is skipped, like an unknown direction"
        );
    })
    .await
    .expect("join");
}
