//! Migration tests for `0054_agent_profiles` (spec 083, data-model.md, T005): a fresh vault and a vault
//! from the schema before end with the three tables; they are tracked by haex-crdt, belong to the
//! vault (no device column), and carry neither a UNIQUE constraint nor a foreign key. A second row
//! with the id of a member row is refused by the primary key, and a delete leaves a marker.

// These tests read `sqlite_master` and `pragma_*` directly, which the CRDT write path does not
// expose.
#![allow(clippy::disallowed_methods)]

use haex_crdt::rusqlite::params;
use haex_crdt::{Database, DatabaseConfig, NoopSignatureProvider, SqlCipherKey};

use super::migrations_agent_profiles::SYNCED_TABLES;
use super::migrations_extensions_tests::{has_hlc_column, unique_indexes};
use super::migrations_tests::{migration_source_before, open, table_exists};
use crate::identity::{holzi_migration_source, installation_id_path, HolziBootstrap};

fn columns(db: &Database, table: &str) -> Vec<String> {
    db.with_connection(|conn| {
        let mut stmt = conn.prepare("SELECT name FROM pragma_table_info(?1) ORDER BY cid")?;
        let rows = stmt.query_map(params![table], |row| row.get::<_, String>(0))?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    })
    .expect("read")
}

fn foreign_keys(db: &Database, table: &str) -> usize {
    db.with_connection(|conn| {
        Ok(conn.query_row(
            "SELECT COUNT(*) FROM pragma_foreign_key_list(?1)",
            params![table],
            |row| row.get::<_, i64>(0),
        )? as usize)
    })
    .expect("read")
}

/// The columns of the table as the data model lists them, without the columns haex-crdt adds.
fn expected(table: &str) -> &'static [&'static str] {
    match table {
        "haex_agent_profiles" => &["id", "name"],
        "haex_agent_profile_grants" => &[
            "id",
            "profile_id",
            "grant_group",
            "grant_key",
            "level",
            "scope_tags",
        ],
        "haex_agent_profile_members" => &["id", "subject", "profile_id"],
        other => panic!("unknown table {other}"),
    }
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
            foreign_keys(db, table),
            0,
            "{table}: deleting a profile deletes its rows explicitly, so each leaves a marker"
        );
        let found = columns(db, table);
        for column in expected(table) {
            assert!(found.iter().any(|c| c == column), "{table}.{column}");
        }
        assert!(
            !found.iter().any(|c| c == "vault_device_uuid"),
            "{table} belongs to the vault, not to a device"
        );
    }
}

#[tokio::test]
async fn migration_0054_gives_a_fresh_vault_the_profile_tables() {
    tokio::task::spawn_blocking(|| {
        let dir = tempfile::tempdir().expect("tempdir");
        let db = open(
            dir.path(),
            "agent-profiles-fresh",
            true,
            holzi_migration_source(),
        );
        assert_schema(&db);
    })
    .await
    .expect("join");
}

#[tokio::test]
async fn migration_0055_adds_non_unique_profile_id_indexes() {
    tokio::task::spawn_blocking(|| {
        let dir = tempfile::tempdir().expect("tempdir");
        let db = open(
            dir.path(),
            "agent-profile-indexes",
            true,
            holzi_migration_source(),
        );
        db.with_connection(|conn| {
            for (table, index) in [
                (
                    "haex_agent_profile_grants",
                    "haex_agent_profile_grants_profile_id",
                ),
                (
                    "haex_agent_profile_members",
                    "haex_agent_profile_members_profile_id",
                ),
            ] {
                let present: i64 = conn.query_row(
                    "SELECT COUNT(*) FROM pragma_index_list(?1) WHERE name = ?2 AND \"unique\" = 0",
                    params![table, index],
                    |row| row.get(0),
                )?;
                assert_eq!(present, 1, "{index} is a non-unique index");
            }
            Ok(())
        })
        .expect("read indexes");
    })
    .await
    .expect("join");
}

#[tokio::test]
async fn migration_0054_upgrades_a_vault_from_the_schema_before_it() {
    tokio::task::spawn_blocking(|| {
        let dir = tempfile::tempdir().expect("tempdir");
        let old = open(
            dir.path(),
            "agent-profiles-upgrade",
            true,
            migration_source_before("0054_agent_profiles"),
        );
        assert!(!table_exists(&old, "haex_agent_profiles"));
        drop(old);
        let db = open(
            dir.path(),
            "agent-profiles-upgrade",
            false,
            holzi_migration_source(),
        );
        assert_schema(&db);
    })
    .await
    .expect("join");
}

