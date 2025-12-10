// src-tauri/src/extension/database/helpers.rs
//
// Helper functions for executing extension SQL statements.
// These can be used both from Tauri commands and internal operations like migrations.

use rusqlite::params_from_iter;
use serde_json::Value as JsonValue;
use sqlparser::ast::Statement;

use crate::crdt::trigger;
use crate::database::core::{
    parse_sql_statements, with_connection, ValueConverter, DRIZZLE_STATEMENT_BREAKPOINT,
};
use crate::database::error::DatabaseError;
use crate::extension::database::executor::SqlExecutor;
use crate::extension::error::ExtensionError;
use crate::AppState;

/// Context for executing extension SQL statements.
/// Used to pass extension info without requiring the extension to be in ExtensionManager.
pub struct ExtensionSqlContext {
    pub public_key: String,
    pub name: String,
    pub is_dev_mode: bool,
}

impl ExtensionSqlContext {
    pub fn new(public_key: String, name: String, is_dev_mode: bool) -> Self {
        Self {
            public_key,
            name,
            is_dev_mode,
        }
    }

    /// Get the expected table prefix for this extension
    pub fn get_table_prefix(&self) -> String {
        crate::extension::utils::get_extension_table_prefix(&self.public_key, &self.name)
    }
}

/// Validates that a SQL statement only affects tables with the correct extension prefix.
/// This is a simpler validation used for migrations during installation when the extension
/// is not yet in the ExtensionManager.
pub fn validate_sql_table_prefix(
    ctx: &ExtensionSqlContext,
    sql: &str,
) -> Result<(), ExtensionError> {
    use crate::database::core::parse_single_statement;

    let statement = parse_single_statement(sql).map_err(|e| DatabaseError::ParseError {
        reason: e.to_string(),
        sql: sql.to_string(),
    })?;

    let expected_prefix = ctx.get_table_prefix();

    // Extract table names based on statement type and validate prefix
    let table_names = match &statement {
        Statement::CreateTable(create_table) => {
            vec![create_table.name.to_string()]
        }
        Statement::AlterTable { name, .. } => {
            vec![name.to_string()]
        }
        Statement::Drop { names, .. } => names.iter().map(|n| n.to_string()).collect(),
        Statement::CreateIndex(create_index) => {
            vec![create_index.table_name.to_string()]
        }
        // For other statements (like INSERT, UPDATE, DELETE, SELECT), skip prefix validation
        // as these would be blocked by permission checks at runtime
        _ => return Ok(()),
    };

    for table_name in table_names {
        let clean_name = table_name.trim_matches('"').trim_matches('`');
        if !clean_name.starts_with(&expected_prefix) {
            return Err(ExtensionError::ValidationError {
                reason: format!(
                    "Extension can only operate on tables with prefix '{}'. Got: '{}'",
                    expected_prefix, clean_name
                ),
            });
        }
    }

    Ok(())
}

/// Validates parameter count against SQL placeholders
fn validate_params(sql: &str, params: &[JsonValue]) -> Result<(), DatabaseError> {
    let total_placeholders = count_sql_placeholders(sql);
    let expected = params.len();

    if total_placeholders != expected {
        return Err(DatabaseError::StatementError {
            reason: format!(
                "Parameter mismatch: SQL has {} placeholders but {} parameters were provided",
                total_placeholders, expected
            ),
        });
    }

    Ok(())
}

/// Counts the number of SQL placeholders (?) in a statement
fn count_sql_placeholders(sql: &str) -> usize {
    let mut count = 0;
    let mut in_string = false;
    let mut prev_char = None;

    for c in sql.chars() {
        match c {
            '\'' if prev_char != Some('\\') => in_string = !in_string,
            '?' if !in_string => count += 1,
            _ => {}
        }
        prev_char = Some(c);
    }

    count
}

