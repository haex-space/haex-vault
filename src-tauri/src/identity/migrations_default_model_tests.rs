//! Migration tests for `0062_extension_default_model`: a fresh vault and a vault from the schema
//! before it end with the default table, tracked by haex-crdt and without a UNIQUE constraint, and
//! without the rank table.

// These tests read `sqlite_master` and `pragma_*` directly, which the CRDT write path does not
// expose.
#![allow(clippy::disallowed_methods)]

use haex_crdt::Database;

use super::migrations_default_model::SYNCED_TABLES;
use super::migrations_extensions_tests::{has_hlc_column, unique_indexes};
use super::migrations_tests::{migration_source_before, open, own_columns, table_exists};
use crate::identity::holzi_migration_source;

fn assert_schema(db: &Database) {
    for table in SYNCED_TABLES {
        assert!(table_exists(db, table), "{table} exists");
        assert!(
            has_hlc_column(db, table),
            "{table} reaches every own device"
        );
        assert_eq!(
            unique_indexes(db, table),
            0,
            "{table}: a UNIQUE constraint would halt the sync"
        );
    }
    assert_eq!(
        own_columns(db, "extension_default_model"),
        ["id", "extension_id", "model_id"]
    );
    assert!(
        !table_exists(db, "extension_model_order"),
        "the rank is gone"
    );
}

#[tokio::test]
async fn migration_0062_gives_a_fresh_vault_the_default_table_and_no_rank() {
    tokio::task::spawn_blocking(|| {
        let dir = tempfile::tempdir().expect("tempdir");
        let db = open(
            dir.path(),
            "default-model-fresh",
            true,
            holzi_migration_source(),
        );
        assert_schema(&db);
    })
    .await
    .expect("join");
}

#[tokio::test]
async fn migration_0062_replaces_the_rank_table_of_an_older_vault() {
    tokio::task::spawn_blocking(|| {
        let dir = tempfile::tempdir().expect("tempdir");
        let old = open(
            dir.path(),
            "default-model-upgrade",
            true,
            migration_source_before("0062_extension_default_model"),
        );
        assert!(table_exists(&old, "extension_model_order"));
        assert!(!table_exists(&old, "extension_default_model"));
        drop(old);

        let db = open(
            dir.path(),
            "default-model-upgrade",
            false,
            holzi_migration_source(),
        );
        assert_schema(&db);
    })
    .await
    .expect("join");
}
