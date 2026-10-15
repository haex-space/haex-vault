//! Migration tests for `0035_file_icon_sets` (spec 053, data-model.md): a fresh vault and a vault
//! from the schema before end with the table, tracked by haex-crdt and without a UNIQUE
//! constraint; deleting a set leaves a delete marker, so the delete reaches the other devices.

// These tests read `sqlite_master` and `pragma_*` directly, which the CRDT write path does not
// expose.
#![allow(clippy::disallowed_methods)]

use haex_crdt::rusqlite::params;
use haex_crdt::Database;

use super::migrations_extensions_tests::{has_hlc_column, unique_indexes};
use super::migrations_icon_sets::SYNCED_TABLES;
use super::migrations_tests::{migration_source_before, open, table_exists};
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
}

fn count(db: &Database, sql: &str) -> i64 {
    crate::storage::query::read(db, |q| {
        use crate::storage::query::Query;
        q.query_row(sql, &[], |row| row.get(0))
    })
    .expect("count")
    .unwrap_or(0)
}

fn primary_key_columns(db: &Database, table: &str) -> Vec<String> {
    db.with_connection(|conn| {
        let mut stmt =
            conn.prepare("SELECT name FROM pragma_table_info(?1) WHERE pk > 0 ORDER BY pk")?;
        let rows = stmt.query_map(params![table], |row| row.get::<_, String>(0))?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    })
    .expect("read")
}

#[tokio::test]
async fn migration_0035_gives_a_fresh_vault_the_icon_set_table() {
    tokio::task::spawn_blocking(|| {
        let dir = tempfile::tempdir().expect("tempdir");
        let db = open(
            dir.path(),
            "icon-sets-fresh",
            true,
            holzi_migration_source(),
        );
        assert_schema(&db);
        assert_eq!(primary_key_columns(&db, "haex_file_icon_sets"), ["id"]);
    })
    .await
    .expect("join");
}

#[tokio::test]
async fn migration_0035_upgrades_a_vault_from_the_schema_before_it() {
    tokio::task::spawn_blocking(|| {
        let dir = tempfile::tempdir().expect("tempdir");
        let old = open(
            dir.path(),
            "icon-sets-upgrade",
            true,
            migration_source_before("0035_file_icon_sets"),
        );
        assert!(!table_exists(&old, "haex_file_icon_sets"));
        drop(old);
        let db = open(
            dir.path(),
            "icon-sets-upgrade",
            false,
            holzi_migration_source(),
        );
        assert_schema(&db);
    })
    .await
    .expect("join");
}

#[tokio::test]
async fn deleting_a_set_leaves_a_marker_for_the_other_devices() {
    tokio::task::spawn_blocking(|| {
        let dir = tempfile::tempdir().expect("tempdir");
        let db = open(
            dir.path(),
            "icon-sets-delete",
            true,
            holzi_migration_source(),
        );
        db.write(|tx| {
            tx.execute(
                "INSERT INTO haex_file_icon_sets (id, name, image_count, byte_size, sha256, \
                 pack, created_at) VALUES ('s1', 'Mine', 1, 3, 'abc', x'010203', 't')",
                &[],
            )?;
            Ok(())
        })
        .expect("insert a set");
        assert_eq!(count(&db, "SELECT COUNT(*) FROM haex_file_icon_sets"), 1);

        db.write(|tx| {
            tx.execute("DELETE FROM haex_file_icon_sets WHERE id = 's1'", &[])?;
            Ok(())
        })
        .expect("delete the set");
        assert_eq!(count(&db, "SELECT COUNT(*) FROM haex_file_icon_sets"), 0);
        assert_eq!(
            count(
                &db,
                "SELECT COUNT(*) FROM haex_deleted_rows WHERE table_name = 'haex_file_icon_sets'"
            ),
            1,
            "the delete leaves a marker that reaches the other devices"
        );
    })
    .await
    .expect("join");
}
