// src-tauri/src/extension/database/planner.rs
// Testable SQL execution planning logic without infrastructure dependencies

use crate::database::core::{parse_sql_statements, ValueConverter};
use crate::database::error::DatabaseError;
use rusqlite::types::Value as SqliteValue;
use serde_json::Value as JsonValue;
use sqlparser::ast::Statement;

/// Testable SQL execution planner that doesn't depend on database infrastructure
///
/// This struct contains logic for validating and preparing SQL executions
/// without requiring Transaction, HlcService, or actual database access.
pub struct SqlExecutionPlanner;

impl SqlExecutionPlanner {
    /// Parses SQL and ensures it's a single statement
    ///
    /// Returns the parsed AST for further processing
    pub fn parse_single_statement(sql: &str) -> Result<Statement, DatabaseError> {
        let mut ast_vec = parse_sql_statements(sql)?;

        if ast_vec.len() != 1 {
            return Err(DatabaseError::ExecutionError {
                sql: sql.to_string(),
                reason: "Expected a single SQL statement".to_string(),
                table: None,
            });
        }

        // Safe to pop because we verified len() == 1
        ast_vec.pop().ok_or_else(|| DatabaseError::ExecutionError {
            sql: sql.to_string(),
            reason: "Failed to extract statement from AST".to_string(),
            table: None,
        })
    }

    /// Converts JSON parameters to SQLite values
    ///
    /// This is the conversion layer between frontend (JSON) and backend (SQLite)
    pub fn convert_params(params: &[JsonValue]) -> Result<Vec<SqliteValue>, DatabaseError> {
        params
            .iter()
            .map(ValueConverter::json_to_rusqlite_value)
            .collect()
    }

    /// Extracts table name from CREATE TABLE statement
    pub fn extract_create_table_name(statement: &Statement) -> Option<String> {
        if let Statement::CreateTable(create_table) = statement {
            let raw_name = create_table.name.to_string();
            // Remove quotes (both " and `)
            Some(raw_name.trim_matches('"').trim_matches('`').to_string())
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_parse_single_statement_success() {
        let sql = "SELECT * FROM users";
        let result = SqlExecutionPlanner::parse_single_statement(sql);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_single_statement_multiple() {
        let sql = "SELECT * FROM users; SELECT * FROM posts";
        let result = SqlExecutionPlanner::parse_single_statement(sql);
        assert!(result.is_err());
    }

    #[test]
    fn test_convert_params() {
        let params = vec![json!("Alice"), json!(30), json!(true), json!(null)];
        let result = SqlExecutionPlanner::convert_params(&params);
        assert!(result.is_ok());

        let converted = result.unwrap();
        assert_eq!(converted.len(), 4);
    }

    #[test]
    fn test_extract_create_table_name() {
        let sql = "CREATE TABLE my_table (id TEXT PRIMARY KEY)";
        let statement = SqlExecutionPlanner::parse_single_statement(sql).unwrap();
        let table_name = SqlExecutionPlanner::extract_create_table_name(&statement);

        assert_eq!(table_name, Some("my_table".to_string()));
    }

    #[test]
    fn test_extract_create_table_name_with_quotes() {
        let sql = r#"CREATE TABLE "quoted_table" (id TEXT PRIMARY KEY)"#;
        let statement = SqlExecutionPlanner::parse_single_statement(sql).unwrap();
        let table_name = SqlExecutionPlanner::extract_create_table_name(&statement);

        assert_eq!(table_name, Some("quoted_table".to_string()));
    }

    #[test]
    fn test_extract_create_table_name_non_create() {
        let sql = "SELECT * FROM users";
        let statement = SqlExecutionPlanner::parse_single_statement(sql).unwrap();
        let table_name = SqlExecutionPlanner::extract_create_table_name(&statement);

        assert_eq!(table_name, None);
    }
}
