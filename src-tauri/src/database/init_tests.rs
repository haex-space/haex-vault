//! Direct unit tests for [`super::discover_crdt_tables`].
//!
//! Regression guard for docs/plans/2026-07-21-haex-logs-no-sync.md (Task 1.4):
//! the security-load-bearing invariant is that `discover_crdt_tables` keys on
//! the presence of a `haex_hlc` column, so a table created without CRDT
//! columns (like the `_no_sync` log table shipped in migration 0009) is never
//! discovered and thus never pushed on the owner full-vault sync path.

use rusqlite::Connection;

use super::discover_crdt_tables;

/// The production `haex_logs_no_sync` column set (migration 0009), minus the
/// `haex_extensions` FK — the FK is irrelevant to the column-presence
/// invariant and dropping it avoids seeding an unrelated table.
const CREATE_HAEX_LOGS_NO_SYNC: &str = "CREATE TABLE haex_logs_no_sync (
    id TEXT PRIMARY KEY NOT NULL,
    timestamp TEXT NOT NULL,
    level TEXT NOT NULL,
    source TEXT NOT NULL,
    extension_id TEXT,
    message TEXT NOT NULL,
    metadata TEXT,
    device_id TEXT NOT NULL
)";

#[test]
fn discover_crdt_tables_excludes_haex_logs_no_sync() {
    let conn = Connection::open_in_memory().expect("open in-memory db");

    // A genuine CRDT-synced table (carries `haex_hlc`) as a positive control.
    conn.execute_batch("CREATE TABLE haex_items (id TEXT PRIMARY KEY, name TEXT, haex_hlc TEXT);")
        .expect("create synced table");

    // The no-sync log table, created without CRDT columns like production.
    conn.execute_batch(CREATE_HAEX_LOGS_NO_SYNC)
        .expect("create haex_logs_no_sync");

    let tables = discover_crdt_tables(&conn).expect("discover_crdt_tables");

    assert!(
        tables.iter().any(|t| t == "haex_items"),
        "positive control: a table with `haex_hlc` must be discovered, got {tables:?}"
    );
    assert!(
        !tables.iter().any(|t| t == "haex_logs_no_sync"),
        "haex_logs_no_sync must NOT be discovered — it has no `haex_hlc` column and \
         must stay device-local (docs/plans/2026-07-21-haex-logs-no-sync.md). Got {tables:?}"
    );
}

#[test]
fn haex_logs_no_sync_has_no_haex_hlc_column() {
    // Task 1.4 Step 3: PRAGMA table_info over the production DDL has no `haex_hlc`.
    let conn = Connection::open_in_memory().expect("open in-memory db");
    conn.execute_batch(CREATE_HAEX_LOGS_NO_SYNC)
        .expect("create haex_logs_no_sync");

    let mut stmt = conn
        .prepare("SELECT name FROM pragma_table_info('haex_logs_no_sync')")
        .expect("prepare pragma");
    let columns: Vec<String> = stmt
        .query_map([], |row| row.get(0))
        .expect("query pragma")
        .collect::<Result<_, _>>()
        .expect("collect columns");

    assert!(
        !columns.iter().any(|c| c == "haex_hlc"),
        "haex_logs_no_sync must not carry `haex_hlc`; columns = {columns:?}"
    );
    // Sanity: a base column is present, so the CREATE actually ran.
    assert!(
        columns.iter().any(|c| c == "message"),
        "columns = {columns:?}"
    );
}
