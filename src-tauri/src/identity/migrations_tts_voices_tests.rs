//! Migration tests for `0052_tts_voices` (spec 066 FR-049, FR-050, T141): a fresh vault and a vault
//! from the schema before end with `haex_tts_voices` and `haex_tts_voices_binaries` as
//! data-model.md has them, both tracked by haex-crdt without a UNIQUE constraint beyond their
//! primary keys. Imported models (`haex_tts_models_no_sync`) belong to PR K part 2.

// These tests read `sqlite_master` and `pragma_*` directly.
#![allow(clippy::disallowed_methods)]

use haex_crdt::Database;

use super::migrations_extensions_tests::{has_hlc_column, unique_indexes};
use super::migrations_tests::{column_names, migration_source_before, open, table_exists};
use crate::identity::holzi_migration_source;

const VOICES: &str = "haex_tts_voices";
const BINARIES: &str = "haex_tts_voices_binaries";

fn assert_schema(db: &Database) {
    for (table, columns) in [
        (
            VOICES,
            &[
                "id",
                "name",
                "language",
                "engine",
                "reference_hash",
                "sample_rate",
                "consent_confirmed_at",
                "created_at",
            ][..],
        ),
        (BINARIES, &["hash", "data", "orphaned_at"][..]),
    ] {
        assert!(table_exists(db, table), "{table} exists");
        assert!(has_hlc_column(db, table), "{table} is tracked by haex-crdt");
        assert_eq!(
            unique_indexes(db, table),
            0,
            "a UNIQUE conflict would halt the sync"
        );
        let found = column_names(db, table);
        for column in columns {
            assert!(found.iter().any(|name| name == column), "{table}.{column}");
        }
    }
    assert!(
        !table_exists(db, "haex_tts_models_no_sync"),
        "imported models come with PR K part 2"
    );
}

#[tokio::test]
async fn migration_0052_gives_a_fresh_vault_the_voice_tables() {
    tokio::task::spawn_blocking(|| {
        let dir = tempfile::tempdir().expect("tempdir");
        let db = open(dir.path(), "voices-fresh", true, holzi_migration_source());
        assert_schema(&db);
    })
    .await
    .expect("join");
}

#[tokio::test]
async fn migration_0052_adds_the_voice_tables_to_an_existing_vault() {
    tokio::task::spawn_blocking(|| {
        let dir = tempfile::tempdir().expect("tempdir");
        let before = migration_source_before("0052_tts_voices");
        let db = open(dir.path(), "voices-upgrade", true, before);
        assert!(!table_exists(&db, VOICES), "{VOICES} is not there yet");
        drop(db);
        let db = open(
            dir.path(),
            "voices-upgrade",
            false,
            holzi_migration_source(),
        );
        assert_schema(&db);
    })
    .await
    .expect("join");
}
