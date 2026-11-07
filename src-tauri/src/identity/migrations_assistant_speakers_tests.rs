//! Migration test for `0058_assistant_speakers` (spec 066 FR-016a, T174): a fresh vault and a
//! vault from the schema before end with `haex_assistant_speakers`, tracked by haex-crdt without a
//! UNIQUE constraint beyond its primary key.

// These tests read `sqlite_master` and `pragma_*` directly.
#![allow(clippy::disallowed_methods)]

use haex_crdt::Database;

use super::migrations_extensions_tests::{has_hlc_column, unique_indexes};
use super::migrations_tests::{column_names, migration_source_before, open, table_exists};
use crate::identity::holzi_migration_source;

const SPEAKERS: &str = "haex_assistant_speakers";

fn assert_schema(db: &Database) {
    assert!(table_exists(db, SPEAKERS), "{SPEAKERS} exists");
    assert!(has_hlc_column(db, SPEAKERS), "tracked by haex-crdt");
    assert_eq!(
        unique_indexes(db, SPEAKERS),
        0,
        "a UNIQUE conflict would halt the sync"
    );
    let found = column_names(db, SPEAKERS);
    for column in [
        "id",
        "name",
        "model",
        "embedding",
        "utterances",
        "created_at",
    ] {
        assert!(
            found.iter().any(|name| name == column),
            "{SPEAKERS}.{column}"
        );
    }
}

#[tokio::test]
async fn migration_0058_gives_a_fresh_vault_the_speakers_table() {
    tokio::task::spawn_blocking(|| {
        let dir = tempfile::tempdir().expect("tempdir");
        let db = open(dir.path(), "speakers-fresh", true, holzi_migration_source());
        assert_schema(&db);
    })
    .await
    .expect("join");
}

#[tokio::test]
async fn migration_0058_adds_the_table_to_an_existing_vault() {
    tokio::task::spawn_blocking(|| {
        let dir = tempfile::tempdir().expect("tempdir");
        let before = migration_source_before("0058_assistant_speakers");
        let db = open(dir.path(), "speakers-upgrade", true, before);
        assert!(!table_exists(&db, SPEAKERS), "{SPEAKERS} is not there yet");
        drop(db);
        let db = open(
            dir.path(),
            "speakers-upgrade",
            false,
            holzi_migration_source(),
        );
        assert_schema(&db);
    })
    .await
    .expect("join");
}
