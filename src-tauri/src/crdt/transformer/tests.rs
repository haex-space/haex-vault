//! Tests for the CRDT transformer module.
//!
//! Since the delete-log refactor, the transformer no longer injects
//! `haex_tombstone` columns or tombstone filters. These tests verify the
//! remaining responsibilities:
//! - CREATE TABLE gets `haex_hlc` + `haex_column_hlcs` added
//! - CREATE UNIQUE INDEX stays untouched (no partial rewrite)
//! - DELETE stays a DELETE
//! - UPDATE gets the HLC timestamp assignment
//! - SELECT passes through, including recursion into subqueries

use crate::crdt::transformer::CrdtTransformer;
use sqlparser::ast::Statement;
use sqlparser::dialect::SQLiteDialect;
use sqlparser::parser::Parser;
use uhlc::HLC;

fn parse_and_transform_execute(sql: &str) -> String {
    let dialect = SQLiteDialect {};
    let mut statements = Parser::parse_sql(&dialect, sql).unwrap();
    let transformer = CrdtTransformer::new();
    let hlc = HLC::default();
    let timestamp = hlc.new_timestamp();

    transformer
        .transform_execute_statement(&mut statements[0], &timestamp)
        .unwrap();

    statements[0].to_string()
}

#[test]
fn test_select_no_longer_adds_tombstone_filter() {
    let result = parse_and_transform_execute("SELECT * FROM items");
    assert!(!result.contains("haex_tombstone"), "Got: {result}");
}

#[test]
fn test_delete_stays_delete() {
    let result = parse_and_transform_execute("DELETE FROM items WHERE id = 'x'");
    assert!(
        result.to_uppercase().starts_with("DELETE"),
        "DELETE must not be rewritten into UPDATE anymore. Got: {result}"
    );
    assert!(!result.contains("haex_tombstone"), "Got: {result}");
}

#[test]
fn test_update_adds_hlc_assignment() {
    let result = parse_and_transform_execute(
        "UPDATE items SET name = 'foo' WHERE id = 'x'",
    );
    assert!(
        result.contains("haex_hlc"),
        "UPDATE must add haex_hlc assignment. Got: {result}"
    );
    assert!(!result.contains("haex_tombstone"), "Got: {result}");
}

#[test]
fn test_create_table_adds_crdt_columns() {
    let result = parse_and_transform_execute(
        "CREATE TABLE items (id TEXT PRIMARY KEY, name TEXT)",
    );
    assert!(result.contains("haex_hlc"), "Got: {result}");
    assert!(result.contains("haex_column_hlcs"), "Got: {result}");
    assert!(
        !result.contains("haex_tombstone"),
        "haex_tombstone must not be added anymore. Got: {result}"
    );
}

#[test]
fn test_create_unique_index_is_not_rewritten_to_partial() {
    let result = parse_and_transform_execute(
        "CREATE UNIQUE INDEX idx_items_name ON items(name)",
    );
    assert!(
        !result.to_uppercase().contains("WHERE"),
        "UNIQUE index must stay full (no partial rewrite). Got: {result}"
    );
    assert!(!result.contains("haex_tombstone"), "Got: {result}");
}

#[test]
fn test_create_table_no_sync_skipped() {
    let result = parse_and_transform_execute(
        "CREATE TABLE my_cache_no_sync (id TEXT PRIMARY KEY, value TEXT)",
    );
    assert!(
        !result.contains("haex_hlc"),
        "_no_sync tables must not get CRDT columns. Got: {result}"
    );
}

#[test]
fn test_insert_into_sync_table_gets_hlc_column() {
    let result = parse_and_transform_execute(
        "INSERT INTO items (id, name) VALUES ('a', 'b')",
    );
    // InsertTransformer adds haex_hlc as a literal column/value
    assert!(result.contains("haex_hlc"), "Got: {result}");
}

#[test]
fn test_delete_from_sync_table_stays_delete() {
    let result = parse_and_transform_execute("DELETE FROM items WHERE id = 'a'");
    assert!(result.to_uppercase().contains("DELETE"));
    assert!(
        !result.to_uppercase().contains("UPDATE"),
        "DELETE must not be rewritten. Got: {result}"
    );
}