#[tokio::test]
async fn migration_0054_upgrades_trigger_version_34_and_tracks_deletes_on_all_tables() {
    tokio::task::spawn_blocking(|| {
        let dir = tempfile::tempdir().expect("tempdir");
        let old = Database::open(DatabaseConfig {
            path: dir.path().join("vault.db"),
            key: SqlCipherKey::new("agent-profiles-trigger-upgrade"),
            create_if_missing: true,
            bootstrap: std::sync::Arc::new(HolziBootstrap::new(installation_id_path(dir.path()))),
            signature_provider: std::sync::Arc::new(NoopSignatureProvider),
            migration_source: migration_source_before("0054_agent_profiles"),
            trigger_version: 34,
            max_transaction_bytes: haex_crdt::MAX_CRDT_TRANSACTION_BYTES,
            max_value_bytes: haex_crdt::MAX_VALUE_BYTES,
        })
        .expect("open version 34 vault");
        drop(old);
        let db = open(
            dir.path(),
            "agent-profiles-trigger-upgrade",
            false,
            holzi_migration_source(),
        );
        db.write(|tx| {
            tx.execute(
                "INSERT INTO haex_agent_profiles (id, name) VALUES ('p', 'Profile')",
                params![],
            )?;
            tx.execute(
                "INSERT INTO haex_agent_profile_grants \
                 (id, profile_id, grant_group, grant_key, level) VALUES ('g', 'p', 'operate', 'x', 'read')",
                params![],
            )?;
            tx.execute(
                "INSERT INTO haex_agent_profile_members (id, subject, profile_id) VALUES ('m', 'agent:a', 'p')",
                params![],
            )?;
            tx.execute(
                "UPDATE haex_agent_profiles SET name = 'Changed' WHERE id = 'p'",
                params![],
            )?;
            tx.execute(
                "UPDATE haex_agent_profile_grants SET level = 'write' WHERE id = 'g'",
                params![],
            )?;
            tx.execute(
                "UPDATE haex_agent_profile_members SET profile_id = 'p2' WHERE id = 'm'",
                params![],
            )?;
            Ok(())
        })
        .expect("write tracked profile rows");
        db.with_connection(|conn| {
            for table in SYNCED_TABLES {
                let marked: i64 = conn.query_row(
                    &format!("SELECT COUNT(*) FROM {table} WHERE haex_hlc_no_sync IS NOT NULL"),
                    [],
                    |row| row.get(0),
                )?;
                assert!(marked > 0, "{table} updates are tracked");
            }
            Ok(())
        })
        .expect("read updated profile rows");
        db.write(|tx| {
            tx.execute("DELETE FROM haex_agent_profiles WHERE id = 'p'", params![])?;
            tx.execute(
                "DELETE FROM haex_agent_profile_grants WHERE id = 'g'",
                params![],
            )?;
            tx.execute(
                "DELETE FROM haex_agent_profile_members WHERE id = 'm'",
                params![],
            )?;
            Ok(())
        })
        .expect("delete tracked profile rows");
        db.with_connection(|conn| {
            for table in SYNCED_TABLES {
                let markers: i64 = conn.query_row(
                    "SELECT COUNT(*) FROM haex_deleted_rows WHERE table_name = ?1",
                    params![table],
                    |row| row.get(0),
                )?;
                assert!(markers >= 1, "{table} delete creates a sync marker");
            }
            Ok(())
        })
        .expect("inspect delete markers");
    })
    .await
    .expect("join");
}

#[tokio::test]
async fn a_second_member_row_with_the_same_id_is_refused() {
    tokio::task::spawn_blocking(|| {
        let dir = tempfile::tempdir().expect("tempdir");
        let db = open(
            dir.path(),
            "agent-profiles-member-pk",
            true,
            holzi_migration_source(),
        );
        db.with_connection(|conn| {
            conn.execute(
                "INSERT INTO haex_agent_profile_members (id, subject, profile_id) VALUES ('m', 'agent:a', 'p1')",
                [],
            )?;
            let second = conn.execute(
                "INSERT INTO haex_agent_profile_members (id, subject, profile_id) VALUES ('m', 'agent:a', 'p2')",
                [],
            );
            assert!(second.is_err(), "the primary key keeps an agent in one profile");
            Ok(())
        })
        .expect("write");
    })
    .await
    .expect("join");
}

#[tokio::test]
async fn deleting_a_row_leaves_a_delete_marker() {
    tokio::task::spawn_blocking(|| {
        let dir = tempfile::tempdir().expect("tempdir");
        let db = open(
            dir.path(),
            "agent-profiles-marker",
            true,
            holzi_migration_source(),
        );
        db.with_connection(|conn| {
            conn.execute(
                "INSERT INTO haex_agent_profile_members (id, subject, profile_id) VALUES ('m', 'agent:a', 'p1')",
                [],
            )?;
            conn.execute("DELETE FROM haex_agent_profile_members WHERE id = 'm'", [])?;
            let markers: i64 = conn.query_row(
                "SELECT COUNT(*) FROM haex_deleted_rows WHERE table_name = 'haex_agent_profile_members'",
                [],
                |row| row.get(0),
            )?;
            assert_eq!(markers, 1, "a synced delete leaves a marker");
            Ok(())
        })
        .expect("write");
    })
    .await
    .expect("join");
}
