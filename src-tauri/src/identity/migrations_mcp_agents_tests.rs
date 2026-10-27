//! Migration tests for `0045_mcp_agents` (spec 021, data-model.md, T005): a fresh vault and a vault
//! from the schema before end with the three tables; the two agent tables are tracked by haex-crdt,
//! bound to a device and free of UNIQUE constraints, the log is not tracked.

// These tests read `sqlite_master` and `pragma_*` directly, which the CRDT write path does not
// expose.
#![allow(clippy::disallowed_methods)]

use haex_crdt::rusqlite::params;
use haex_crdt::Database;

use super::migrations_extensions_tests::{has_hlc_column, unique_indexes};
use super::migrations_mcp_agents::{LOG_TABLE, SYNCED_TABLES};
use super::migrations_tests::{migration_source_before, open, table_exists};
use crate::identity::holzi_migration_source;

fn columns(db: &Database, table: &str) -> Vec<String> {
    db.with_connection(|conn| {
        let mut stmt = conn.prepare("SELECT name FROM pragma_table_info(?1) ORDER BY cid")?;
        let rows = stmt.query_map(params![table], |row| row.get::<_, String>(0))?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    })
    .expect("read")
}

fn device_foreign_key(db: &Database, table: &str) -> Vec<(String, String)> {
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

fn assert_schema(db: &Database) {
    for table in SYNCED_TABLES {
        assert!(table_exists(db, table), "{table} exists");
        assert!(has_hlc_column(db, table), "{table} is tracked by haex-crdt");
        assert_eq!(
            unique_indexes(db, table),
            0,
            "{table}: a UNIQUE constraint would halt the sync"
        );
        assert_eq!(
            device_foreign_key(db, table),
            [("vault_device_uuid".to_owned(), "known_devices".to_owned())],
            "{table}: a row belongs to one device (ADR-0001)"
        );
    }
    assert!(table_exists(db, LOG_TABLE), "the log exists");
    assert!(
        !has_hlc_column(db, LOG_TABLE),
        "the log stays on the device"
    );
    let agents = columns(db, "haex_mcp_agents");
    for column in [
        "id",
        "vault_device_uuid",
        "kind",
        "slug",
        "client_name",
        "client_version",
        "transport",
        "oauth_client_id",
        "token_hash",
        "status",
        "permission_mode",
        "registered_at",
        "last_seen_at",
    ] {
        assert!(agents.iter().any(|c| c == column), "agents.{column}");
    }
    let grants = columns(db, "haex_mcp_agent_grants");
    for column in [
        "agent_id",
        "grant_group",
        "grant_key",
        "level",
        "scope_tags",
    ] {
        assert!(grants.iter().any(|c| c == column), "grants.{column}");
    }
    let log = columns(db, LOG_TABLE);
    for column in [
        "agent_id",
        "created_at",
        "tool",
        "effect",
        "target",
        "input",
        "outcome",
        "asked",
    ] {
        assert!(log.iter().any(|c| c == column), "log.{column}");
    }
}

#[tokio::test]
async fn migration_0045_gives_a_fresh_vault_the_agent_tables() {
    tokio::task::spawn_blocking(|| {
        let dir = tempfile::tempdir().expect("tempdir");
        let db = open(
            dir.path(),
            "mcp-agents-fresh",
            true,
            holzi_migration_source(),
        );
        assert_schema(&db);
    })
    .await
    .expect("join");
}

#[tokio::test]
async fn migration_0045_upgrades_a_vault_from_the_schema_before_it() {
    tokio::task::spawn_blocking(|| {
        let dir = tempfile::tempdir().expect("tempdir");
        let old = open(
            dir.path(),
            "mcp-agents-upgrade",
            true,
            migration_source_before("0045_mcp_agents"),
        );
        assert!(!table_exists(&old, "haex_mcp_agents"));
        drop(old);
        let db = open(
            dir.path(),
            "mcp-agents-upgrade",
            false,
            holzi_migration_source(),
        );
        assert_schema(&db);
    })
    .await
    .expect("join");
}
