//! Migration tests for `0057_agent_memberships` (spec 083 FR-006): a vault from the schema before it
//! gets the lookup index by subject and keeps its member rows; a fresh vault has the index.

// These tests read `sqlite_master` directly, which the CRDT write path does not expose.
#![allow(clippy::disallowed_methods)]

use haex_crdt::rusqlite::params;

use super::migrations_tests::{migration_source_before, open};
use crate::identity::holzi_migration_source;

const INDEX: &str = "haex_agent_profile_members_subject";

fn index_exists(db: &haex_crdt::Database, name: &str) -> bool {
    db.with_connection(|conn| {
        Ok(conn.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'index' AND name = ?1",
            params![name],
            |row| row.get::<_, i64>(0),
        )? > 0)
    })
    .expect("sqlite_master")
}

#[tokio::test]
async fn migration_0057_adds_the_subject_index_and_keeps_the_member_rows() {
    tokio::task::spawn_blocking(|| {
        let dir = tempfile::tempdir().expect("tempdir");
        let old = open(
            dir.path(),
            "agent-memberships",
            true,
            migration_source_before("0057_agent_memberships"),
        );
        assert!(!index_exists(&old, INDEX));
        old.with_connection(|conn| {
            conn.execute(
                "INSERT INTO haex_agent_profile_members (id, subject, profile_id) \
                 VALUES ('m1', 'model:a', 'p1')",
                [],
            )?;
            Ok(())
        })
        .expect("a vault from before");
        drop(old);

        let db = open(
            dir.path(),
            "agent-memberships",
            false,
            holzi_migration_source(),
        );
        assert!(index_exists(&db, INDEX));
        db.with_connection(|conn| {
            let rows: i64 = conn.query_row(
                "SELECT COUNT(*) FROM haex_agent_profile_members",
                [],
                |row| row.get(0),
            )?;
            assert_eq!(rows, 1, "the membership stays");
            Ok(())
        })
        .expect("read");
    })
    .await
    .expect("join");
}

#[tokio::test]
async fn a_fresh_vault_has_the_subject_index() {
    tokio::task::spawn_blocking(|| {
        let dir = tempfile::tempdir().expect("tempdir");
        let db = open(
            dir.path(),
            "fresh-with-memberships",
            true,
            holzi_migration_source(),
        );
        assert!(index_exists(&db, INDEX));
    })
    .await
    .expect("join");
}
