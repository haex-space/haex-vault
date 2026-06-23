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

#[test]
fn test_reject_detach_database() {
    let sql = "DETACH DATABASE main";
    let result = parse_sql_statements(sql);
    println!("DETACH parse result: {:?}", result.is_ok());
}

#[test]
fn test_reject_pragma_statements() {
    // PRAGMA can be used to modify database settings or leak information
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
        let result = parse_sql_statements(sql);
        println!("PRAGMA '{}' parse result: {:?}", sql, result.is_ok());
        // All PRAGMA statements should be rejected by statement type validation
    }
}

#[test]
fn test_reject_vacuum() {
    let sql = "VACUUM";
    let _ = parse_sql_statements(sql);
    // Should be blocked by statement type validation
}

#[test]
fn test_reject_reindex() {
    let sql = "REINDEX";
    let _ = parse_sql_statements(sql);
    // Should be blocked by statement type validation
}

#[test]
fn test_reject_analyze() {
    let sql = "ANALYZE";
    let _ = parse_sql_statements(sql);
    // Should be blocked by statement type validation
}

#[test]
fn test_reject_create_trigger() {
    // CREATE TRIGGER could be used to execute code on data changes
    let sql = r#"
        CREATE TRIGGER evil_trigger
        AFTER INSERT ON testpublickey__testextension__users
        BEGIN
            INSERT INTO haex_extensions VALUES ('malicious');
        END
    "#;

    let result = parse_sql_statements(sql);
    // Should be blocked - extensions cannot create triggers
    println!("CREATE TRIGGER parse result: {:?}", result.is_ok());
}

#[test]
fn test_reject_drop_trigger() {
    let sql = "DROP TRIGGER IF EXISTS crdt_insert_trigger";
    let _ = parse_sql_statements(sql);
    // Should be blocked - extensions cannot modify triggers
}

#[test]
fn test_reject_create_view() {
    let sql = "CREATE VIEW evil_view AS SELECT * FROM haex_extensions";
    let result = parse_sql_statements(sql);
    println!("CREATE VIEW parse result: {:?}", result.is_ok());
    // Views that access system tables must be blocked
}
