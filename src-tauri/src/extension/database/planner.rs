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
    /// Validates that SQL has correct number of parameters
    ///
    /// # Examples
    /// ```
    /// let sql = "INSERT INTO users (name, age) VALUES (?, ?)";
    /// let params = vec![json!("Alice"), json!(30)];
    /// assert!(SqlExecutionPlanner::validate_parameters(sql, &params).is_ok());
    /// ```
    pub fn validate_parameters(sql: &str, params: &[JsonValue]) -> Result<(), DatabaseError> {
        let placeholder_count = count_sql_placeholders(sql);

        if placeholder_count != params.len() {
            return Err(DatabaseError::ParameterMismatchError {
                expected: placeholder_count,
                provided: params.len(),
                sql: sql.to_string(),
            });
        }

        Ok(())
    }

    /// Validates batch execution parameters
    ///
    /// Ensures that:
    /// 1. Statement count matches parameter set count
    /// 2. Each statement has correct parameter count
    pub fn validate_batch(
        sqls: &[String],
        params: &[Vec<JsonValue>],
    ) -> Result<(), DatabaseError> {
        if sqls.len() != params.len() {
            return Err(DatabaseError::ExecutionError {
                sql: format!("{} statements but {} param sets", sqls.len(), params.len()),
                reason: "Statement count and parameter count mismatch".to_string(),
                table: None,
            });
        }

        // Validate each statement's parameters
        for (sql, param_set) in sqls.iter().zip(params.iter()) {
            Self::validate_parameters(sql, param_set)?;
        }

        Ok(())
    }

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

    /// Validates that a statement is a SELECT query
    pub fn is_select_statement(statement: &Statement) -> bool {
        matches!(statement, Statement::Query(_))
    }

    /// Checks if a statement has a RETURNING clause
    pub fn has_returning_clause(statement: &Statement) -> bool {
        match statement {
            Statement::Insert(insert) => insert.returning.is_some(),
            Statement::Update { returning, .. } => returning.is_some(),
            Statement::Delete(delete) => delete.returning.is_some(),
            _ => false,
        }
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

/// Counts SQL placeholders (?)
///
/// This is a simple implementation that counts '?' characters.
/// More sophisticated implementations might handle escaped '?' in strings.
pub(crate) fn count_sql_placeholders(sql: &str) -> usize {
    sql.matches('?').count()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_validate_parameters_correct_count() {
        let sql = "INSERT INTO users (name, age) VALUES (?, ?)";
        let params = vec![json!("Alice"), json!(30)];

        assert!(SqlExecutionPlanner::validate_parameters(sql, &params).is_ok());
    }

    #[test]
    fn test_validate_parameters_mismatch() {
        let sql = "INSERT INTO users (name, age) VALUES (?, ?)";
        let params = vec![json!("Alice")]; // Missing one parameter

        let result = SqlExecutionPlanner::validate_parameters(sql, &params);
        assert!(result.is_err());

        if let Err(DatabaseError::ParameterMismatchError {
            expected,
            provided,
            ..
        }) = result
        {
            assert_eq!(expected, 2);
            assert_eq!(provided, 1);
        } else {
            panic!("Expected ParameterMismatchError");
        }
    }

    #[test]
    fn test_validate_parameters_no_placeholders() {
        let sql = "CREATE TABLE users (id TEXT PRIMARY KEY)";
        let params = vec![];

        assert!(SqlExecutionPlanner::validate_parameters(sql, &params).is_ok());
    }

    #[test]
    fn test_validate_batch_success() {
        let sqls = vec![
            "INSERT INTO users (name) VALUES (?)".to_string(),
            "INSERT INTO users (name) VALUES (?)".to_string(),
        ];
        let params = vec![vec![json!("Alice")], vec![json!("Bob")]];

        assert!(SqlExecutionPlanner::validate_batch(&sqls, &params).is_ok());
    }

    #[test]
    fn test_validate_batch_count_mismatch() {
        let sqls = vec!["INSERT INTO users (name) VALUES (?)".to_string()];
        let params = vec![vec![json!("Alice")], vec![json!("Bob")]]; // Too many param sets

        let result = SqlExecutionPlanner::validate_batch(&sqls, &params);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_batch_parameter_mismatch() {
        let sqls = vec![
            "INSERT INTO users (name, age) VALUES (?, ?)".to_string(),
            "INSERT INTO users (name) VALUES (?)".to_string(),
        ];
        let params = vec![
            vec![json!("Alice"), json!(30)],
            vec![json!("Bob"), json!(25)], // Too many params for second statement
        ];

        let result = SqlExecutionPlanner::validate_batch(&sqls, &params);
        assert!(result.is_err());
    }

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
    fn test_is_select_statement() {
        let select_sql = "SELECT * FROM users";
        let statement = SqlExecutionPlanner::parse_single_statement(select_sql).unwrap();
        assert!(SqlExecutionPlanner::is_select_statement(&statement));

        let insert_sql = "INSERT INTO users (name) VALUES ('Alice')";
        let statement = SqlExecutionPlanner::parse_single_statement(insert_sql).unwrap();
        assert!(!SqlExecutionPlanner::is_select_statement(&statement));
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

    #[test]
    fn test_count_sql_placeholders() {
        assert_eq!(
            count_sql_placeholders("INSERT INTO users (name, age) VALUES (?, ?)"),
            2
        );
        assert_eq!(count_sql_placeholders("SELECT * FROM users WHERE id = ?"), 1);
        assert_eq!(count_sql_placeholders("CREATE TABLE users (id TEXT)"), 0);
        assert_eq!(
            count_sql_placeholders("UPDATE users SET name = ?, age = ? WHERE id = ?"),
            3
        );
    }
}
