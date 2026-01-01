//! Tests for the CRDT transformer module
//!
//! These tests verify that the tombstone filter is correctly added to SELECT queries,
//! particularly ensuring that JOINs use qualified column names to avoid ambiguity.

use crate::crdt::transformer::CrdtTransformer;
use sqlparser::ast::Statement;
use sqlparser::dialect::SQLiteDialect;
use sqlparser::parser::Parser;

fn parse_and_transform_query(sql: &str) -> String {
    let dialect = SQLiteDialect {};
    let mut statements = Parser::parse_sql(&dialect, sql).unwrap();
    let transformer = CrdtTransformer::new();

    if let Statement::Query(ref mut query) = statements[0] {
        transformer.transform_query(query);
    }

    statements[0].to_string()
}

// Note: sqlparser outputs != as <> (SQL standard)
const TOMBSTONE_FILTER_UNQUALIFIED: &str = "IFNULL(haex_tombstone, 0) <> 1";

fn tombstone_filter_qualified(qualifier: &str) -> String {
    // Note: sqlparser outputs identifiers with double quotes when they were created with with_quote
    format!("IFNULL(\"{}\".haex_tombstone, 0) <> 1", qualifier)
}

#[test]
fn test_simple_select_adds_tombstone_filter() {
    let sql = "SELECT * FROM items";
    let result = parse_and_transform_query(sql);

    // Should add IFNULL(haex_tombstone, 0) <> 1 without table qualifier
    assert!(
        result.contains(TOMBSTONE_FILTER_UNQUALIFIED),
        "Expected tombstone filter in: {}",
        result
    );
    // Should NOT have table qualifier for simple queries
    assert!(
        !result.contains("items.haex_tombstone"),
        "Should not have table qualifier for simple query: {}",
        result
    );
}

#[test]
fn test_select_with_existing_where_adds_tombstone_filter() {
    let sql = "SELECT * FROM items WHERE title = 'test'";
    let result = parse_and_transform_query(sql);

    // Should add tombstone filter with AND
    assert!(
        result.contains(TOMBSTONE_FILTER_UNQUALIFIED),
        "Expected tombstone filter in: {}",
        result
    );
    assert!(
        result.contains("title = 'test'"),
        "Should preserve original WHERE: {}",
        result
    );
}

#[test]
fn test_select_with_join_adds_qualified_tombstone_filter() {
    let sql = "SELECT i.*, c.name FROM items i JOIN categories c ON i.category_id = c.id";
    let result = parse_and_transform_query(sql);

    // Should add tombstone filter WITH table qualifier (alias 'i')
    assert!(
        result.contains(&tombstone_filter_qualified("i")),
        "Expected qualified tombstone filter with alias 'i' in: {}",
        result
    );
}

#[test]
fn test_select_with_join_no_alias_uses_table_name() {
    let sql = "SELECT items.*, categories.name FROM items JOIN categories ON items.category_id = categories.id";
    let result = parse_and_transform_query(sql);

    // Should add tombstone filter WITH table name qualifier
    assert!(
        result.contains(&tombstone_filter_qualified("items")),
        "Expected qualified tombstone filter with table name 'items' in: {}",
        result
    );
}

#[test]
fn test_select_with_left_join_adds_qualified_tombstone_filter() {
    let sql = "SELECT a.*, b.value FROM accounts a LEFT JOIN balances b ON a.id = b.account_id";
    let result = parse_and_transform_query(sql);

    // Should add tombstone filter for the main table (accounts with alias 'a')
    assert!(
        result.contains(&tombstone_filter_qualified("a")),
        "Expected qualified tombstone filter with alias 'a' in: {}",
        result
    );
}

#[test]
fn test_select_with_multiple_joins_uses_first_table() {
    let sql = "SELECT p.*, u.name, c.title FROM posts p JOIN users u ON p.user_id = u.id JOIN categories c ON p.category_id = c.id";
    let result = parse_and_transform_query(sql);

    // Should use the first (main) table's alias 'p'
    assert!(
        result.contains(&tombstone_filter_qualified("p")),
        "Expected qualified tombstone filter with alias 'p' in: {}",
        result
    );
}

#[test]
fn test_select_excludes_crdt_internal_tables() {
    let sql = "SELECT * FROM haex_crdt_changes";
    let result = parse_and_transform_query(sql);

    // Should NOT add tombstone filter for internal CRDT tables
    assert!(
        !result.contains("haex_tombstone"),
        "Should not add tombstone filter for internal CRDT table: {}",
        result
    );
}

