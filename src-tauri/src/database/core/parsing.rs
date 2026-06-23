// src-tauri/src/database/core/parsing.rs

use crate::database::error::DatabaseError;
use sqlparser::ast::Statement;
use sqlparser::dialect::SQLiteDialect;
use sqlparser::parser::Parser;

/// Utility für SQL-Parsing - parst ein einzelnes SQL-Statement
pub fn parse_single_statement(sql: &str) -> Result<Statement, DatabaseError> {
    let dialect = SQLiteDialect {};
    let statements = Parser::parse_sql(&dialect, sql).map_err(|e| DatabaseError::ParseError {
        reason: e.to_string(),
        sql: sql.to_string(),
    })?;

    statements
        .into_iter()
        .next()
        .ok_or(DatabaseError::ParseError {
            reason: "No SQL statement found".to_string(),
            sql: sql.to_string(),
        })
}

/// Utility für SQL-Parsing - parst mehrere SQL-Statements
pub fn parse_sql_statements(sql: &str) -> Result<Vec<Statement>, DatabaseError> {
    let dialect = SQLiteDialect {};

    // Normalize whitespace: replace multiple whitespaces (including newlines, tabs) with single space
    let normalized_sql = sql.split_whitespace().collect::<Vec<&str>>().join(" ");

    Parser::parse_sql(&dialect, &normalized_sql).map_err(|e| DatabaseError::ParseError {
        reason: format!("Failed to parse SQL: {e}"),
        sql: sql.to_string(),
    })
}

/// Prüft ob ein Statement ein RETURNING Clause hat (AST-basiert, sicher)
pub fn statement_has_returning(statement: &Statement) -> bool {
    match statement {
        Statement::Insert(insert) => insert.returning.is_some(),
        Statement::Update(update) => update.returning.is_some(),
        Statement::Delete(delete) => delete.returning.is_some(),
        _ => false,
    }
}
