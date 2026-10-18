//! Migration tests for `0023_extensions` (spec 017-extension-host, data-model.md): a genesis vault
//! and a vault upgraded from the schema before it both end with every registry table, the synced
//! ones tracked by haex-crdt and the device ones not, and none with a UNIQUE constraint.

// These tests read `sqlite_master` and `pragma_*` directly, which the CRDT write path does not
// expose.
#![allow(clippy::disallowed_methods)]

use haex_crdt::rusqlite::params;
use haex_crdt::Database;

use super::migrations_extensions::{DEVICE_TABLES, SYNCED_TABLES};
use super::migrations_tests::{migration_source_before, open, own_columns, table_exists};
use crate::identity::holzi_migration_source;

const COLUMNS: [(&str, &[&str]); 15] = [
    (
        "extensions",
        &[
            "id",
            "public_key",
            "name",
            "display_name",
            "enabled",
            "state",
            "purge_data",
            "purge_hlc",
            "installed_at",
            "updated_at",
        ],
    ),
    (
        "extension_bundles",
        &[
            "id",
            "extension_id",
            "version",
            "manifest_json",
            "signature_json",
            "retired",
            "added_at",
        ],
    ),
    (
        "extension_bundle_files",
        &["id", "bundle_id", "path", "size", "sha256"],
    ),
    ("extension_blobs", &["hash", "data", "size", "orphaned_at"]),
    (
        "extension_migrations",
        &[
            "id",
            "extension_id",
            "name",
            "position",
            "sql",
            "sql_sha256",
        ],
    ),
    (
        "extension_permissions",
        &[
            "id",
            "extension_id",
            "kind",
            "action",
            "target",
            "status",
            "declared",
            "vault_device_uuid",
            "updated_at",
        ],
    ),
    (
        "extension_limits",
        &[
            "id",
            "extension_id",
            "max_rows",
            "max_concurrent",
            "max_sql_bytes",
            "timeout_ms",
            "max_response_bytes",
            "model_job_timeout_ms",
        ],
    ),
    (
        "extension_device_status",
        &[
            "id",
            "extension_id",
            "vault_device_uuid",
            "status",
            "bundle_id",
            "error",
            "updated_at",
        ],
    ),
    (
        "extension_kv",
        &["vault_device_uuid", "extension_id", "key", "value"],
    ),
    (
        "extension_migrations_applied_no_sync",
        &["extension_id", "name", "sql_sha256", "applied_at"],
    ),
    (
        "extension_purges_applied_no_sync",
        &["extension_id", "purge_hlc", "data_purge_hlc"],
    ),
    (
        "extension_logs_no_sync",
        &[
            "id",
            "extension_id",
            "vault_device_uuid",
            "level",
            "message",
            "metadata",
            "created_at",
        ],
    ),
    (
        "sync_parked_groups_no_sync",
        &[
            "id",
            "origin",
            "hlc",
            "extension_prefix",
            "tables",
            "group_blob",
            "bytes",
            "reason",
            "parked_at",
        ],
    ),
    (
        "dev_extensions_no_sync",
        &[
            "id",
            "public_key",
            "name",
            "display_name",
            "enabled",
            "vault_device_uuid",
            "project_path",
            "dev_url",
            "installed_at",
            "updated_at",
        ],
    ),
    (
        "dev_extension_permissions_no_sync",
        &[
            "id",
            "extension_id",
            "kind",
            "action",
            "target",
            "status",
            "declared",
            "vault_device_uuid",
            "updated_at",
        ],
    ),
];

pub(super) fn has_hlc_column(db: &Database, table: &str) -> bool {
    db.with_connection(|conn| {
        Ok(conn.query_row(
            "SELECT COUNT(*) FROM pragma_table_info(?1) WHERE name = 'haex_hlc_no_sync'",
            params![table],
            |r| r.get::<_, i64>(0),
        )?)
    })
    .expect("read columns")
        > 0
}

