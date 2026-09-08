//! Column-management tests carried over from before the cutover.
//!
//! `ensure_crdt_columns` and `get_table_schema` are `haex_crdt`'s now and
//! are re-exported by the parent module; these keep asserting the contract
//! vault depends on (the three metadata columns get backfilled by
//! ALTER TABLE, idempotently, and a missing table is a no-op).

use super::super::*;
use rusqlite::Connection;

#[test]
fn test_ensure_crdt_columns_consistency_with_transformer() {
    let conn = Connection::open_in_memory().unwrap();

    conn.execute(
        "CREATE TABLE test_table (id TEXT PRIMARY KEY, name TEXT)",
        [],
    )
    .unwrap();

    let tx = conn.unchecked_transaction().unwrap();

    let result = ensure_crdt_columns(&tx, "test_table").unwrap();
    assert!(result, "Should have added columns");

    tx.commit().unwrap();

    let columns = get_table_schema(&conn, "test_table").unwrap();
    let column_names: Vec<&str> = columns.iter().map(|c| c.name.as_str()).collect();

    assert!(
        column_names.contains(&HLC_TIMESTAMP_COLUMN),
        "Missing {} column. Found: {:?}",
        HLC_TIMESTAMP_COLUMN,
        column_names
    );
    assert!(
        column_names.contains(&COLUMN_HLCS_COLUMN),
        "Missing {} column. Found: {:?}",
        COLUMN_HLCS_COLUMN,
        column_names
    );
    assert!(
        column_names.contains(&COLUMN_SIGS_COLUMN),
        "Missing {} column. Found: {:?}",
        COLUMN_SIGS_COLUMN,
        column_names
    );
}

#[test]
fn test_ensure_crdt_columns_idempotent() {
    let conn = Connection::open_in_memory().unwrap();

    conn.execute(
        &format!(
            "CREATE TABLE test_table (
                id TEXT PRIMARY KEY,
                name TEXT,
                {} TEXT,
                {} TEXT NOT NULL DEFAULT '{{}}',
                {} TEXT NOT NULL DEFAULT '{{}}'
            )",
            HLC_TIMESTAMP_COLUMN, COLUMN_HLCS_COLUMN, COLUMN_SIGS_COLUMN
        ),
        [],
    )
    .unwrap();

    let tx = conn.unchecked_transaction().unwrap();

    let result = ensure_crdt_columns(&tx, "test_table").unwrap();
    assert!(!result, "Should not have added any columns");

    tx.commit().unwrap();
}

#[test]
fn test_ensure_crdt_columns_partial() {
    let conn = Connection::open_in_memory().unwrap();

    conn.execute(
        &format!(
            "CREATE TABLE test_table (
                id TEXT PRIMARY KEY,
                name TEXT,
                {} TEXT
            )",
            HLC_TIMESTAMP_COLUMN
        ),
        [],
    )
    .unwrap();

    let tx = conn.unchecked_transaction().unwrap();

    let result = ensure_crdt_columns(&tx, "test_table").unwrap();
    assert!(result, "Should have added missing columns");

    tx.commit().unwrap();

    let columns = get_table_schema(&conn, "test_table").unwrap();
    let column_names: Vec<&str> = columns.iter().map(|c| c.name.as_str()).collect();

    assert!(column_names.contains(&HLC_TIMESTAMP_COLUMN));
    assert!(column_names.contains(&COLUMN_HLCS_COLUMN));
    assert!(column_names.contains(&COLUMN_SIGS_COLUMN));
}

#[test]
fn test_ensure_crdt_columns_nonexistent_table() {
    let conn = Connection::open_in_memory().unwrap();
    let tx = conn.unchecked_transaction().unwrap();

    // Should return false for non-existent table
    let result = ensure_crdt_columns(&tx, "nonexistent_table").unwrap();
    assert!(!result, "Should return false for non-existent table");
}
