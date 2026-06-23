// src-tauri/src/database/core/mod.rs

/// Statement breakpoint marker used by Drizzle migrations
pub const DRIZZLE_STATEMENT_BREAKPOINT: &str = "--> statement-breakpoint";

mod connection;
mod execute;
mod extract;
mod init;
mod parsing;
mod prefix;
mod select;
mod value;

pub use connection::with_connection;
pub use execute::{execute, execute_with_crdt, MAX_CRDT_TRANSACTION_BYTES};
pub use extract::{
    extract_primary_table_name_from_sql, extract_table_names_from_sql,
    extract_table_names_from_statement,
};
pub use init::{install_tx_hlc_hooks, open_and_init_db, register_current_hlc_udf};
pub use parsing::{parse_single_statement, parse_sql_statements, statement_has_returning};
pub use prefix::strip_main_schema_prefix;
pub use select::{select, select_with_crdt};
pub use value::{convert_value_ref_to_json, ValueConverter};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crdt::hlc::HlcService;
    use crate::crdt::trigger::UUID_FUNCTION_NAME;
    use crate::database::connection_context::ConnectionContext;
    use crate::database::error::DatabaseError;
    use rusqlite::functions::FunctionFlags;
    use rusqlite::Connection;
    use serde_json::Value as JsonValue;
    use sqlparser::ast::Statement;
    use uuid::Uuid;

    #[test]
    fn test_extract_simple_select() {
        let sql = "SELECT * FROM users";
        let tables = extract_table_names_from_sql(sql).unwrap();
        assert_eq!(tables, vec!["users"]);
    }

    #[test]
    fn test_extract_select_with_join() {
        let sql = "SELECT u.name, p.title FROM users u JOIN posts p ON u.id = p.user_id";
        let tables = extract_table_names_from_sql(sql).unwrap();
        assert_eq!(tables, vec!["users", "posts"]);
    }

    #[test]
    fn test_extract_insert() {
        let sql = "INSERT INTO users (name, email) VALUES (?, ?)";
        let tables = extract_table_names_from_sql(sql).unwrap();
        assert_eq!(tables, vec!["users"]);
    }

    #[test]
    fn test_extract_update() {
        let sql = "UPDATE users SET name = ? WHERE id = ?";
        let tables = extract_table_names_from_sql(sql).unwrap();
        assert_eq!(tables, vec!["users"]);
    }

    #[test]
    fn test_extract_delete() {
        let sql = "DELETE FROM users WHERE id = ?";
        let tables = extract_table_names_from_sql(sql).unwrap();
        assert_eq!(tables, vec!["users"]);
    }

    #[test]
    fn test_extract_create_table() {
        let sql = "CREATE TABLE new_table (id INTEGER, name TEXT)";
        let tables = extract_table_names_from_sql(sql).unwrap();
        assert_eq!(tables, vec!["new_table"]);
    }

    #[test]
    fn test_extract_subquery() {
        let sql = "SELECT * FROM (SELECT id FROM users) AS sub";
        let tables = extract_table_names_from_sql(sql).unwrap();
        assert_eq!(tables, vec!["users"]);
    }

    #[test]
    fn test_extract_primary_table() {
        let sql = "SELECT u.name FROM users u JOIN posts p ON u.id = p.user_id";
        let primary_table = extract_primary_table_name_from_sql(sql).unwrap();
        assert_eq!(primary_table, Some("users".to_string()));
    }

    #[test]
    fn test_extract_complex_query() {
        let sql = r#"
            SELECT u.name, COUNT(p.id) as post_count
            FROM users u
            LEFT JOIN posts p ON u.id = p.user_id
            WHERE u.created_at > (SELECT MIN(created_at) FROM sessions)
            GROUP BY u.id
        "#;
        let tables = extract_table_names_from_sql(sql).unwrap();
        assert_eq!(tables, vec!["users", "posts", "sessions"]);
    }

    #[test]
    fn test_invalid_sql() {
        let sql = "INVALID SQL";
        let result = extract_table_names_from_sql(sql);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_single_statement() {
        let sql = "SELECT * FROM users WHERE id = ?";
        let result = parse_single_statement(sql);
        assert!(result.is_ok());
        assert!(matches!(result.unwrap(), Statement::Query(_)));
    }

    #[test]
    fn test_parse_invalid_sql() {
        let sql = "INVALID SQL STATEMENT";
        let result = parse_single_statement(sql);
        assert!(matches!(result, Err(DatabaseError::ParseError { .. })));
    }

    #[test]
    fn test_convert_value_ref_to_json() {
        use rusqlite::types::ValueRef;

        assert_eq!(
            convert_value_ref_to_json(ValueRef::Null).unwrap(),
            JsonValue::Null
        );
        assert_eq!(
            convert_value_ref_to_json(ValueRef::Integer(42)).unwrap(),
            JsonValue::Number(42.into())
        );
        assert_eq!(
            convert_value_ref_to_json(ValueRef::Text(b"hello")).unwrap(),
            JsonValue::String("hello".to_string())
        );
    }

    // Test für die neuen AST-basierten Funktionen
    #[test]
    fn test_extract_table_names_comprehensive() {
        // Test verschiedene SQL-Statement-Typen
        assert_eq!(
            extract_primary_table_name_from_sql("SELECT * FROM users WHERE id = 1").unwrap(),
            Some("users".to_string())
        );
        assert_eq!(
            extract_primary_table_name_from_sql("INSERT INTO products (name) VALUES ('test')")
                .unwrap(),
            Some("products".to_string())
        );
        assert_eq!(
            extract_primary_table_name_from_sql("UPDATE orders SET status = 'completed'").unwrap(),
            Some("orders".to_string())
        );
        assert_eq!(
            extract_primary_table_name_from_sql("DELETE FROM customers").unwrap(),
            Some("customers".to_string())
        );
    }

    #[test]
    fn test_statement_has_returning_insert() {
        // INSERT ohne RETURNING
        let stmt = parse_single_statement("INSERT INTO users (name) VALUES ('test')").unwrap();
        assert!(!statement_has_returning(&stmt));

        // INSERT mit RETURNING
        let stmt_ret =
            parse_single_statement("INSERT INTO users (name) VALUES ('test') RETURNING id, name")
                .unwrap();
        assert!(statement_has_returning(&stmt_ret));

        // INSERT mit RETURNING *
        let stmt_ret_all =
            parse_single_statement("INSERT INTO users (name) VALUES ('test') RETURNING *").unwrap();
        assert!(statement_has_returning(&stmt_ret_all));
    }

    #[test]
    fn test_statement_has_returning_update() {
        // UPDATE ohne RETURNING
        let stmt = parse_single_statement("UPDATE users SET name = 'new' WHERE id = 1").unwrap();
        assert!(!statement_has_returning(&stmt));

        // UPDATE mit RETURNING
        let stmt_ret =
            parse_single_statement("UPDATE users SET name = 'new' WHERE id = 1 RETURNING id, name")
                .unwrap();
        assert!(statement_has_returning(&stmt_ret));
    }

    #[test]
    fn test_statement_has_returning_delete() {
        // DELETE ohne RETURNING
        let stmt = parse_single_statement("DELETE FROM users WHERE id = 1").unwrap();
        assert!(!statement_has_returning(&stmt));

        // DELETE mit RETURNING
        let stmt_ret =
            parse_single_statement("DELETE FROM users WHERE id = 1 RETURNING id, name").unwrap();
        assert!(statement_has_returning(&stmt_ret));
    }

    #[test]
    fn test_statement_has_returning_select() {
        // SELECT hat kein RETURNING (immer false)
        let stmt = parse_single_statement("SELECT * FROM users").unwrap();
        assert!(!statement_has_returning(&stmt));
    }

    #[test]
    fn test_gen_uuid_produces_distinct_values() {
        let conn = Connection::open_in_memory().unwrap();
        conn.create_scalar_function(
            UUID_FUNCTION_NAME,
            0,
            FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_INNOCUOUS,
            |_ctx| Ok(Uuid::new_v4().to_string()),
        )
        .unwrap();

        conn.execute_batch("CREATE TABLE test_uuids (id TEXT NOT NULL, other_id TEXT NOT NULL);")
            .unwrap();

        conn.execute(
            &format!(
                "INSERT INTO test_uuids (id, other_id) VALUES ({fn_name}(), {fn_name}());",
                fn_name = UUID_FUNCTION_NAME
            ),
            [],
        )
        .unwrap();

        let (id, other_id): (String, String) = conn
            .query_row("SELECT id, other_id FROM test_uuids", [], |row| {
                Ok((row.get(0).unwrap(), row.get(1).unwrap()))
            })
            .unwrap();

        assert_ne!(
            id, other_id,
            "Two gen_uuid() calls in the same INSERT must produce different values"
        );
    }

    #[test]
    fn test_strip_main_schema_preserves_string_literals() {
        let sql = "SELECT * FROM main.users WHERE notes LIKE '%main.table%'";
        let result = strip_main_schema_prefix(sql);
        assert!(
            !result.contains("main.users"),
            "Should strip main. from table ref"
        );
        assert!(
            result.contains("%main.table%"),
            "Should NOT strip main. inside string literal"
        );
    }

    // ---- current_hlc() UDF + transaction-scope HLC ---------------------

    fn setup_hlc_test_connection(device_id: &str) -> Connection {
        let conn = Connection::open_in_memory().expect("in-memory connection");
        let hlc = HlcService::new_for_testing(device_id);
        let ctx = ConnectionContext::new();
        register_current_hlc_udf(&conn, hlc, ctx.clone()).expect("register current_hlc");
        install_tx_hlc_hooks(&conn, ctx).expect("install tx-hlc hooks");
        conn
    }

    #[test]
    fn test_current_hlc_differs_across_autocommit_statements() {
        let conn = setup_hlc_test_connection("hlc-across-stmts");
        let first: String = conn
            .query_row("SELECT current_hlc()", [], |row| row.get(0))
            .unwrap();
        // Any non-query statement forces the auto-commit transaction to close.
        conn.execute_batch("CREATE TABLE _tick (id INTEGER);")
            .unwrap();
        let second: String = conn
            .query_row("SELECT current_hlc()", [], |row| row.get(0))
            .unwrap();
        assert_ne!(
            first, second,
            "current_hlc() must differ across separate auto-commit transactions"
        );
    }

    #[test]
    fn test_current_hlc_same_across_writes_in_one_tx() {
        // The transaction-scope invariant only applies to *writes*: multiple
        // INSERT/UPDATE/DELETE statements inside one tx must share one HLC.
        // Bare read-only `SELECT current_hlc()` calls intentionally draw fresh
        // timestamps so a stray probe cannot poison the HLC of a later write.
        let mut conn = setup_hlc_test_connection("hlc-explicit-tx-writes");
        conn.execute_batch("CREATE TABLE t (id INTEGER PRIMARY KEY, hlc TEXT);")
            .unwrap();
        let tx = conn.transaction().expect("begin tx");
        tx.execute("INSERT INTO t (id, hlc) VALUES (1, current_hlc())", [])
            .unwrap();
        tx.execute("INSERT INTO t (id, hlc) VALUES (2, current_hlc())", [])
            .unwrap();
        tx.commit().unwrap();
        let (a, b): (String, String) = conn
            .query_row(
                "SELECT (SELECT hlc FROM t WHERE id=1), (SELECT hlc FROM t WHERE id=2)",
                [],
                |row| Ok((row.get(0).unwrap(), row.get(1).unwrap())),
            )
            .unwrap();
        assert_eq!(
            a, b,
            "two writes within one explicit transaction must share one HLC"
        );
    }

    #[test]
    fn test_readonly_probe_does_not_poison_next_write_tx() {
        // Regression test for the CodeRabbit-identified poisoning scenario:
        // a bare `SELECT current_hlc()` outside any write must not dictate
        // the HLC that a later write transaction receives.
        let conn = setup_hlc_test_connection("hlc-no-poison");
        conn.execute_batch("CREATE TABLE t (id INTEGER PRIMARY KEY, hlc TEXT);")
            .unwrap();
        let probed: String = conn
            .query_row("SELECT current_hlc()", [], |row| row.get(0))
            .unwrap();
        conn.execute("INSERT INTO t (id, hlc) VALUES (1, current_hlc())", [])
            .unwrap();
        let persisted: String = conn
            .query_row("SELECT hlc FROM t WHERE id=1", [], |row| row.get(0))
            .unwrap();
        assert_ne!(
            probed, persisted,
            "the probed value must not be reused by the subsequent write"
        );
    }

    #[test]
    fn test_current_hlc_reset_on_rollback() {
        let mut conn = setup_hlc_test_connection("hlc-rollback");
        let tx = conn.transaction().expect("begin tx");
        let a: String = tx
            .query_row("SELECT current_hlc()", [], |row| row.get(0))
            .unwrap();
        tx.rollback().unwrap();
        let b: String = conn
            .query_row("SELECT current_hlc()", [], |row| row.get(0))
            .unwrap();
        assert_ne!(a, b, "current_hlc() must be fresh after a rollback");
    }
}
