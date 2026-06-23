//! Multiple-statement / stacked-query injection.
//!
//! Classic vector: end one statement with `;` and start another to smuggle
//! a DROP / DELETE / INSERT past the parser. Also covers the
//! LIMIT-then-stacked variation.

use crate::database::core::parse_sql_statements;
use crate::extension::database::planner::SqlExecutionPlanner;

#[test]
fn test_reject_multiple_statements_semicolon() {
    // Classic SQL injection: end statement and start a new one
    let attacks = [
        "SELECT * FROM users; DROP TABLE haex_extensions; --",
        "SELECT * FROM t WHERE id=1; DELETE FROM haex_vault_settings;",
        "INSERT INTO t VALUES (1);INSERT INTO haex_extensions VALUES ('malicious');",
        "UPDATE t SET x=1; UPDATE haex_vault_settings SET value='hacked';",
        "SELECT 1;SELECT * FROM haex_extensions;",
    ];

    for sql in attacks {
        let result = SqlExecutionPlanner::parse_single_statement(sql);
        assert!(
            result.is_err(),
            "Should reject multiple statements in: {}",
            sql
        );
    }
}

#[test]
fn test_reject_stacked_queries() {
    // Stacked query injection
    let result = parse_sql_statements(
        "SELECT * FROM users WHERE id=1; SELECT * FROM sqlite_master WHERE type='table'",
    );

    // Even if it parses, it should return multiple statements
    if let Ok(statements) = result {
        assert!(
            statements.len() > 1,
            "Parser should identify multiple statements"
        );
    }
}

#[test]
fn test_limit_injection() {
    let sql = "SELECT * FROM users LIMIT 1; SELECT * FROM haex_extensions; --";
    let result = SqlExecutionPlanner::parse_single_statement(sql);
    // Should reject multiple statements
    assert!(
        result.is_err() || {
            // If it parses, the second part should be identified as another statement
            if let Ok(stmts) = parse_sql_statements(sql) {
                stmts.len() > 1
            } else {
                true
            }
        }
    );
}
