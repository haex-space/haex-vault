//! Migration tests for `0051_assistant_tones` (spec 066 FR-045a, T156): a fresh vault and a vault
//! from the schema before end with `haex_assistant_tones`, tracked by haex-crdt without a UNIQUE
//! constraint.

// These tests read `sqlite_master` and `pragma_*` directly.
#![allow(clippy::disallowed_methods)]

use haex_crdt::Database;

use super::migrations_extensions_tests::{has_hlc_column, unique_indexes};
use super::migrations_tests::{column_names, migration_source_before, open, table_exists};
use crate::identity::holzi_migration_source;

const TONES: &str = "haex_assistant_tones";

fn assert_schema(db: &Database) {
    assert!(table_exists(db, TONES), "{TONES} exists");
    assert!(has_hlc_column(db, TONES), "{TONES} is tracked by haex-crdt");
    assert_eq!(
        unique_indexes(db, TONES),
        0,
        "a UNIQUE conflict would halt the sync"
    );
    let columns = column_names(db, TONES);
    for column in [
        "id",
        "name",
        "duration_ms",
        "sample_rate",
        "sha256",
        "sound",
        "created_at",
    ] {
        assert!(
            columns.iter().any(|name| name == column),
            "{TONES}.{column}"
        );
    }
}

#[tokio::test]
async fn migration_0051_gives_a_fresh_vault_the_tone_table() {
    tokio::task::spawn_blocking(|| {
        let dir = tempfile::tempdir().expect("tempdir");
        let db = open(dir.path(), "tones-fresh", true, holzi_migration_source());
        assert_schema(&db);
    })
    .await
    .expect("join");
}

#[tokio::test]
async fn migration_0051_adds_the_tone_table_to_an_existing_vault() {
    tokio::task::spawn_blocking(|| {
        let dir = tempfile::tempdir().expect("tempdir");
        let before = migration_source_before("0051_assistant_tones");
        let db = open(dir.path(), "tones-upgrade", true, before);
        assert!(!table_exists(&db, TONES), "{TONES} is not there yet");
        drop(db);
        let db = open(dir.path(), "tones-upgrade", false, holzi_migration_source());
        assert_schema(&db);
    })
    .await
    .expect("join");
}
