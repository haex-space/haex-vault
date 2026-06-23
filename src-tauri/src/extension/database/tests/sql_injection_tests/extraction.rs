//! Sub-query table-name extraction.
//!
//! `extract_table_names_from_sql` must surface tables referenced from
//! nested SELECTs inside UPDATE-SET, DELETE-WHERE, and INSERT-SELECT
//! clauses — otherwise the permission checker only sees the outer
//! statement's target and a system table reference slips through.

use crate::database::core::extract_table_names_from_sql;

#[test]
fn test_subquery_in_update_set_clause_detected() {
    let sql =
        "UPDATE test_pub__test_ext__users SET name = (SELECT did FROM haex_identities LIMIT 1)";
    let tables = extract_table_names_from_sql(sql).unwrap();
    assert!(
        tables.iter().any(|t| t.contains("haex_identities")),
        "Should detect haex_identities in subquery. Found: {:?}",
        tables
    );
}

#[test]
fn test_subquery_in_delete_where_clause_detected() {
    let sql = "DELETE FROM test_pub__test_ext__users WHERE id IN (SELECT id FROM haex_extensions)";
    let tables = extract_table_names_from_sql(sql).unwrap();
    assert!(
        tables.iter().any(|t| t.contains("haex_extensions")),
        "Should detect haex_extensions in subquery. Found: {:?}",
        tables
    );
}

#[test]
fn test_subquery_in_insert_select_detected() {
    let sql = "INSERT INTO test_pub__test_ext__users (name) SELECT did FROM haex_identities";
    let tables = extract_table_names_from_sql(sql).unwrap();
    assert!(
        tables.iter().any(|t| t.contains("haex_identities")),
        "Should detect haex_identities in INSERT...SELECT. Found: {:?}",
        tables
    );
}
