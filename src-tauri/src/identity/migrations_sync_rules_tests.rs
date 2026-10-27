//! Migration tests for `0044_sync_rules` (spec 061, data-model.md): a fresh vault and a vault from the
//! schema before end with the five tables; none is tracked by haex-crdt (ADR 0018), and deleting a rule
//! or its device takes everything of it along.

// These tests read `sqlite_master` and write rows directly, which the CRDT write path does not expose.
#![allow(clippy::disallowed_methods)]

use haex_crdt::rusqlite::params;
use haex_crdt::Database;

use super::migrations_extensions_tests::has_hlc_column;
use super::migrations_sync_rules::TABLES;
use super::migrations_tests::{device_of, migration_source_before, open, table_exists};
use crate::identity::holzi_migration_source;

fn assert_schema(db: &Database) {
    for table in TABLES {
        assert!(table_exists(db, table), "{table} exists");
        assert!(
            !has_hlc_column(db, table),
            "{table} stays on this device (ADR 0018)"
        );
    }
}

fn count(db: &Database, table: &str) -> i64 {
    db.with_connection(|conn| {
        Ok(conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))?)
    })
    .expect("count")
}

/// One rule of `device` with a state row, a pending deletion, a run and a run item.
fn fill(db: &Database, device: &str) {
    db.with_connection(|conn| {
        conn.execute(
            "INSERT INTO haex_sync_rules_no_sync (id, vault_device_uuid, name, enabled, \
             place_a_source, place_a_path, place_b_source, place_b_path, direction, delete_mode, \
             interval_secs, created_at, updated_at) \
             VALUES ('r1', ?1, 'Fotos', 1, 'device', '/a', 'device', '/b', 'aToB', 'keep', 900, \
             '2026-10-26T00:00:00Z', '2026-10-26T00:00:00Z')",
            params![device],
        )?;
        conn.execute(
            "INSERT INTO haex_sync_rule_files_no_sync (rule_id, vault_device_uuid, path, a_size, \
             a_mtime_ms, b_size, b_mtime_ms) VALUES ('r1', ?1, 'x.jpg', 1, 2, 1, 3)",
            params![device],
        )?;
        conn.execute(
            "INSERT INTO haex_sync_rule_pending_no_sync (rule_id, vault_device_uuid, path, side, \
             found_run_id) VALUES ('r1', ?1, 'y.jpg', 'b', 'run1')",
            params![device],
        )?;
        conn.execute(
            "INSERT INTO haex_sync_runs_no_sync (id, rule_id, vault_device_uuid, started_ms, \
             duration_ms, cause, outcome, copied, moved, deleted, conflicts, errors, skipped) \
             VALUES ('run1', 'r1', ?1, 1, 2, 'manual', 'done', 1, 0, 0, 0, 0, 0)",
            params![device],
        )?;
        conn.execute(
            "INSERT INTO haex_sync_run_items_no_sync (run_id, seq, path, action, side) \
             VALUES ('run1', 0, 'x.jpg', 'copy', 'b')",
            [],
        )?;
        Ok(())
    })
    .expect("fill");
}

#[tokio::test]
async fn migration_0044_gives_a_fresh_vault_the_sync_rule_tables() {
    tokio::task::spawn_blocking(|| {
        let dir = tempfile::tempdir().expect("tempdir");
        let db = open(
            dir.path(),
            "sync-rules-fresh",
            true,
            holzi_migration_source(),
        );
        assert_schema(&db);
    })
    .await
    .expect("join");
}

#[tokio::test]
async fn migration_0044_upgrades_a_vault_from_the_schema_before_it() {
    tokio::task::spawn_blocking(|| {
        let dir = tempfile::tempdir().expect("tempdir");
        let old = open(
            dir.path(),
            "sync-rules-upgrade",
            true,
            migration_source_before("0044_sync_rules"),
        );
        for table in TABLES {
            assert!(!table_exists(&old, table), "{table} is new");
        }
        drop(old);
        let db = open(
            dir.path(),
            "sync-rules-upgrade",
            false,
            holzi_migration_source(),
        );
        assert_schema(&db);
    })
    .await
    .expect("join");
}

#[tokio::test]
async fn deleting_a_rule_takes_its_state_pending_and_log_along() {
    tokio::task::spawn_blocking(|| {
        let dir = tempfile::tempdir().expect("tempdir");
        let db = open(
            dir.path(),
            "sync-rules-cascade",
            true,
            holzi_migration_source(),
        );
        let device = device_of(&db, dir.path()).to_string();
        fill(&db, &device);
        db.with_connection(|conn| {
            conn.execute("DELETE FROM haex_sync_rules_no_sync WHERE id = 'r1'", [])?;
            Ok(())
        })
        .expect("delete");
        for table in TABLES {
            assert_eq!(count(&db, table), 0, "{table} is empty after the rule went");
        }
    })
    .await
    .expect("join");
}

#[tokio::test]
async fn deleting_the_device_takes_its_rules_along() {
    tokio::task::spawn_blocking(|| {
        let dir = tempfile::tempdir().expect("tempdir");
        let db = open(
            dir.path(),
            "sync-rules-device",
            true,
            holzi_migration_source(),
        );
        let device = device_of(&db, dir.path()).to_string();
        fill(&db, &device);
        db.with_connection(|conn| {
            conn.execute(
                "DELETE FROM known_devices WHERE vault_device_uuid = ?1",
                params![device],
            )?;
            Ok(())
        })
        .expect("delete");
        for table in TABLES {
            assert_eq!(
                count(&db, table),
                0,
                "{table} is empty after the device went"
            );
        }
    })
    .await
    .expect("join");
}