/// Executes a SQL statement with CRDT support using the provided extension context.
/// This is the core execution logic used by both `extension_sql_execute` and migration execution.
///
/// Unlike `extension_sql_execute`, this function does NOT perform full permission validation.
/// It only validates that table operations use the correct extension prefix.
/// Use this for trusted internal operations like migrations from signed bundles.
pub fn execute_sql_with_context(
    ctx: &ExtensionSqlContext,
    sql: &str,
    params: &[JsonValue],
    state: &AppState,
) -> Result<Vec<Vec<JsonValue>>, ExtensionError> {
    // Validate table prefix
    validate_sql_table_prefix(ctx, sql)?;

    // Parameter validation
    validate_params(sql, params)?;

    // SQL parsing
    let mut ast_vec = parse_sql_statements(sql)?;

    if ast_vec.len() != 1 {
        return Err(ExtensionError::Database {
            source: DatabaseError::ExecutionError {
                sql: sql.to_string(),
                reason: "execute_sql_with_context should only receive a single SQL statement"
                    .to_string(),
                table: None,
            },
        });
    }

    let statement = ast_vec.pop().unwrap();

    // If this is a SELECT statement, just execute it
    if matches!(statement, Statement::Query(_)) {
        return with_connection(&state.db, |conn| {
            let sql_params = ValueConverter::convert_params(params)?;
            let transformed_sql = statement.to_string();

            let mut prepared_stmt =
                conn.prepare(&transformed_sql)
                    .map_err(|e| DatabaseError::ExecutionError {
                        sql: transformed_sql.clone(),
                        reason: e.to_string(),
                        table: None,
                    })?;

            let num_columns = prepared_stmt.column_count();
            let mut rows = prepared_stmt
                .query(params_from_iter(sql_params.iter()))
                .map_err(|e| DatabaseError::QueryError {
                    reason: e.to_string(),
                })?;

            let mut result_vec: Vec<Vec<JsonValue>> = Vec::new();
            while let Some(row) = rows.next().map_err(|e| DatabaseError::QueryError {
                reason: e.to_string(),
            })? {
                let mut row_values: Vec<JsonValue> = Vec::new();
                for i in 0..num_columns {
                    let value_ref = row.get_ref(i).map_err(|e| DatabaseError::QueryError {
                        reason: e.to_string(),
                    })?;
                    let json_value = crate::database::core::convert_value_ref_to_json(value_ref)?;
                    row_values.push(json_value);
                }
                result_vec.push(row_values);
            }

            Ok(result_vec)
        })
        .map_err(ExtensionError::from);
    }

    // Check if statement has RETURNING clause
    let has_returning = crate::database::core::statement_has_returning(&statement);

    // Database operation
    with_connection(&state.db, |conn| {
        let tx = conn.transaction().map_err(DatabaseError::from)?;

        // Get HLC service reference
        let hlc_service = state.hlc.lock().map_err(|_| DatabaseError::MutexPoisoned {
            reason: "Failed to lock HLC service".to_string(),
        })?;

        // Note: CRDT transformation (adding haex_timestamp) is handled by
        // SqlExecutor::execute_internal_typed / query_internal_typed.
        // Do NOT transform here to avoid double transformation!

        // Convert parameters to references
        let sql_values = ValueConverter::convert_params(params)?;
        let param_refs: Vec<&dyn rusqlite::ToSql> = sql_values
            .iter()
            .map(|v| v as &dyn rusqlite::ToSql)
            .collect();

        let statement_sql = statement.to_string();
        eprintln!("DEBUG: [execute_sql_with_context] Statement SQL: {statement_sql}");

        let result = if has_returning {
            eprintln!(
                "DEBUG: [execute_sql_with_context] Using query_internal_typed (has RETURNING)"
            );
            let (_, rows) =
                SqlExecutor::query_internal_typed(&tx, &hlc_service, &statement_sql, &param_refs)?;
            rows
        } else {
            eprintln!(
                "DEBUG: [execute_sql_with_context] Using execute_internal_typed (no RETURNING)"
            );
            SqlExecutor::execute_internal_typed(&tx, &hlc_service, &statement_sql, &param_refs)?;
            vec![]
        };

        // Handle CREATE TABLE trigger setup (only for production extensions)
        if let Statement::CreateTable(ref create_table_details) = statement {
            let raw_name = create_table_details.name.to_string();
            let table_name_str = raw_name.trim_matches('"').trim_matches('`').to_string();

            if ctx.is_dev_mode {
                println!(
                    "[DEV] Table '{}' created by dev extension - NO CRDT triggers (local-only)",
                    table_name_str
                );
            } else {
                println!(
                    "Table '{}' created by extension, setting up CRDT triggers...",
                    table_name_str
                );
                trigger::setup_triggers_for_table(&tx, &table_name_str, false)?;
                println!(
                    "Triggers for table '{}' successfully created.",
                    table_name_str
                );
            }
        }

        // Commit transaction
        tx.commit().map_err(DatabaseError::from)?;

        Ok(result)
    })
    .map_err(ExtensionError::from)
}

/// Executes all statements from a migration SQL string.
///
/// This helper function:
/// 1. Splits the SQL by statement breakpoint (`--> statement-breakpoint`)
/// 2. Validates each statement has the correct table prefix
/// 3. Executes each statement with CRDT support
/// 4. Sets up triggers for CREATE TABLE (if not dev mode)
///
/// Returns the number of statements executed.
pub fn execute_migration_statements(
    ctx: &ExtensionSqlContext,
    sql_content: &str,
    state: &AppState,
) -> Result<usize, ExtensionError> {
    // Split SQL by statement breakpoint
    let statements: Vec<&str> = sql_content
        .split(DRIZZLE_STATEMENT_BREAKPOINT)
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect();

    for statement in &statements {
        // Validate table prefix
        validate_sql_table_prefix(ctx, statement)?;

        // Execute statement with CRDT support and trigger creation
        execute_sql_with_context(ctx, statement, &[], state)?;
    }

    Ok(statements.len())
}