#[test]
fn test_select_with_existing_tombstone_condition_does_not_duplicate() {
    let sql = "SELECT * FROM items WHERE haex_tombstone = 1";
    let result = parse_and_transform_query(sql);

    // Should NOT add another tombstone filter when one already exists
    let tombstone_count = result.matches("haex_tombstone").count();
    assert_eq!(
        tombstone_count, 1,
        "Should not duplicate tombstone condition: {}",
        result
    );
}

#[test]
fn test_subquery_also_gets_tombstone_filter() {
    let sql = "SELECT * FROM (SELECT * FROM items) AS sub";
    let result = parse_and_transform_query(sql);

    // The inner SELECT should get the tombstone filter
    assert!(
        result.contains(TOMBSTONE_FILTER_UNQUALIFIED),
        "Expected tombstone filter in subquery: {}",
        result
    );
}

#[test]
fn test_union_both_selects_get_tombstone_filter() {
    let sql = "SELECT id, title FROM items UNION SELECT id, name FROM categories";
    let result = parse_and_transform_query(sql);

    // Both SELECTs should get tombstone filters
    let ifnull_count = result.matches("IFNULL").count();
    assert_eq!(
        ifnull_count, 2,
        "Both UNION parts should have tombstone filters: {}",
        result
    );
}

#[test]
fn test_tombstone_filter_handles_null_and_zero() {
    // The IFNULL(haex_tombstone, 0) <> 1 approach:
    // - haex_tombstone = 0 → IFNULL(0, 0) = 0 → 0 <> 1 → TRUE (included)
    // - haex_tombstone = NULL → IFNULL(NULL, 0) = 0 → 0 <> 1 → TRUE (included)
    // - haex_tombstone = 1 → IFNULL(1, 0) = 1 → 1 <> 1 → FALSE (excluded)
    let sql = "SELECT * FROM items";
    let result = parse_and_transform_query(sql);

    // Verify the exact filter format
    assert!(
        result.contains(TOMBSTONE_FILTER_UNQUALIFIED),
        "Filter should use IFNULL pattern: {}",
        result
    );
}

#[test]
fn test_join_with_where_clause_adds_qualified_filter() {
    let sql = "SELECT i.*, c.name FROM items i JOIN categories c ON i.category_id = c.id WHERE i.title LIKE '%test%'";
    let result = parse_and_transform_query(sql);

    // Should add qualified tombstone filter AND preserve existing WHERE
    assert!(
        result.contains(&tombstone_filter_qualified("i")),
        "Expected qualified tombstone filter in: {}",
        result
    );
    assert!(
        result.contains("i.title LIKE '%test%'"),
        "Should preserve original WHERE clause: {}",
        result
    );
}

#[test]
fn test_right_join_adds_qualified_tombstone_filter() {
    let sql = "SELECT a.*, b.value FROM items a RIGHT JOIN related b ON a.id = b.item_id";
    let result = parse_and_transform_query(sql);

    // Should add tombstone filter for the first table (items with alias 'a')
    assert!(
        result.contains(&tombstone_filter_qualified("a")),
        "Expected qualified tombstone filter with alias 'a' in: {}",
        result
    );
}

#[test]
fn test_cross_join_adds_qualified_tombstone_filter() {
    let sql = "SELECT a.*, b.* FROM items a CROSS JOIN tags b";
    let result = parse_and_transform_query(sql);

    // Should add qualified tombstone filter for the main table
    assert!(
        result.contains(&tombstone_filter_qualified("a")),
        "Expected qualified tombstone filter in: {}",
        result
    );
}

#[test]
fn test_deeply_nested_subquery() {
    let sql = "SELECT * FROM (SELECT * FROM (SELECT * FROM items) AS inner_sub) AS outer_sub";
    let result = parse_and_transform_query(sql);

    // The innermost SELECT should get the tombstone filter
    assert!(
        result.contains(TOMBSTONE_FILTER_UNQUALIFIED),
        "Expected tombstone filter in deeply nested subquery: {}",
        result
    );
}

#[test]
fn test_subquery_in_join() {
    let sql = "SELECT a.*, sub.cnt FROM items a JOIN (SELECT category_id, COUNT(*) as cnt FROM items GROUP BY category_id) sub ON a.category_id = sub.category_id";
    let result = parse_and_transform_query(sql);

    // Both the outer query (with qualified filter) and the subquery should get tombstone filters
    assert!(
        result.contains(&tombstone_filter_qualified("a")),
        "Outer query should have qualified tombstone filter: {}",
        result
    );
    // The subquery inside the JOIN should also have a tombstone filter
    let ifnull_count = result.matches("IFNULL").count();
    assert_eq!(
        ifnull_count, 2,
        "Both outer and inner queries should have tombstone filters: {}",
        result
    );
}
