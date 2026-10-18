//! Migration tests for `0037_extension_model_access` (spec 055, data-model.md): a fresh vault and
//! a vault from the schema before it end with the rank table, tracked by haex-crdt and without a
//! UNIQUE constraint, and with the run-time limit of a model job on `extension_limits`.

// These tests read `sqlite_master` and `pragma_*` directly, which the CRDT write path does not
// expose.
#![allow(clippy::disallowed_methods)]

use haex_crdt::rusqlite::params;
use haex_crdt::Database;

use super::migrations_extensions_tests::{has_hlc_column, unique_indexes};
use super::migrations_model_access::SYNCED_TABLES;
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
        own_columns(db, "extension_model_order"),
        ["id", "extension_id", "model_id", "position"]
    );
    assert!(
        own_columns(db, "extension_limits").contains(&"model_job_timeout_ms".to_owned()),
        "the limits carry the run time of a model job"
    );
}

#[tokio::test]
async fn migration_0037_gives_a_fresh_vault_the_rank_table_and_the_limit() {
    tokio::task::spawn_blocking(|| {
        let dir = tempfile::tempdir().expect("tempdir");
        let db = open(
            dir.path(),
            "model-access-fresh",
            true,
            holzi_migration_source(),
        );
        assert_schema(&db);
    })
    .await
    .expect("join");
}

#[tokio::test]
async fn migration_0037_upgrades_a_vault_and_keeps_the_limits_it_had() {
    tokio::task::spawn_blocking(|| {
        let dir = tempfile::tempdir().expect("tempdir");
        let old = open(
            dir.path(),
            "model-access-upgrade",
            true,
            migration_source_before("0037_extension_model_access"),
        );
        assert!(!table_exists(&old, "extension_model_order"));
        assert!(!own_columns(&old, "extension_limits").contains(&"model_job_timeout_ms".to_owned()));
        old.with_connection(|conn| {
            conn.execute("PRAGMA foreign_keys = OFF", [])?;
            conn.execute(
                "INSERT INTO extension_limits (id, extension_id, max_rows, haex_hlc_no_sync) \
                 VALUES ('l1', 'e1', 7, current_hlc())",
                [],
            )?;
            Ok(())
        })
        .expect("a limits row of the old schema");
        drop(old);

        let db = open(
            dir.path(),
            "model-access-upgrade",
            false,
            holzi_migration_source(),
        );
        assert_schema(&db);
        let (rows, timeout): (i64, i64) = db
            .with_connection(|conn| {
                Ok(conn.query_row(
                    "SELECT max_rows, model_job_timeout_ms FROM extension_limits WHERE id = ?1",
                    params!["l1"],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )?)
            })
            .expect("read");
        assert_eq!(rows, 7, "the stored limit stays");
        assert_eq!(timeout, 300_000, "the new limit starts at its default");
    })
    .await
    .expect("join");
}
