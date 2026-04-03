//! Tests for the CRDT transformer module
//!
//! These tests verify that the tombstone filter is correctly added to SELECT queries,
//! particularly ensuring that JOINs use qualified column names to avoid ambiguity.

use crate::crdt::transformer::CrdtTransformer;
use sqlparser::ast::Statement;
use sqlparser::dialect::SQLiteDialect;
use sqlparser::parser::Parser;
use uhlc::HLC;

fn parse_and_transform_query(sql: &str) -> String {
    let dialect = SQLiteDialect {};
    let mut statements = Parser::parse_sql(&dialect, sql).unwrap();
    let transformer = CrdtTransformer::new();

    if let Statement::Query(ref mut query) = statements[0] {
        transformer.transform_query(query);
    }

    statements[0].to_string()
}

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

    // Both tables are CRDT tables, so both should get tombstone filters
    assert!(
        result.contains(&tombstone_filter_qualified("i")),
        "Expected qualified tombstone filter with alias 'i' in: {}",
        result
    );
    assert!(
        result.contains(&tombstone_filter_qualified("c")),
        "Expected qualified tombstone filter with alias 'c' in: {}",
        result
    );
}

#[test]
fn test_select_with_join_no_alias_uses_table_name() {
    let sql = "SELECT items.*, categories.name FROM items JOIN categories ON items.category_id = categories.id";
    let result = parse_and_transform_query(sql);

    // Both tables are CRDT tables, so both should get tombstone filters using table names
    assert!(
        result.contains(&tombstone_filter_qualified("items")),
        "Expected qualified tombstone filter with table name 'items' in: {}",
        result
    );
    assert!(
        result.contains(&tombstone_filter_qualified("categories")),
        "Expected qualified tombstone filter with table name 'categories' in: {}",
        result
    );
}

#[test]
fn test_select_with_left_join_adds_qualified_tombstone_filter() {
    let sql = "SELECT a.*, b.value FROM accounts a LEFT JOIN balances b ON a.id = b.account_id";
    let result = parse_and_transform_query(sql);

    // Both tables are CRDT tables, so both should get tombstone filters
    assert!(
        result.contains(&tombstone_filter_qualified("a")),
        "Expected qualified tombstone filter with alias 'a' in: {}",
        result
    );
    assert!(
        result.contains(&tombstone_filter_qualified("b")),
        "Expected qualified tombstone filter with alias 'b' in: {}",
        result
    );
}

#[test]
fn test_select_with_multiple_joins_filters_all_crdt_tables() {
    let sql = "SELECT p.*, u.name, c.title FROM posts p JOIN users u ON p.user_id = u.id JOIN categories c ON p.category_id = c.id";
    let result = parse_and_transform_query(sql);

    // All three tables are CRDT tables, so all should get tombstone filters
    assert!(
        result.contains(&tombstone_filter_qualified("p")),
        "Expected qualified tombstone filter with alias 'p' in: {}",
        result
    );
    assert!(
        result.contains(&tombstone_filter_qualified("u")),
        "Expected qualified tombstone filter with alias 'u' in: {}",
        result
    );
    assert!(
        result.contains(&tombstone_filter_qualified("c")),
        "Expected qualified tombstone filter with alias 'c' in: {}",
        result
    );
    let ifnull_count = result.matches("IFNULL").count();
    assert_eq!(
        ifnull_count, 3,
        "All three CRDT tables should have tombstone filters: {}",
        result
    );
}

