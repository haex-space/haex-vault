//! Migration tests for `0062_extension_default_model`: a fresh vault and a vault from the schema
//! before it end with the default table, tracked by haex-crdt and without a UNIQUE constraint, and
//! without the rank table.

// These tests read `sqlite_master` and `pragma_*` directly, which the CRDT write path does not
// expose.
#![allow(clippy::disallowed_methods)]

use haex_crdt::rusqlite::params;
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

/// The rank goes without a delete marker (so no other device loses anything it still needs) and
/// the grants of the models stay: dropping the rank never takes a right away or gives one.
#[tokio::test]
async fn migration_0062_keeps_the_grants_and_writes_no_delete_marker_for_the_rank() {
    tokio::task::spawn_blocking(|| {
        let dir = tempfile::tempdir().expect("tempdir");
        let old = open(
            dir.path(),
            "default-model-keeps-grants",
            true,
            migration_source_before("0062_extension_default_model"),
        );
        old.with_connection(|conn| {
            conn.execute("PRAGMA foreign_keys = OFF", [])?;
            conn.execute(
                "INSERT INTO extension_model_order (id, extension_id, model_id, position, \
                 haex_hlc_no_sync) VALUES ('o1', 'e1', 'm1', 0, current_hlc())",
                [],
            )?;
            conn.execute(
                "INSERT INTO extension_permissions (id, extension_id, kind, action, target, \
                 status, declared, vault_device_uuid, updated_at, haex_hlc_no_sync) \
                 VALUES ('p1', 'e1', 'model', 'medium', 'm1', 'granted', 0, 'd1', 1, current_hlc())",
                [],
            )?;
            Ok(())
        })
        .expect("a vault with a rank and a grant");
        drop(old);

        let db = open(
            dir.path(),
            "default-model-keeps-grants",
            false,
            holzi_migration_source(),
        );
        assert_schema(&db);
        db.with_connection(|conn| {
            let granted: i64 = conn.query_row(
                "SELECT COUNT(*) FROM extension_permissions WHERE id = ?1 AND status = 'granted'",
                params!["p1"],
                |r| r.get(0),
            )?;
            assert_eq!(granted, 1, "the grant stays");
            let defaults: i64 =
                conn.query_row("SELECT COUNT(*) FROM extension_default_model", [], |r| {
                    r.get(0)
                })?;
            assert_eq!(defaults, 0, "no default is made up from the rank");
            let markers: i64 = conn.query_row(
                "SELECT COUNT(*) FROM haex_deleted_rows \
                 WHERE table_name = 'extension_model_order'",
                [],
                |r| r.get(0),
            )?;
            assert_eq!(markers, 0, "the drop writes no marker that would sync");
            Ok(())
        })
        .expect("read");
    })
    .await
    .expect("join");
}
