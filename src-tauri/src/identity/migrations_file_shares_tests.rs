//! Migration tests for `0039_file_shares` (spec 056, data-model.md): a fresh vault and a vault from
//! the schema before end with the tables; the synced ones are tracked by haex-crdt without a UNIQUE
//! constraint, the switch is not, and a share takes its grants with it.

// These tests read `sqlite_master` and `pragma_*` directly, which the CRDT write path does not
// expose.
#![allow(clippy::disallowed_methods)]

use haex_crdt::rusqlite::params;
use haex_crdt::Database;

use super::migrations_extensions_tests::{has_hlc_column, unique_indexes};
use super::migrations_file_shares::SYNCED_TABLES;
use super::migrations_tests::{migration_source_before, open, table_exists};
use crate::identity::holzi_migration_source;

const SWITCH: &str = "haex_file_device_access_no_sync";

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
    assert!(table_exists(db, SWITCH), "the switch exists");
    assert!(
        !has_hlc_column(db, SWITCH),
        "the switch stays on this device (research R12)"
    );
}

fn foreign_keys(db: &Database, table: &str) -> Vec<(String, String)> {
    db.with_connection(|conn| {
        let mut stmt =
            conn.prepare("SELECT \"from\", \"table\" FROM pragma_foreign_key_list(?1)")?;
        let rows = stmt.query_map(params![table], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    })
    .expect("read")
}

#[tokio::test]
async fn migration_0039_gives_a_fresh_vault_the_file_share_tables() {
    tokio::task::spawn_blocking(|| {
        let dir = tempfile::tempdir().expect("tempdir");
        let db = open(
            dir.path(),
            "file-shares-fresh",
            true,
            holzi_migration_source(),
        );
        assert_schema(&db);
        assert_eq!(
            foreign_keys(&db, "haex_file_shares"),
            [("vault_device_uuid".to_owned(), "known_devices".to_owned())],
            "a share belongs to the device that serves it (FR-018)"
        );
        assert_eq!(
            foreign_keys(&db, "haex_file_share_grants"),
            [("share_id".to_owned(), "haex_file_shares".to_owned())],
            "a grant belongs to its share"
        );
    })
    .await
    .expect("join");
}

#[tokio::test]
async fn migration_0039_upgrades_a_vault_from_the_schema_before_it() {
    tokio::task::spawn_blocking(|| {
        let dir = tempfile::tempdir().expect("tempdir");
        let old = open(
            dir.path(),
            "file-shares-upgrade",
            true,
            migration_source_before("0039_file_shares"),
        );
        for table in SYNCED_TABLES {
            assert!(!table_exists(&old, table), "{table} is new");
        }
        drop(old);
        let db = open(
            dir.path(),
            "file-shares-upgrade",
            false,
            holzi_migration_source(),
        );
        assert_schema(&db);
    })
    .await
    .expect("join");
}
