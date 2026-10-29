//! Migration tests for `0048_marketplaces` (spec 069, data-model.md): a fresh vault and a vault
//! from the schema before end with the table, tracked by haex-crdt, without a UNIQUE constraint and
//! with the defaults of the data model.

// These tests read `sqlite_master` and `pragma_*` directly, which the CRDT write path does not
// expose.
#![allow(clippy::disallowed_methods)]

use haex_crdt::Database;

use super::migrations_extensions_tests::{has_hlc_column, unique_indexes};
use super::migrations_marketplaces::SYNCED_TABLES;
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

/// `(name, declared default)` of every column of `haex_marketplaces`.
fn defaults(db: &Database) -> Vec<(String, Option<String>)> {
    db.with_connection(|conn| {
        let mut stmt = conn.prepare("SELECT name, dflt_value FROM pragma_table_info(?1)")?;
        let rows = stmt.query_map(["haex_marketplaces"], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?))
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    })
    .expect("read")
}

fn foreign_keys(db: &Database) -> i64 {
    db.with_connection(|conn| {
        Ok(conn.query_row(
            "SELECT COUNT(*) FROM pragma_foreign_key_list(?1)",
            ["haex_marketplaces"],
            |row| row.get(0),
        )?)
    })
    .expect("read")
}

#[tokio::test]
async fn migration_0048_gives_a_fresh_vault_the_marketplace_table() {
    tokio::task::spawn_blocking(|| {
        let dir = tempfile::tempdir().expect("tempdir");
        let db = open(
            dir.path(),
            "marketplaces-fresh",
            true,
            holzi_migration_source(),
        );
        assert_schema(&db);
        let defaults = defaults(&db);
        let default_of = |column: &str| {
            defaults
                .iter()
                .find(|(name, _)| name == column)
                .unwrap_or_else(|| panic!("column {column}"))
                .1
                .clone()
        };
        assert_eq!(default_of("enabled").as_deref(), Some("1"));
        assert_eq!(default_of("auth_type").as_deref(), Some("'none'"));
        assert_eq!(default_of("sort_order"), None, "the rank is always given");
        assert_eq!(
            foreign_keys(&db),
            0,
            "the credentials entry may arrive after the row (research R11)"
        );
    })
    .await
    .expect("join");
}

#[tokio::test]
async fn migration_0048_adds_the_table_to_a_vault_from_before() {
    tokio::task::spawn_blocking(|| {
        let dir = tempfile::tempdir().expect("tempdir");
        let before = migration_source_before("0048_marketplaces");
        let db = open(dir.path(), "marketplaces-upgrade", true, before);
        for table in SYNCED_TABLES {
            assert!(!table_exists(&db, table), "{table} is not there yet");
        }
        drop(db);
        let db = open(
            dir.path(),
            "marketplaces-upgrade",
            false,
            holzi_migration_source(),
        );
        assert_schema(&db);
    })
    .await
    .expect("join");
}
