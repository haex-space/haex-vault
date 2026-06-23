//! Edge cases that don't fit a single injection vector:
//! empty / whitespace inputs, pathologically long SQL, deeply nested
//! sub-selects, reserved-word identifiers, ORDER BY CASE tricks, and
//! time-based blind injection payloads.

use crate::database::core::parse_sql_statements;

#[test]
fn test_time_based_injection() {
    // Time-based blind injection is harder to prevent at parsing level
    // but the statement type restrictions help
    let attacks = [
        "SELECT * FROM users WHERE id=1 AND (SELECT CASE WHEN (1=1) THEN randomblob(100000000) ELSE 1 END)",
        "SELECT * FROM users WHERE id=1 AND 1=(SELECT CASE WHEN 1=1 THEN 1 ELSE 0 END)",
    ];

    for sql in attacks {
        let result = parse_sql_statements(sql);
        println!(
            "Time-based injection '{}' parse result: {:?}",
            sql.chars().take(50).collect::<String>(),
            result.is_ok()
        );
    }
}

#[test]
fn test_order_by_injection() {
    let attacks = [
        "SELECT * FROM users ORDER BY (CASE WHEN 1=1 THEN id ELSE name END)",
        "SELECT * FROM users ORDER BY IF(1=1,id,name)",
    ];

    for sql in attacks {
        let _ = parse_sql_statements(sql);
    }
}

#[test]
fn test_empty_string_handling() {
    let inputs = ["", " ", "\n", "\t", "   \n\t  "];
    for input in inputs {
        let result = parse_sql_statements(input);
        // Empty/whitespace should fail or return empty
        match result {
            Ok(stmts) => assert!(stmts.is_empty()),
            Err(_) => {} // Parse error is also acceptable
        }
    }
}

#[test]
fn test_very_long_sql() {
    // Very long SQL string to test buffer handling
    let long_table = "a".repeat(1000);
    let sql = format!("SELECT * FROM {}", long_table);
    let _ = parse_sql_statements(&sql);
}

#[test]
fn test_deeply_nested_subqueries() {
    let sql = "SELECT * FROM (SELECT * FROM (SELECT * FROM (SELECT * FROM users)))";
    let result = parse_sql_statements(sql);
    println!("Nested subqueries parse result: {:?}", result.is_ok());
}

#[test]
fn test_sql_with_special_identifiers() {
    // SQL reserved words as identifiers
    let sql = r#"SELECT "select", "from", "where" FROM "table""#;
    let _ = parse_sql_statements(sql);
}
