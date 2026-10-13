//! Migration tests for `0029_agent_file_permissions` (spec 044, data-model.md) and
//! `0032_agent_folder_reach` (spec 048): a genesis vault and a vault upgraded from the schema before
//! either both end with the table and its `reach`, tracked by haex-crdt and without a UNIQUE
//! constraint; rows from before 0032 keep no reach.

// These tests read `sqlite_master` and `pragma_*` directly, which the CRDT write path does not
// expose.
#![allow(clippy::disallowed_methods)]

use haex_crdt::Database;

use super::migrations_extensions_tests::{has_hlc_column, unique_indexes};
use super::migrations_tests::{migration_source_before, open, own_columns, table_exists};
use crate::identity::holzi_migration_source;

const TABLE: &str = "agent_file_permissions";

fn assert_schema(db: &Database) {
    assert!(table_exists(db, TABLE));
    assert_eq!(
        own_columns(db, TABLE),
        [
            "id",
            "agent_id",
            "kind",
            "target",
            "status",
            "updated_at",
            "reach"
        ]
    );
    assert!(
        has_hlc_column(db, TABLE),
        "a revoked grant must reach every own device"
    );
    assert_eq!(
        unique_indexes(db, TABLE),
        0,
        "a UNIQUE constraint would halt the sync"
    );
}

#[tokio::test]
async fn migration_0029_gives_a_fresh_vault_the_table() {
    tokio::task::spawn_blocking(|| {
        let dir = tempfile::tempdir().expect("tempdir");
        let db = open(
            dir.path(),
            "agent-files-fresh",
            true,
            holzi_migration_source(),
        );
        assert_schema(&db);
    })
    .await
    .expect("join");
}

#[tokio::test]
async fn migration_0029_upgrades_a_vault_from_the_schema_before_it() {
    tokio::task::spawn_blocking(|| {
        let dir = tempfile::tempdir().expect("tempdir");
        let old = open(
            dir.path(),
            "agent-files-upgrade",
            true,
            migration_source_before("0029_agent_file_permissions"),
        );
        assert!(!table_exists(&old, TABLE));
        drop(old);
        let db = open(
            dir.path(),
            "agent-files-upgrade",
            false,
            holzi_migration_source(),
        );
        assert_schema(&db);
    })
    .await
    .expect("join");
}

#[tokio::test]
async fn migration_0032_gives_existing_rows_no_reach() {
    tokio::task::spawn_blocking(|| {
        let dir = tempfile::tempdir().expect("tempdir");
        let old = open(
            dir.path(),
            "agent-folder-reach",
            true,
            migration_source_before("0032_agent_folder_reach"),
        );
        old.with_connection(|conn| {
            conn.execute(
                "INSERT INTO agent_file_permissions (id, agent_id, kind, target, status, updated_at) \
                 VALUES ('p', 'builtin', 'storage', 's1', 'read', 1)",
                [],
            )?;
            Ok(())
        })
        .expect("insert a permission before the migration");
        drop(old);

        let db = open(
            dir.path(),
            "agent-folder-reach",
            false,
            holzi_migration_source(),
        );
        assert_schema(&db);
        let reach: Option<String> = db
            .with_connection(|conn| {
                Ok(conn.query_row(
                    "SELECT reach FROM agent_file_permissions WHERE id = 'p'",
                    [],
                    |r| r.get(0),
                )?)
            })
            .expect("read the permission");
        assert_eq!(reach, None);
    })
    .await
    .expect("join");
}
