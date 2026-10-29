//! Migration tests for `0047_agent_browser` (spec 065, T010): a fresh vault and a vault from the
//! schema before get the table of the agent browser, and haex-crdt does not track it.

// These tests read `sqlite_master` and `pragma_*` directly, which the CRDT write path does not
// expose.
#![allow(clippy::disallowed_methods)]

use haex_crdt::rusqlite::params;
use haex_crdt::Database;

use super::migrations_agent_browser::TABLE;
use super::migrations_extensions_tests::has_hlc_column;
use super::migrations_tests::{migration_source_before, open, table_exists};
use crate::identity::holzi_migration_source;

fn columns(db: &Database) -> Vec<String> {
    db.with_connection(|conn| {
        let mut stmt = conn.prepare("SELECT name FROM pragma_table_info(?1) ORDER BY cid")?;
        let rows = stmt.query_map(params![TABLE], |row| row.get::<_, String>(0))?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    })
    .expect("read")
}

fn assert_schema(db: &Database) {
    assert!(table_exists(db, TABLE), "the table exists");
    assert!(!has_hlc_column(db, TABLE), "the table stays on the device");
    assert_eq!(
        columns(db),
        ["vault_device_uuid", "browser_path", "created_at"]
    );
}

#[tokio::test]
async fn migration_0047_gives_a_fresh_vault_the_agent_browser_table() {
    tokio::task::spawn_blocking(|| {
        let dir = tempfile::tempdir().expect("tempdir");
        let db = open(
            dir.path(),
            "agent-browser-fresh",
            true,
            holzi_migration_source(),
        );
        assert_schema(&db);
    })
    .await
    .expect("join");
}

#[tokio::test]
async fn migration_0047_upgrades_a_vault_from_the_schema_before_it() {
    tokio::task::spawn_blocking(|| {
        let dir = tempfile::tempdir().expect("tempdir");
        let old = open(
            dir.path(),
            "agent-browser-upgrade",
            true,
            migration_source_before("0047_agent_browser"),
        );
        assert!(!table_exists(&old, TABLE));
        drop(old);
        let db = open(
            dir.path(),
            "agent-browser-upgrade",
            false,
            holzi_migration_source(),
        );
        assert_schema(&db);
    })
    .await
    .expect("join");
}
