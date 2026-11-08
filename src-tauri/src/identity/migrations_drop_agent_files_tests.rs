//! Migration tests for `0059_drop_agent_files` (spec 083 FR-033): a vault from the schema before it
//! loses `agent_file_permissions`; the drop writes no delete marker that would reach other devices;
//! a fresh vault never has the table.

// These tests read `sqlite_master` and the delete log directly, which the CRDT write path does not
// expose.
#![allow(clippy::disallowed_methods)]

use haex_crdt::rusqlite::params;

use super::migrations_tests::{migration_source_before, open, table_exists};
use crate::identity::holzi_migration_source;

#[tokio::test]
async fn migration_0059_drops_the_old_file_permissions() {
    tokio::task::spawn_blocking(|| {
        let dir = tempfile::tempdir().expect("tempdir");
        let old = open(
            dir.path(),
            "drop-agent-files",
            true,
            migration_source_before("0059_drop_agent_files"),
        );
        assert!(table_exists(&old, "agent_file_permissions"));
        old.with_connection(|conn| {
            conn.execute(
                "INSERT INTO agent_file_permissions (id, agent_id, kind, target, status, updated_at) \
                 VALUES ('p1', 'builtin', 'device', '', 'granted', 1)",
                params![],
            )?;
            Ok(())
        })
        .expect("a vault from before");
        drop(old);

        let db = open(dir.path(), "drop-agent-files", false, holzi_migration_source());
        assert!(!table_exists(&db, "agent_file_permissions"));
        db.with_connection(|conn| {
            let markers: i64 = conn.query_row(
                "SELECT COUNT(*) FROM haex_deleted_rows WHERE table_name = 'agent_file_permissions'",
                [],
                |row| row.get(0),
            )?;
            assert_eq!(markers, 0, "a drop writes no marker that would sync");
            Ok(())
        })
        .expect("read");
    })
    .await
    .expect("join");
}

#[tokio::test]
async fn a_fresh_vault_has_no_file_permissions_table() {
    tokio::task::spawn_blocking(|| {
        let dir = tempfile::tempdir().expect("tempdir");
        let db = open(
            dir.path(),
            "fresh-without-agent-files",
            true,
            holzi_migration_source(),
        );
        assert!(!table_exists(&db, "agent_file_permissions"));
    })
    .await
    .expect("join");
}
