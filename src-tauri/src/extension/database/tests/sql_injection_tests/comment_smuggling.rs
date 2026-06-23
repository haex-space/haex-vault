//! Comment- and boolean-based filter bypass.
//!
//! `--`, `#`, and `/* … */` are valid SQL and parse cleanly; the real
//! defence is parameterized queries. Boolean tautologies (`OR 1=1`,
//! `AND 1=1`) live here too because they're the same "the parser will let
//! it through, parameter binding is what saves you" category.

use crate::database::core::parse_sql_statements;

#[test]
fn test_comment_injection_single_line() {
    // Single-line comment to bypass filters
    let attacks = [
        "SELECT * FROM users--; DROP TABLE haex_extensions",
        "SELECT * FROM users -- DROP TABLE haex_extensions",
        "SELECT * FROM users # malicious comment",
    ];

    for sql in attacks {
        // Two safe outcomes:
        //   - Parser errors (rejects the injection)
        //   - Parser succeeds with EXACTLY 1 statement (DROP stays inside the comment)
        // Anything else (>=2 statements) would be a smuggling regression.
        match parse_sql_statements(sql) {
            Ok(stmts) => assert_eq!(
                stmts.len(),
                1,
                "Comment-smuggling payload smuggled a second statement past the parser: {sql} (got {} stmts)",
                stmts.len()
            ),
            Err(_) => { /* parser rejected the injection — also safe */ }
        }
    }
}

#[test]
fn test_comment_injection_multi_line() {
    // Multi-line comment injection
    let sql = "SELECT * FROM users /* DROP TABLE haex_extensions */ WHERE id=1";
    let result = parse_sql_statements(sql);
    // This is valid SQL - the DROP part is inside a comment
    assert!(result.is_ok());
}

#[test]
fn test_comment_injection_nested() {
    // Nested comment attempts — system table check still blocks
    use crate::extension::permissions::checker::is_system_table;
    assert!(is_system_table("haex_extensions"));
}

#[test]
fn test_boolean_injection_or() {
    let attacks = [
        "SELECT * FROM users WHERE id=1 OR 1=1",
        "SELECT * FROM users WHERE id=1 OR ''=''",
        "SELECT * FROM users WHERE id=1 OR true",
        "SELECT * FROM users WHERE name='' OR 1=1--'",
    ];

    for sql in attacks {
        let result = parse_sql_statements(sql);
        // These parse as valid SQL - protection comes from parameterized queries
        assert!(
            result.is_ok(),
            "Should parse (but be protected by params): {}",
            sql
        );
    }
}

#[test]
fn test_boolean_injection_and() {
    let attacks = [
        "SELECT * FROM users WHERE id=1 AND 1=1",
        "SELECT * FROM users WHERE id=1 AND 1=2",
        "SELECT * FROM users WHERE name='admin' AND password IS NOT NULL",
    ];

    for sql in attacks {
        let result = parse_sql_statements(sql);
        assert!(result.is_ok());
    }
}