pub(super) fn unique_indexes(db: &Database, table: &str) -> i64 {
    db.with_connection(|conn| {
        Ok(conn.query_row(
            "SELECT COUNT(*) FROM pragma_index_list(?1) \
             WHERE \"unique\" = 1 AND origin IN ('c', 'u')",
            params![table],
            |r| r.get::<_, i64>(0),
        )?)
    })
    .expect("index list")
}

fn assert_extensions_schema(db: &Database) {
    assert_eq!(
        SYNCED_TABLES.len() + DEVICE_TABLES.len(),
        COLUMNS.len(),
        "every table of 0023 is listed once"
    );
    for (table, columns) in COLUMNS {
        assert!(table_exists(db, table), "{table} must exist after 0023");
        assert_eq!(own_columns(db, table), columns, "columns of {table}");
    }
    for table in SYNCED_TABLES {
        assert_eq!(
            unique_indexes(db, table),
            0,
            "{table}: a UNIQUE constraint would halt the sync on a conflict (research R5)"
        );
    }
    assert_eq!(
        unique_indexes(db, "sync_parked_groups_no_sync"),
        1,
        "a parked group is stored once per origin and HLC (0025)"
    );
    for table in SYNCED_TABLES {
        assert!(
            has_hlc_column(db, table),
            "{table} is synced, so haex-crdt must track it"
        );
    }
    for table in DEVICE_TABLES {
        assert!(
            !has_hlc_column(db, table),
            "{table} stays on the device and must not be tracked"
        );
    }
}

#[tokio::test]
async fn migration_0023_gives_a_fresh_vault_the_extension_tables() {
    tokio::task::spawn_blocking(|| {
        let dir = tempfile::tempdir().expect("tempdir");
        let db = open(
            dir.path(),
            "extensions-migration-fresh",
            true,
            holzi_migration_source(),
        );
        assert_extensions_schema(&db);
    })
    .await
    .expect("join");
}

#[tokio::test]
async fn migration_0023_upgrades_a_vault_from_before_it() {
    tokio::task::spawn_blocking(|| {
        let dir = tempfile::tempdir().expect("tempdir");
        let old = open(
            dir.path(),
            "extensions-migration-upgrade",
            true,
            migration_source_before("0023_extensions"),
        );
        assert!(!table_exists(&old, "extensions"));
        drop(old);

        let db = open(
            dir.path(),
            "extensions-migration-upgrade",
            false,
            holzi_migration_source(),
        );
        assert_extensions_schema(&db);
    })
    .await
    .expect("join");
}

#[tokio::test]
async fn migration_0025_keeps_one_of_each_parked_group_and_adds_the_data_purge() {
    tokio::task::spawn_blocking(|| {
        let dir = tempfile::tempdir().expect("tempdir");
        let old = open(
            dir.path(),
            "sync-parking-migration-upgrade",
            true,
            migration_source_before("0025_sync_parking"),
        );
        old.with_connection(|conn| {
            for _ in 0..2 {
                conn.execute(
                    "INSERT INTO sync_parked_groups_no_sync (origin, hlc, extension_prefix, tables, \
                     group_blob, bytes, reason, parked_at) \
                     VALUES ('o', 'h', 'p', '[]', x'00', 1, 'missing_table', 0)",
                    [],
                )?;
            }
            Ok(())
        })
        .expect("park twice");
        drop(old);

        let db = open(
            dir.path(),
            "sync-parking-migration-upgrade",
            false,
            holzi_migration_source(),
        );
        let parked = db
            .with_connection(|conn| {
                Ok(conn.query_row(
                    "SELECT COUNT(*) FROM sync_parked_groups_no_sync",
                    [],
                    |r| r.get::<_, i64>(0),
                )?)
            })
            .expect("count");
        assert_eq!(parked, 1);
        assert_extensions_schema(&db);
    })
    .await
    .expect("join");
}
