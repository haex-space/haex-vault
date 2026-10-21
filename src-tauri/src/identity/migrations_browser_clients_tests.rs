//! Migration tests for `0038_browser_clients` (spec 049, data-model.md): a fresh vault and a vault
//! from the schema before end with the table, tracked by haex-crdt, without a UNIQUE constraint and
//! bound to a device.

// These tests read `sqlite_master` and `pragma_*` directly, which the CRDT write path does not
// expose.
#![allow(clippy::disallowed_methods)]

use haex_crdt::rusqlite::params;
use haex_crdt::Database;

use super::migrations_browser_clients::SYNCED_TABLES;
use super::migrations_extensions_tests::{has_hlc_column, unique_indexes};
use super::migrations_tests::{migration_source_before, open, table_exists};
use crate::identity::holzi_migration_source;

fn assert_schema(db: &Database) {
    for table in SYNCED_TABLES {
        assert!(table_exists(db, table), "{table} exists");
        assert!(has_hlc_column(db, table), "{table} is tracked by haex-crdt");
        assert_eq!(
            unique_indexes(db, table),
            0,
            "{table}: a UNIQUE constraint would halt the sync"
        );
    }
}

fn device_foreign_key(db: &Database) -> Vec<(String, String)> {
    db.with_connection(|conn| {
        let mut stmt =
            conn.prepare("SELECT \"from\", \"table\" FROM pragma_foreign_key_list(?1)")?;
        let rows = stmt.query_map(params!["haex_browser_clients"], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    })
    .expect("read")
}

#[tokio::test]
async fn migration_0038_gives_a_fresh_vault_the_browser_client_table() {
    tokio::task::spawn_blocking(|| {
        let dir = tempfile::tempdir().expect("tempdir");
        let db = open(
            dir.path(),
            "browser-clients-fresh",
            true,
            holzi_migration_source(),
        );
        assert_schema(&db);
        assert_eq!(
            device_foreign_key(&db),
            [("vault_device_uuid".to_owned(), "known_devices".to_owned())],
            "a row belongs to one device (ADR-0001)"
        );
    })
    .await
    .expect("join");
}

#[tokio::test]
async fn migration_0038_upgrades_a_vault_from_the_schema_before_it() {
    tokio::task::spawn_blocking(|| {
        let dir = tempfile::tempdir().expect("tempdir");
        let old = open(
            dir.path(),
            "browser-clients-upgrade",
            true,
            migration_source_before("0038_browser_clients"),
        );
        assert!(!table_exists(&old, "haex_browser_clients"));
        drop(old);
        let db = open(
            dir.path(),
            "browser-clients-upgrade",
            false,
            holzi_migration_source(),
        );
        assert_schema(&db);
    })
    .await
    .expect("join");
}
