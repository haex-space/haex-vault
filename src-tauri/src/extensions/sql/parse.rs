//! Parsing one statement of extension SQL.
//!
//! Ported from haex-space/haex-vault-legacy `8dce379d94e18fcd42c3b73686a06f984ca3f574`,
//! `src-tauri/src/database/core/parsing.rs` (`parse_sql_statements`) and
//! `src-tauri/src/extension/database/planner.rs` (`parse_single_statement`). Changed: the SQL is
//! parsed as written (haex-vault 2.x (Legacy) collapsed all whitespace first, which also changed string
//! literals), and exactly one statement is accepted.

use haex_crdt::sqlparser::ast::Statement;
use haex_crdt::sqlparser::dialect::SQLiteDialect;
use haex_crdt::sqlparser::parser::Parser;
use haex_crdt::sqlparser::tokenizer::{Token, Tokenizer};

use crate::extensions::error::{BridgeError, ExtensionErrorCode};

pub(crate) fn violation(message: impl Into<String>) -> BridgeError {
    BridgeError::new(ExtensionErrorCode::SecurityViolation, message)
}

/// Exactly one statement, else 1000.
pub fn parse_one(sql: &str) -> Result<Statement, BridgeError> {
    let mut statements = Parser::parse_sql(&SQLiteDialect {}, sql)
        .map_err(|e| violation(format!("SQL not understood: {e}")))?;
    if statements.len() != 1 {
        return Err(violation("exactly one statement is allowed"));
    }
    Ok(statements.remove(0))
}

/// Every identifier and keyword of the SQL with its quoting removed; string literals are left out.
pub fn words(sql: &str) -> Result<Vec<String>, BridgeError> {
    let tokens = Tokenizer::new(&SQLiteDialect {}, sql)
        .tokenize()
        .map_err(|e| violation(format!("SQL not understood: {e}")))?;
    Ok(tokens
        .into_iter()
        .filter_map(|token| match token {
            Token::Word(word) => Some(word.value),
            _ => None,
        })
        .collect())
}

/// Whether the statement returns rows: a query, or a write with `RETURNING`.
pub fn returns_rows(statement: &Statement) -> bool {
    match statement {
        Statement::Query(_) => true,
        Statement::Insert(insert) => insert.returning.is_some(),
        Statement::Update(update) => update.returning.is_some(),
        Statement::Delete(delete) => delete.returning.is_some(),
        _ => false,
    }
}