#[test]
fn test_select_excludes_no_sync_tables() {
    let sql = "SELECT * FROM haex_crdt_configs_no_sync";
    let result = parse_and_transform_query(sql);

    // Should NOT add tombstone filter for _no_sync tables
    assert!(
        !result.contains("haex_tombstone"),
        "Should not add tombstone filter for _no_sync table: {}",
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

    // Both tables are CRDT tables, should add tombstone filter for each AND preserve existing WHERE
    assert!(
        result.contains(&tombstone_filter_qualified("i")),
        "Expected qualified tombstone filter for 'i' in: {}",
        result
    );
    assert!(
        result.contains(&tombstone_filter_qualified("c")),
        "Expected qualified tombstone filter for 'c' in: {}",
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

    // Both tables are CRDT tables, so both should get tombstone filters
    assert!(
        result.contains(&tombstone_filter_qualified("a")),
        "Expected qualified tombstone filter with alias 'a' in: {}",
        result
    );
    assert!(
        result.contains(&tombstone_filter_qualified("b")),
        "Expected qualified tombstone filter with alias 'b' in: {}",
        result
    );
}

#[test]
fn test_cross_join_adds_qualified_tombstone_filter() {
    let sql = "SELECT a.*, b.* FROM items a CROSS JOIN tags b";
    let result = parse_and_transform_query(sql);

    // Both tables are CRDT tables, so both should get tombstone filters
    assert!(
        result.contains(&tombstone_filter_qualified("a")),
        "Expected qualified tombstone filter with alias 'a' in: {}",
        result
    );
    assert!(
        result.contains(&tombstone_filter_qualified("b")),
        "Expected qualified tombstone filter with alias 'b' in: {}",
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

// =============================================================================
// CREATE INDEX TRANSFORMATION TESTS
// =============================================================================

#[test]
fn test_unique_index_adds_tombstone_predicate() {
    let sql = "CREATE UNIQUE INDEX idx_items_name ON items(name)";
    let result = parse_and_transform_execute(sql);

    // Should add WHERE IFNULL(haex_tombstone, 0) <> 1
    assert!(
        result.contains(TOMBSTONE_FILTER_UNQUALIFIED),
        "Unique index should get tombstone predicate: {}",
        result
    );
}

#[test]
fn test_non_unique_index_no_tombstone_predicate() {
    let sql = "CREATE INDEX idx_items_name ON items(name)";
    let result = parse_and_transform_execute(sql);

    // Should NOT add tombstone predicate for non-unique indices
    assert!(
        !result.contains("haex_tombstone"),
        "Non-unique index should not get tombstone predicate: {}",
        result
    );
}

#[test]
fn test_unique_index_with_existing_predicate_merges() {
    let sql = "CREATE UNIQUE INDEX idx_items_active ON items(name) WHERE active = 1";
    let result = parse_and_transform_execute(sql);

    // Should keep original predicate AND add tombstone filter
    assert!(
        result.contains("active = 1"),
        "Should preserve original predicate: {}",
        result
    );
    assert!(
        result.contains(TOMBSTONE_FILTER_UNQUALIFIED),
        "Should add tombstone predicate: {}",
        result
    );
}

#[test]
fn test_unique_index_with_existing_tombstone_condition_not_duplicated() {
    let sql = "CREATE UNIQUE INDEX idx_items_name ON items(name) WHERE haex_tombstone = 0";
    let result = parse_and_transform_execute(sql);

    // Should NOT add another tombstone condition
    let tombstone_count = result.matches("haex_tombstone").count();
    assert_eq!(
        tombstone_count, 1,
        "Should not duplicate tombstone condition: {}",
        result
    );
}

#[test]
fn test_unique_index_on_no_sync_internal_table_no_predicate() {
    let sql = "CREATE UNIQUE INDEX idx_crdt_test ON haex_crdt_configs_no_sync(key)";
    let result = parse_and_transform_execute(sql);

    // Should NOT add tombstone predicate for _no_sync tables
    assert!(
        !result.contains("IFNULL"),
        "_no_sync table index should not get tombstone predicate: {}",
        result
    );
}

#[test]
fn test_unique_index_multi_column() {
    let sql = "CREATE UNIQUE INDEX idx_items_compound ON items(category_id, name)";
    let result = parse_and_transform_execute(sql);

    // Should add tombstone predicate
    assert!(
        result.contains(TOMBSTONE_FILTER_UNQUALIFIED),
        "Multi-column unique index should get tombstone predicate: {}",
        result
    );
}

// =============================================================================
// NO_SYNC SUFFIX TESTS
// =============================================================================

#[test]
fn test_no_sync_suffix_excludes_from_crdt_select() {
    let sql = "SELECT * FROM items_no_sync";
    let result = parse_and_transform_query(sql);

    // Should NOT add tombstone filter for _no_sync tables
    assert!(
        !result.contains("haex_tombstone"),
        "No-sync table should not get tombstone filter: {}",
        result
    );
}

#[test]
fn test_regular_table_gets_crdt() {
    let sql = "SELECT * FROM items";
    let result = parse_and_transform_query(sql);

    // Should add tombstone filter for regular tables
    assert!(
        result.contains(TOMBSTONE_FILTER_UNQUALIFIED),
        "Regular table should get tombstone filter: {}",
        result
    );
}

#[test]
fn test_unique_index_on_no_sync_table_no_predicate() {
    let sql = "CREATE UNIQUE INDEX idx_test ON cache_data_no_sync(key)";
    let result = parse_and_transform_execute(sql);

    // Should NOT add tombstone predicate for _no_sync tables
    assert!(
        !result.contains("IFNULL"),
        "No-sync table index should not get tombstone predicate: {}",
        result
    );
}

// =============================================================================
// JOIN WITH MIXED CRDT / NO_SYNC TABLES
// =============================================================================

#[test]
fn test_join_crdt_with_no_sync_table_only_filters_crdt() {
    let sql = "SELECT a.*, b.value FROM items a JOIN cache_no_sync b ON a.id = b.item_id";
    let result = parse_and_transform_query(sql);

    // Only 'items' (alias 'a') is a CRDT table; cache_no_sync should not get a filter
    assert!(
        result.contains(&tombstone_filter_qualified("a")),
        "Expected qualified tombstone filter for CRDT table 'a' in: {}",
        result
    );
    let ifnull_count = result.matches("IFNULL").count();
    assert_eq!(
        ifnull_count, 1,
        "Only one CRDT table should have tombstone filter: {}",
        result
    );
}

#[test]
fn test_join_no_sync_main_with_crdt_joined_table() {
    let sql =
        "SELECT a.*, b.name FROM local_cache_no_sync a JOIN items b ON a.item_id = b.id";
    let result = parse_and_transform_query(sql);

    // Only 'items' (alias 'b') is a CRDT table; the main table is _no_sync
    assert!(
        result.contains(&tombstone_filter_qualified("b")),
        "Expected qualified tombstone filter for joined CRDT table 'b' in: {}",
        result
    );
    assert!(
        !result.contains(&tombstone_filter_qualified("a")),
        "Should not have tombstone filter for no_sync table 'a' in: {}",
        result
    );
    let ifnull_count = result.matches("IFNULL").count();
    assert_eq!(
        ifnull_count, 1,
        "Only the CRDT joined table should have tombstone filter: {}",
        result
    );
}

#[test]
fn test_join_both_no_sync_tables_no_filter() {
    let sql = "SELECT a.*, b.value FROM cache_no_sync a JOIN session_no_sync b ON a.id = b.cache_id";
    let result = parse_and_transform_query(sql);

    // Neither table is a CRDT table, so no tombstone filter should be added
    assert!(
        !result.contains("IFNULL"),
        "No tombstone filter should be added for two no_sync tables: {}",
        result
    );
}

#[test]
fn test_multiple_joins_mixed_crdt_and_no_sync() {
    let sql = "SELECT p.*, u.name, c.data FROM posts p JOIN users u ON p.user_id = u.id JOIN settings_no_sync c ON p.id = c.post_id";
    let result = parse_and_transform_query(sql);

    // posts (p) and users (u) are CRDT; settings_no_sync (c) is not
    assert!(
        result.contains(&tombstone_filter_qualified("p")),
        "Expected tombstone filter for CRDT table 'p' in: {}",
        result
    );
    assert!(
        result.contains(&tombstone_filter_qualified("u")),
        "Expected tombstone filter for CRDT table 'u' in: {}",
        result
    );
    let ifnull_count = result.matches("IFNULL").count();
    assert_eq!(
        ifnull_count, 2,
        "Only two CRDT tables should have tombstone filters: {}",
        result
    );
}
