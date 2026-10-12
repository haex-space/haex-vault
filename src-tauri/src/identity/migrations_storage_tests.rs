//! Migration tests for `0028_storage_connections` (spec 038, data-model.md, research R3) and
//! `0031_storage_protocols` (spec 050, data-model.md): a genesis vault and a vault upgraded from the
//! schema before them both end with the three tables (the connections with the columns of 0031
//! appended), the connections and storages tracked by haex-crdt (they reach every own device,
//! FR-006) and the test results not, and no synced table with a UNIQUE constraint. A connection
//! from before 0031 reads as S3 without a host key.

// These tests read `sqlite_master` and `pragma_*` directly, which the CRDT write path does not
// expose.
#![allow(clippy::disallowed_methods)]

use haex_crdt::Database;

use super::migrations_extensions_tests::{has_hlc_column, unique_indexes};
use super::migrations_storage::{DEVICE_TABLES, SYNCED_TABLES};
use super::migrations_tests::{migration_source_before, open, own_columns, table_exists};
use crate::identity::holzi_migration_source;

const COLUMNS: [(&str, &[&str]); 3] = [
    (
        "haex_storage_connections",
        &[
            "id",
            "provider_name",
            "provider_kind",
            "endpoint",
            "endpoint_scope",
            "region",
            "addressing",
            "credentials_item_id",
            "created_at",
            "updated_at",
            "protocol",
            "host_key",
            "host_key_confirmed_at",
        ],
    ),
    (
        "haex_storages",
        &[
            "id",
            "connection_id",
            "name",
            "bucket",
            "created_at",
            "updated_at",
        ],
    ),
    (
        "storage_tests_no_sync",
        &["storage_id", "tested_at", "outcome"],
    ),
];

fn assert_storage_schema(db: &Database) {
    assert_eq!(
        SYNCED_TABLES.len() + DEVICE_TABLES.len(),
        COLUMNS.len(),
        "every table of 0028 is listed once"
    );
    for (table, columns) in COLUMNS {
        assert!(table_exists(db, table), "{table} must exist after 0028");
        assert_eq!(own_columns(db, table), columns, "columns of {table}");
    }
    for table in SYNCED_TABLES {
        assert!(
            has_hlc_column(db, table),
            "{table} is vault-wide, so haex-crdt must track it"
        );
        assert_eq!(
            unique_indexes(db, table),
            0,
            "{table}: a UNIQUE constraint would halt the sync on a conflict"
        );
    }
    for table in DEVICE_TABLES {
        assert!(
            !has_hlc_column(db, table),
            "{table} is an observation of this device and must not be tracked"
        );
    }
}

#[tokio::test]
async fn migration_0028_gives_a_fresh_vault_the_storage_tables() {
    tokio::task::spawn_blocking(|| {
        let dir = tempfile::tempdir().expect("tempdir");
        let db = open(dir.path(), "storage-fresh", true, holzi_migration_source());
        assert_storage_schema(&db);
    })
    .await
    .expect("join");
}

#[tokio::test]
async fn migration_0028_upgrades_a_vault_from_the_schema_before_it() {
    tokio::task::spawn_blocking(|| {
        let dir = tempfile::tempdir().expect("tempdir");
        let old = open(
            dir.path(),
            "storage-upgrade",
            true,
            migration_source_before("0028_storage_connections"),
        );
        assert!(!table_exists(&old, "haex_storages"));
        drop(old);

        let db = open(
            dir.path(),
            "storage-upgrade",
            false,
            holzi_migration_source(),
        );
        assert_storage_schema(&db);
    })
    .await
    .expect("join");
}

#[tokio::test]
async fn migration_0031_makes_existing_connections_s3_without_a_host_key() {
    tokio::task::spawn_blocking(|| {
        let dir = tempfile::tempdir().expect("tempdir");
        let old = open(
            dir.path(),
            "protocols-upgrade",
            true,
            migration_source_before("0031_storage_protocols"),
        );
        old.with_connection(|conn| {
            conn.execute(
                "INSERT INTO haex_storage_connections (id, provider_name, provider_kind, endpoint, \
                 endpoint_scope, region, addressing, credentials_item_id, created_at, updated_at) \
                 VALUES ('c', 'RustFS', 'rustfs', 'http://127.0.0.1:9000', 'local', 'us-east-1', \
                 'path', 'item', '2026-10-12T00:00:00Z', '2026-10-12T00:00:00Z')",
                [],
            )?;
            Ok(())
        })
        .expect("insert a connection before the migration");
        drop(old);

        let db = open(
            dir.path(),
            "protocols-upgrade",
            false,
            holzi_migration_source(),
        );
        assert_storage_schema(&db);
        let (protocol, host_key, confirmed): (String, Option<String>, Option<String>) = db
            .with_connection(|conn| {
                Ok(conn.query_row(
                    "SELECT protocol, host_key, host_key_confirmed_at \
                     FROM haex_storage_connections WHERE id = 'c'",
                    [],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
                )?)
            })
            .expect("read the connection");
        assert_eq!(protocol, "s3", "a connection from before spec 050 is S3");
        assert_eq!((host_key, confirmed), (None, None));
    })
    .await
    .expect("join");
}
