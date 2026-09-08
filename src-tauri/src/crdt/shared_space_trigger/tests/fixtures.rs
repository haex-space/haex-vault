//! Shared in-memory fixtures for the shared-space trigger tests.
//!
//! The schemas here mirror production (migration 0000 for
//! `haex_deleted_rows`, 0013 for the register plus the per-space
//! delete-log) so a divergence in column names shows up immediately.

use super::super::*;
use rusqlite::functions::FunctionFlags;
use rusqlite::Connection;
use std::sync::atomic::{AtomicU64, Ordering};

fn register_shared_space_test_udfs(conn: &Connection) {
    // Fresh test UDFs: gen_uuid returns unique strings, current_hlc returns
    // a monotonically increasing string so LWW ordering is well-defined.
    static UUID_COUNTER: AtomicU64 = AtomicU64::new(0);
    static HLC_COUNTER: AtomicU64 = AtomicU64::new(0);

    conn.create_scalar_function(
        UUID_FUNCTION_NAME,
        0,
        FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_INNOCUOUS,
        |_| {
            Ok(format!(
                "test-uuid-{}",
                UUID_COUNTER.fetch_add(1, Ordering::Relaxed)
            ))
        },
    )
    .expect("register gen_uuid");
    conn.create_scalar_function(
        HLC_FUNCTION_NAME,
        0,
        FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_INNOCUOUS,
        |_| {
            Ok(format!(
                "hlc-{:016}",
                HLC_COUNTER.fetch_add(1, Ordering::Relaxed)
            ))
        },
    )
    .expect("register current_hlc");
}

/// Builds a bare in-memory DB with the minimal CRDT plumbing needed to
/// exercise the register-DELETE fanout trigger. Doesn't touch the real
/// migration file — the schemas here match production so a divergence in
/// column names shows up immediately.
pub(super) fn setup_register_delete_fixture() -> Connection {
    let conn = Connection::open_in_memory().expect("open in-memory db");
    register_shared_space_test_udfs(&conn);

    // Bookkeeping tables the triggers read/write.
    conn.execute_batch(
        "CREATE TABLE haex_crdt_configs_no_sync (
             key TEXT PRIMARY KEY NOT NULL,
             value TEXT,
             type TEXT
         );
         CREATE TABLE haex_crdt_dirty_tables_no_sync (
             table_name TEXT PRIMARY KEY NOT NULL,
             last_modified TEXT
         );
         INSERT INTO haex_crdt_configs_no_sync (key, value, type)
         VALUES ('triggers_enabled', '1', 'boolean');",
    )
    .unwrap();

    // Owner-domain delete-log (same shape as production 0000 migration
    // plus CRDT meta cols the transformer injects at CREATE-TABLE time).
    conn.execute_batch(
        "CREATE TABLE haex_deleted_rows (
             id TEXT PRIMARY KEY NOT NULL,
             table_name TEXT NOT NULL,
             row_pks TEXT NOT NULL,
             haex_hlc_no_sync TEXT,
             haex_column_hlcs_no_sync TEXT NOT NULL DEFAULT '{}',
             haex_column_sigs_no_sync TEXT NOT NULL DEFAULT '{}'
         );",
    )
    .unwrap();

    // Register table.
    conn.execute_batch(
        "CREATE TABLE haex_shared_space_sync (
             id TEXT PRIMARY KEY NOT NULL,
             table_name TEXT NOT NULL,
             row_pks TEXT NOT NULL,
             space_id TEXT NOT NULL,
             haex_hlc_no_sync TEXT,
             haex_column_hlcs_no_sync TEXT NOT NULL DEFAULT '{}',
             haex_column_sigs_no_sync TEXT NOT NULL DEFAULT '{}'
         );",
    )
    .unwrap();

    // Shared-space-domain delete-log (Migration 0013 + CRDT meta cols).
    conn.execute_batch(
        "CREATE TABLE haex_shared_space_deleted_rows (
             id TEXT PRIMARY KEY NOT NULL,
             space_id TEXT NOT NULL,
             table_name TEXT NOT NULL,
             row_pks TEXT NOT NULL,
             haex_hlc_no_sync TEXT,
             haex_column_hlcs_no_sync TEXT NOT NULL DEFAULT '{}',
             haex_column_sigs_no_sync TEXT NOT NULL DEFAULT '{}'
         );",
    )
    .unwrap();

    let tx = conn.unchecked_transaction().unwrap();
    install_crdt_with_shared_space(&tx, "haex_shared_space_sync", false)
        .expect("register triggers");
    tx.commit().unwrap();

    conn
}

pub(super) fn trigger_exists(conn: &Connection, trigger_name: &str) -> bool {
    conn.query_row(
        "SELECT COUNT(*) > 0 FROM sqlite_master WHERE type = 'trigger' AND name = ?",
        [trigger_name],
        |row| row.get(0),
    )
    .unwrap()
}

pub(super) fn setup_business_delete_fixture() -> Connection {
    // Reuse the register-DELETE fixture (has all bookkeeping tables +
    // register triggers) and layer business tables on top.
    let conn = setup_register_delete_fixture();

    // A representative space-scoped infra table (path A) — schema mirrors
    // haex_peer_shares' relevant columns.
    conn.execute_batch(
        "CREATE TABLE haex_peer_shares (
             id TEXT PRIMARY KEY NOT NULL,
             space_id TEXT NOT NULL,
             name TEXT NOT NULL,
             haex_hlc_no_sync TEXT,
             haex_column_hlcs_no_sync TEXT NOT NULL DEFAULT '{}',
             haex_column_sigs_no_sync TEXT NOT NULL DEFAULT '{}'
         );",
    )
    .unwrap();

    // A representative extension table (path B) — arbitrary schema, no
    // space_id column; ownership lives in the register.
    conn.execute_batch(
        "CREATE TABLE ext_notes_items (
             id TEXT PRIMARY KEY NOT NULL,
             body TEXT,
             haex_hlc_no_sync TEXT,
             haex_column_hlcs_no_sync TEXT NOT NULL DEFAULT '{}',
             haex_column_sigs_no_sync TEXT NOT NULL DEFAULT '{}'
         );",
    )
    .unwrap();

    let tx = conn.unchecked_transaction().unwrap();
    install_crdt_with_shared_space(&tx, "haex_peer_shares", false).expect("infra triggers");
    install_crdt_with_shared_space(&tx, "ext_notes_items", false).expect("ext triggers");
    tx.commit().unwrap();

    conn
}

pub(super) fn delete_log_rows(conn: &Connection) -> Vec<(String, String, String)> {
    conn.prepare(
        "SELECT space_id, table_name, row_pks \
         FROM haex_shared_space_deleted_rows \
         ORDER BY space_id, table_name",
    )
    .unwrap()
    .query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
        ))
    })
    .unwrap()
    .collect::<Result<_, _>>()
    .unwrap()
}
