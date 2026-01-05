// src-tauri/src/extension/database/helpers.rs
//
// Helper functions for executing extension SQL statements.
// These can be used both from Tauri commands and internal operations like migrations.

use rusqlite::params_from_iter;
use serde_json::Value as JsonValue;
use sqlparser::ast::Statement;

use crate::crdt::transformer::CrdtTransformer;
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
///
/// Also allows temporary tables with `__new_` prefix used by Drizzle for table reconstruction
/// when changing primary keys or foreign key constraints.
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

        // Check if the table name starts with the expected extension prefix
        // Also allow Drizzle's temporary tables that use __new_ prefix for table reconstruction
        // (e.g., when changing PKs or FKs, Drizzle creates __new_<tablename>, copies data, drops old, renames)
        let is_valid = clean_name.starts_with(&expected_prefix)
            || (clean_name.starts_with("__new_")
                && clean_name
                    .strip_prefix("__new_")
                    .is_some_and(|rest| rest.starts_with(&expected_prefix)));

        if !is_valid {
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

    let mut statement = ast_vec.pop().unwrap();

    // If this is a SELECT statement, apply tombstone filter and execute
    if let Statement::Query(ref mut query) = statement {
        // Apply CRDT tombstone filter to SELECT queries (unless in dev mode)
        // This ensures tombstoned (soft-deleted) rows are filtered out
        if !ctx.is_dev_mode {
            let transformer = CrdtTransformer::new();
            transformer.transform_query(query);
        }

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

        // Convert parameters to references
        let sql_values = ValueConverter::convert_params(params)?;
        let param_refs: Vec<&dyn rusqlite::ToSql> = sql_values
            .iter()
            .map(|v| v as &dyn rusqlite::ToSql)
            .collect();

        let statement_sql = statement.to_string();

        // Dev mode extensions execute SQL directly without CRDT transformation.
        // Their tables don't have CRDT columns (haex_timestamp, haex_tombstone, haex_column_hlcs).
        let result = if ctx.is_dev_mode {
            if has_returning {
                // Execute with RETURNING clause
                let mut stmt = tx.prepare(&statement_sql).map_err(|e| DatabaseError::ExecutionError {
                    sql: statement_sql.clone(),
                    table: None,
                    reason: e.to_string(),
                })?;
                let num_columns = stmt.column_count();
                let mut rows = stmt.query(params_from_iter(param_refs.iter())).map_err(|e| {
                    DatabaseError::ExecutionError {
                        sql: statement_sql.clone(),
                        table: None,
                        reason: e.to_string(),
                    }
                })?;
                let mut result_vec: Vec<Vec<JsonValue>> = Vec::new();
                while let Some(row) = rows.next().map_err(|e| DatabaseError::ExecutionError {
                    sql: statement_sql.clone(),
                    table: None,
                    reason: e.to_string(),
                })? {
                    let mut row_values: Vec<JsonValue> = Vec::new();
                    for i in 0..num_columns {
                        let value_ref = row.get_ref(i).map_err(|e| DatabaseError::ExecutionError {
                            sql: statement_sql.clone(),
                            table: None,
                            reason: e.to_string(),
                        })?;
                        let json_value = crate::database::core::convert_value_ref_to_json(value_ref)?;
                        row_values.push(json_value);
                    }
                    result_vec.push(row_values);
                }
                result_vec
            } else {
                // Execute without RETURNING clause
                tx.execute(&statement_sql, params_from_iter(param_refs.iter()))
                    .map_err(|e| DatabaseError::ExecutionError {
                        sql: statement_sql.clone(),
                        table: None,
                        reason: format!("Execute failed: {e}"),
                    })?;
                vec![]
            }
        } else {
            // Production mode: Use CRDT-aware execution
            // Get HLC service reference
            let hlc_service = state.hlc.lock().map_err(|_| DatabaseError::MutexPoisoned {
                reason: "Failed to lock HLC service".to_string(),
            })?;

            // Note: CRDT transformation (adding haex_timestamp) is handled by
            // SqlExecutor::execute_internal_typed / query_internal_typed.
            // Do NOT transform here to avoid double transformation!

            if has_returning {
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
            }
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
                // For CREATE TABLE IF NOT EXISTS: The table might already exist without CRDT columns
                // (e.g., from a previous dev mode installation). Ensure CRDT columns exist.
                let columns_added = trigger::ensure_crdt_columns(&tx, &table_name_str)?;
                if columns_added {
                    println!(
                        "[CRDT] Added missing CRDT columns to existing table '{}'",
                        table_name_str
                    );
                }

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

/// Checks if a SQL statement is any PRAGMA statement.
pub fn is_pragma_statement(sql: &str) -> bool {
    sql.trim().to_uppercase().starts_with("PRAGMA")
}

/// Checks if a PRAGMA statement is allowed for extension migrations.
/// Only PRAGMA foreign_keys=OFF/ON is permitted - used by Drizzle for table reconstruction.
/// All other PRAGMAs are rejected for security reasons.
pub fn is_allowed_pragma(sql: &str) -> bool {
    let normalized = sql
        .trim()
        .to_uppercase()
        .replace(" ", "")
        .replace(";", "");

    // Only allow foreign_keys pragma for table reconstruction
    normalized == "PRAGMAFOREIGN_KEYS=OFF" || normalized == "PRAGMAFOREIGN_KEYS=ON"
}

/// Executes an allowed PRAGMA statement directly.
/// PRAGMA statements don't go through the SQL parser because sqlparser doesn't support them.
///
/// SECURITY: Only PRAGMA foreign_keys=OFF/ON is allowed. This is needed for Drizzle migrations
/// that reconstruct tables with foreign keys. All other PRAGMAs are rejected to prevent:
/// - PRAGMA key/rekey (encryption key manipulation - though SQLCipher handles this separately)
/// - PRAGMA writable_schema (direct schema manipulation)
/// - PRAGMA database_list (information disclosure)
/// - PRAGMA table_info (information disclosure about other tables)
/// - Other potentially dangerous PRAGMAs
fn execute_pragma_statement(sql: &str, state: &AppState) -> Result<(), ExtensionError> {
    // Security check: only allow specific PRAGMAs
    if !is_allowed_pragma(sql) {
        return Err(ExtensionError::ValidationError {
            reason: format!(
                "PRAGMA statement not allowed: '{}'. Only 'PRAGMA foreign_keys=OFF/ON' is permitted for migrations.",
                sql.chars().take(50).collect::<String>()
            ),
        });
    }

    with_connection(&state.db, |conn| {
        conn.execute(sql, [])
            .map_err(|e| DatabaseError::ExecutionError {
                sql: sql.to_string(),
                table: None,
                reason: format!("PRAGMA execution failed: {e}"),
            })?;
        Ok(())
    })
    .map_err(ExtensionError::from)
}

/// Executes all statements from a migration SQL string.
///
/// This helper function:
/// 1. Splits the SQL by statement breakpoint (`--> statement-breakpoint`)
/// 2. Validates each statement has the correct table prefix (skipped for PRAGMA)
/// 3. Executes each statement with CRDT support (PRAGMA executed directly)
/// 4. Sets up triggers for CREATE TABLE (if not dev mode)
/// 5. After all statements: ensures ALL extension tables have CRDT columns and triggers
///
/// Note: This function is idempotent for schema changes:
/// - "duplicate column" errors from ALTER TABLE ADD COLUMN are ignored
/// - "table already exists" errors from CREATE TABLE are ignored
///
/// PRAGMA statements (like `PRAGMA foreign_keys=OFF/ON`) are executed directly
/// without going through the SQL parser, as they are used by Drizzle for table
/// reconstruction but are not supported by sqlparser-rs.
///
/// The final step (5) is crucial for upgrading tables that were created in dev mode
/// (without CRDT columns) to production mode (with CRDT columns and triggers).
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
        // Handle PRAGMA statements separately (not supported by sqlparser)
        // PRAGMA is used by Drizzle for table reconstruction with foreign keys
        if is_pragma_statement(statement) {
            println!(
                "[MIGRATION] Executing PRAGMA: {}",
                statement.chars().take(50).collect::<String>()
            );
            execute_pragma_statement(statement, state)?;
            continue;
        }

        // Validate table prefix
        validate_sql_table_prefix(ctx, statement)?;

        // Execute statement with CRDT support and trigger creation
        // Ignore idempotent errors (duplicate column, table already exists)
        match execute_sql_with_context(ctx, statement, &[], state) {
            Ok(_) => {}
            Err(ExtensionError::Database { source }) => {
                let error_msg = format!("{:?}", source);
                // Check for idempotent schema errors that can be safely ignored
                if error_msg.contains("duplicate column name")
                    || error_msg.contains("table") && error_msg.contains("already exists")
                {
                    println!(
                        "[MIGRATION] Skipping already-applied schema change: {}",
                        statement.chars().take(80).collect::<String>()
                    );
                    continue;
                }
                // Re-throw other database errors
                return Err(ExtensionError::Database { source });
            }
            Err(e) => return Err(e),
        }
    }

    // After all migrations: ensure ALL extension tables have CRDT columns and triggers.
    // This is important for tables that were created in dev mode (without CRDT columns).
    // When the extension is installed in production mode, these tables need to be upgraded.
    if !ctx.is_dev_mode {
        ensure_extension_tables_have_crdt(ctx, state)?;
    }

    Ok(statements.len())
}

/// Ensures all tables of an extension have CRDT columns and triggers.
///
/// This function:
/// 1. Discovers all tables belonging to the extension
/// 2. For each table, adds missing CRDT columns and triggers
///
/// This is called after migrations to handle the case where tables were
/// created in dev mode (without CRDT columns) and are now being used in
/// production mode.
fn ensure_extension_tables_have_crdt(
    ctx: &ExtensionSqlContext,
    state: &AppState,
) -> Result<(), ExtensionError> {
    use crate::database::core::with_connection;
    use crate::extension::utils::discover_extension_tables;

    with_connection(&state.db, |conn| {
        // Find all tables belonging to this extension
        let tables = discover_extension_tables(conn, &ctx.public_key, &ctx.name)?;

        if tables.is_empty() {
            return Ok(());
        }

        let tx = conn.transaction()?;

        let mut total_columns_added = 0;
        let mut total_triggers_created = 0;

        for table_name in &tables {
            match trigger::ensure_crdt_columns_and_triggers(&tx, table_name) {
                Ok((columns_added, triggers_created)) => {
                    if columns_added {
                        total_columns_added += 1;
                    }
                    if triggers_created {
                        total_triggers_created += 1;
                    }
                }
                Err(e) => {
                    eprintln!(
                        "[CRDT] Warning: Failed to ensure CRDT for table '{}': {}",
                        table_name, e
                    );
                    // Continue with other tables - don't fail the whole migration
                }
            }
        }

        tx.commit()?;

        if total_columns_added > 0 || total_triggers_created > 0 {
            println!(
                "[CRDT] Extension '{}::{}': added CRDT columns to {} tables, created triggers for {} tables",
                ctx.public_key, ctx.name, total_columns_added, total_triggers_created
            );
        }

        Ok(())
    })
    .map_err(ExtensionError::from)
}

/// Splits a migration SQL content into individual statements
pub fn split_migration_statements(sql: &str) -> Vec<&str> {
    sql.split(DRIZZLE_STATEMENT_BREAKPOINT)
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect()
}

/// Execute migrations for dev mode extensions without database tracking
///
/// Dev extensions are not persisted to haex_extensions table, so we cannot
/// store migrations in haex_extension_migrations (foreign key constraint).
/// Instead, we validate and execute all migrations directly.
/// CREATE TABLE IF NOT EXISTS ensures idempotency across hot reloads.
pub fn execute_dev_mode_migrations(
    public_key: &str,
    extension_name: &str,
    migrations: &[serde_json::Map<String, serde_json::Value>],
    state: &AppState,
) -> Result<crate::extension::database::types::MigrationResult, ExtensionError> {
    use crate::extension::database::types::MigrationResult;

    // Create context for dev mode (no CRDT triggers)
    let ctx = ExtensionSqlContext::new(public_key.to_string(), extension_name.to_string(), true);

    let mut applied_names: Vec<String> = Vec::new();

    for migration_obj in migrations {
        let migration_name = migration_obj
            .get("name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ExtensionError::ValidationError {
                reason: "Migration must have a 'name' field".to_string(),
            })?;

        let sql_statement = migration_obj
            .get("sql")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ExtensionError::ValidationError {
                reason: "Migration must have a 'sql' field".to_string(),
            })?;

        println!(
            "[EXT_MIGRATIONS/DEV] Processing migration: {}",
            migration_name
        );

        // Execute all statements using the helper function
        // CREATE TABLE IF NOT EXISTS handles idempotency for dev hot reloads
        let stmt_count = execute_migration_statements(&ctx, sql_statement, state)?;

        println!(
            "[EXT_MIGRATIONS/DEV] Migration '{}' executed ({} statements)",
            migration_name, stmt_count
        );

        applied_names.push(migration_name.to_string());
    }

    println!(
        "[EXT_MIGRATIONS/DEV] Executed {} migrations (no DB tracking in dev mode)",
        applied_names.len()
    );

    Ok(MigrationResult {
        applied_count: applied_names.len(),
        already_applied_count: 0, // Can't track in dev mode
        applied_migrations: applied_names,
    })
}
