//! Migration tests for `0030_encrypted_folders` (spec 048, data-model.md): a genesis vault and a
//! vault upgraded from the schema before it both end with the two cache tables, untracked by
//! haex-crdt.

// These tests read `sqlite_master` and `pragma_*` directly, which the CRDT write path does not
// expose.
#![allow(clippy::disallowed_methods)]

use haex_crdt::Database;

use super::migrations_encrypted_folders::DEVICE_TABLES;
use super::migrations_extensions_tests::has_hlc_column;
use super::migrations_tests::{migration_source_before, open, own_columns, table_exists};
use crate::identity::holzi_migration_source;

const COLUMNS: [(&str, &[&str]); 2] = [
    (
        "encrypted_folder_heads_no_sync",
        &[
            "storage_id",
            "object_key",
            "etag",
            "folder_id",
            "name",
            "state",
            "key_id",
            "open_ack",
            "loaded_at",
        ],
    ),
    (
        "encrypted_folder_entries_no_sync",
        &[
            "storage_id",
            "folder_id",
            "sidecar",
            "etag",
            "entry_id",
            "parent_id",
            "name",
            "kind",
            "revision",
            "base",
            "written",
            "size",
            "modified",
            "content_type",
            "sha256",
            "object_id",
            "content_id",
            "wrapped_dek",
        ],
    ),
];

fn assert_schema(db: &Database) {
    assert_eq!(DEVICE_TABLES.len(), COLUMNS.len());
    for (table, columns) in COLUMNS {
        assert!(table_exists(db, table), "{table} must exist after 0030");
        assert_eq!(own_columns(db, table), columns, "columns of {table}");
        assert!(
            !has_hlc_column(db, table),
            "{table} is a cache of this device and must not be tracked"
        );
    }
}

#[tokio::test]
async fn migration_0030_gives_a_fresh_vault_the_cache_tables() {
    tokio::task::spawn_blocking(|| {
        let dir = tempfile::tempdir().expect("tempdir");
        let db = open(
            dir.path(),
            "encrypted-fresh",
            true,
            holzi_migration_source(),
        );
        assert_schema(&db);
    })
    .await
    .expect("join");
}

#[tokio::test]
async fn migration_0030_upgrades_a_vault_from_the_schema_before_it() {
    tokio::task::spawn_blocking(|| {
        let dir = tempfile::tempdir().expect("tempdir");
        let old = open(
            dir.path(),
            "encrypted-upgrade",
            true,
            migration_source_before("0030_encrypted_folders"),
        );
        assert!(!table_exists(&old, "encrypted_folder_heads_no_sync"));
        drop(old);
        let db = open(
            dir.path(),
            "encrypted-upgrade",
            false,
            holzi_migration_source(),
        );
        assert_schema(&db);
    })
    .await
    .expect("join");
}
