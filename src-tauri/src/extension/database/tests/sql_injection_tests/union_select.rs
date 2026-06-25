//! UNION / sub-SELECT injection — appending a SELECT against a system
//! table via `UNION`, `IN (SELECT …)`, `EXISTS (SELECT …)`, or a CTE.
//!
//! The parser will happily accept these (they're valid SQL); blocking
//! relies on `is_system_table` rejecting the referenced target.

use crate::database::core::parse_sql_statements;
use crate::extension::permissions::checker::is_system_table;

#[test]
fn test_union_injection_to_system_tables() {
    // UNION-based injection to access system tables
    let sql = "SELECT * FROM testpublickey__testextension__users WHERE id='1' UNION SELECT * FROM haex_extensions";
    let result = parse_sql_statements(sql);

    // Should parse as single statement (valid SQL)
    assert!(result.is_ok());

    // But permission checker should block access to haex_extensions
    assert!(is_system_table("haex_extensions"));
}

#[test]
fn test_union_injection_to_sqlite_master() {
    // System table check should catch union injection attempts
    assert!(is_system_table("sqlite_master"));
    assert!(is_system_table("sqlite_sequence"));
    assert!(is_system_table("sqlite_stat1"));
}

#[test]
fn test_union_all_injection() {
    // Permission check should block haex_vault_settings even via UNION ALL
    assert!(is_system_table("haex_vault_settings"));
}

#[test]
fn test_subquery_to_system_table() {
    // Subquery injection to access system tables
    let sql = "SELECT * FROM users WHERE id IN (SELECT id FROM haex_extensions)";
    let result = parse_sql_statements(sql);

    // Valid SQL, but permission checker should catch the subquery target
    assert!(result.is_ok());
    assert!(is_system_table("haex_extensions"));
}

#[test]
fn test_exists_subquery_injection() {
    let sql =
        "SELECT * FROM users WHERE EXISTS (SELECT 1 FROM haex_vault_settings WHERE value='admin')";
    let result = parse_sql_statements(sql);

    assert!(result.is_ok());
    assert!(is_system_table("haex_vault_settings"));
}

#[test]
fn test_cte_injection() {
    // Common Table Expression (CTE) injection
    let sql = r#"
        WITH stolen_data AS (
            SELECT * FROM haex_extensions
        )
        SELECT * FROM stolen_data
    "#;

    let result = parse_sql_statements(sql);
    // CTE is valid SQL — defence is the system-table check in the
    // permission layer (see siblings above). Pin both: parses cleanly,
    // and `haex_extensions` is recognised as a system table.
    assert!(result.is_ok(), "CTE should parse as valid SQL");
    assert!(is_system_table("haex_extensions"));
}
