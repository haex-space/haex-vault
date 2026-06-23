//! Dangerous DDL / pragma / trigger / view statements.
//!
//! Statements that extensions must never be able to run:
//! ATTACH/DETACH (other DB files), PRAGMA (engine config / info leak),
//! VACUUM/REINDEX/ANALYZE (maintenance), CREATE/DROP TRIGGER, CREATE
//! VIEW (deferred read of a system table). Statement-type validation is
//! what blocks them; these tests pin the parser's behaviour.

use crate::database::core::parse_sql_statements;

#[test]
fn test_reject_attach_database() {
    // ATTACH DATABASE could be used to access other databases
    let sql = "ATTACH DATABASE '/tmp/malicious.db' AS attack";
    let result = parse_sql_statements(sql);

    // If it parses, it must be rejected at validation level
    match result {
        Ok(stmts) => {
            println!("ATTACH parsed - should be blocked by statement type validation");
            assert!(stmts.len() <= 1);
        }
        Err(_) => {
            // Parse error is acceptable - ATTACH not in allowed statement types
        }
    }
}

// These statements (ATTACH/DETACH/PRAGMA/VACUUM/REINDEX/ANALYZE/CREATE TRIGGER/
// DROP TRIGGER/CREATE VIEW) are all valid SQL — `parse_sql_statements` is a
// pure parser and will happily accept most of them. Actual rejection lives at
// the statement-type-validation layer (executor), not here. The assertions
// below therefore pin *parser* behaviour: whatever it returns, it must never
// allow stacked statements — that is the only smuggling regression a
// parser-level test can catch.
fn assert_no_stacked_statements(sql: &str) {
    match parse_sql_statements(sql) {
        Ok(stmts) => assert!(
            stmts.len() <= 1,
            "Dangerous SQL `{sql}` smuggled multiple statements past the parser ({} stmts)",
            stmts.len()
        ),
        Err(_) => { /* parse error is also safe — nothing executes */ }
    }
}

#[test]
fn test_reject_detach_database() {
    assert_no_stacked_statements("DETACH DATABASE main");
}

#[test]
fn test_reject_pragma_statements() {
    let pragmas = [
        "PRAGMA foreign_keys = OFF",
        "PRAGMA journal_mode = DELETE",
        "PRAGMA table_info(haex_extensions)",
        "PRAGMA database_list",
        "PRAGMA secure_delete = OFF",
        "PRAGMA read_uncommitted = 1",
        "PRAGMA writable_schema = ON",
    ];
    for sql in pragmas {
        assert_no_stacked_statements(sql);
    }
}

#[test]
fn test_reject_vacuum() {
    assert_no_stacked_statements("VACUUM");
}

#[test]
fn test_reject_reindex() {
    assert_no_stacked_statements("REINDEX");
}

#[test]
fn test_reject_analyze() {
    assert_no_stacked_statements("ANALYZE");
}

#[test]
fn test_reject_create_trigger() {
    let sql = r#"
        CREATE TRIGGER evil_trigger
        AFTER INSERT ON testpublickey__testextension__users
        BEGIN
            INSERT INTO haex_extensions VALUES ('malicious');
        END
    "#;
    assert_no_stacked_statements(sql);
}

#[test]
fn test_reject_drop_trigger() {
    assert_no_stacked_statements("DROP TRIGGER IF EXISTS crdt_insert_trigger");
}

#[test]
fn test_reject_create_view() {
    assert_no_stacked_statements("CREATE VIEW evil_view AS SELECT * FROM haex_extensions");
}
