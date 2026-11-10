//! Migration tests for `0056_drop_agent_grants` (spec 083 T072): a vault from the schema before it
//! loses `haex_mcp_agent_grants` and keeps its agents; the drop writes no delete marker that would
//! reach other devices; a fresh vault never has the table.

// These tests read `sqlite_master` and the delete log directly, which the CRDT write path does not
// expose.
#![allow(clippy::disallowed_methods)]

use haex_crdt::rusqlite::params;

use super::migrations_tests::{migration_source_before, open, table_exists};
use crate::identity::holzi_migration_source;

#[tokio::test]
async fn migration_0056_drops_the_old_rights_table_and_keeps_the_agents() {
    tokio::task::spawn_blocking(|| {
        let dir = tempfile::tempdir().expect("tempdir");
        let old = open(
            dir.path(),
            "drop-agent-grants",
            true,
            migration_source_before("0056_drop_agent_grants"),
        );
        assert!(table_exists(&old, "haex_mcp_agent_grants"));
        let device = old.device_id().to_string();
        old.with_connection(|conn| {
            conn.execute(
                "INSERT INTO haex_mcp_agents (id, vault_device_uuid, kind, slug, client_name, \
                 transport, status, permission_mode, registered_at) \
                 VALUES ('a1', ?1, 'external', 'codex', 'Codex', 'ipc', 'registered', 'manual', 1)",
                params![device],
            )?;
            conn.execute(
                "INSERT INTO haex_mcp_agent_grants (id, vault_device_uuid, agent_id, grant_group, \
                 grant_key, level) VALUES ('g1', ?1, 'a1', 'operate', 'chat.write', 'write')",
                params![device],
            )?;
            Ok(())
        })
        .expect("a vault from before");
        drop(old);

        let db = open(
            dir.path(),
            "drop-agent-grants",
            false,
            holzi_migration_source(),
        );
        assert!(!table_exists(&db, "haex_mcp_agent_grants"));
        db.with_connection(|conn| {
            let agents: i64 =
                conn.query_row("SELECT COUNT(*) FROM haex_mcp_agents", [], |row| row.get(0))?;
            assert_eq!(agents, 1, "the registration stays");
            let markers: i64 = conn.query_row(
                "SELECT COUNT(*) FROM haex_deleted_rows WHERE table_name = 'haex_mcp_agent_grants'",
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
async fn a_fresh_vault_has_no_old_rights_table() {
    tokio::task::spawn_blocking(|| {
        let dir = tempfile::tempdir().expect("tempdir");
        let db = open(
            dir.path(),
            "fresh-without-agent-grants",
            true,
            holzi_migration_source(),
        );
        assert!(!table_exists(&db, "haex_mcp_agent_grants"));
        assert!(table_exists(&db, "haex_mcp_agents"));
    })
    .await
    .expect("join");
}
