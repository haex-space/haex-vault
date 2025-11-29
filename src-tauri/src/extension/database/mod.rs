// src-tauri/src/extension/database/mod.rs

pub mod executor;
pub mod planner;
#[cfg(test)]
mod tests;

use crate::crdt::transformer::CrdtTransformer;
use crate::crdt::trigger;
use crate::database::core::{parse_sql_statements, with_connection, ValueConverter};
use crate::database::error::DatabaseError;
use crate::extension::core::types::ExtensionSource;
use crate::extension::database::executor::SqlExecutor;
use crate::extension::error::ExtensionError;
use crate::extension::permissions::validator::SqlPermissionValidator;
use crate::table_names::TABLE_EXTENSION_MIGRATIONS;
use crate::AppState;

use rusqlite::params_from_iter;
use serde_json::Value as JsonValue;
use sqlparser::ast::Statement;
use tauri::State;

#[tauri::command]
pub async fn extension_sql_execute(
    sql: &str,
    params: Vec<JsonValue>,
    public_key: String,
    name: String,
    state: State<'_, AppState>,
) -> Result<Vec<Vec<JsonValue>>, ExtensionError> {
    // Get extension to retrieve its ID and check if dev mode
    let extension = state
        .extension_manager
        .get_extension_by_public_key_and_name(&public_key, &name)?
        .ok_or_else(|| ExtensionError::NotFound {
            public_key: public_key.clone(),
            name: name.clone(),
        })?;

    let is_dev_mode = matches!(extension.source, ExtensionSource::Development { .. });

    // Permission check
    SqlPermissionValidator::validate_sql(&state, &extension.id, sql).await?;

    // Parameter validation
    validate_params(sql, &params)?;

    // SQL parsing
    let mut ast_vec = parse_sql_statements(sql)?;

    if ast_vec.len() != 1 {
        return Err(ExtensionError::Database {
            source: DatabaseError::ExecutionError {
                sql: sql.to_string(),
                reason: "extension_sql_execute should only receive a single SQL statement"
                    .to_string(),
                table: None,
            },
        });
    }

    let mut statement = ast_vec.pop().unwrap();

    // If this is a SELECT statement, delegate to extension_sql_select
    if matches!(statement, Statement::Query(_)) {
        return extension_sql_select(sql, params, public_key, name, state).await;
    }

    // Check if statement has RETURNING clause
    let has_returning = crate::database::core::statement_has_returning(&statement);

    // Database operation
    with_connection(&state.db, |conn| {
        let tx = conn.transaction().map_err(DatabaseError::from)?;

        let transformer = CrdtTransformer::new();

        // Get HLC service reference
        let hlc_service = state.hlc.lock().map_err(|_| DatabaseError::MutexPoisoned {
            reason: "Failed to lock HLC service".to_string(),
        })?;

        // Generate HLC timestamp
        let hlc_timestamp =
            hlc_service
                .new_timestamp_and_persist(&tx)
                .map_err(|e| DatabaseError::HlcError {
                    reason: e.to_string(),
                })?;

        // Transform statement
        transformer.transform_execute_statement(&mut statement, &hlc_timestamp)?;

        // Convert parameters to references
        let sql_values = ValueConverter::convert_params(&params)?;
        let param_refs: Vec<&dyn rusqlite::ToSql> = sql_values
            .iter()
            .map(|v| v as &dyn rusqlite::ToSql)
            .collect();

        let result = if has_returning {
            // Use query_internal for statements with RETURNING
            let (_, rows) = SqlExecutor::query_internal_typed(
                &tx,
                &hlc_service,
                &statement.to_string(),
                &param_refs,
            )?;
            rows
        } else {
            // Use execute_internal for statements without RETURNING
            SqlExecutor::execute_internal_typed(
                &tx,
                &hlc_service,
                &statement.to_string(),
                &param_refs,
            )?;
            vec![]
        };

        // Handle CREATE TABLE trigger setup (only for production extensions)
        // Dev mode extensions don't get CRDT triggers - their tables are local-only and not synced
        if let Statement::CreateTable(ref create_table_details) = statement {
            let raw_name = create_table_details.name.to_string();
            let table_name_str = raw_name.trim_matches('"').trim_matches('`').to_string();

            if is_dev_mode {
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
                println!("Triggers for table '{}' successfully created.", table_name_str);
            }
        }

        // Commit transaction
        tx.commit().map_err(DatabaseError::from)?;

        Ok(result)
    })
    .map_err(ExtensionError::from)
}

#[tauri::command]
pub async fn extension_sql_select(
    sql: &str,
    params: Vec<JsonValue>,
    public_key: String,
    name: String,
    state: State<'_, AppState>,
) -> Result<Vec<Vec<JsonValue>>, ExtensionError> {
    // Get extension to retrieve its ID
    let extension = state
        .extension_manager
        .get_extension_by_public_key_and_name(&public_key, &name)?
        .ok_or_else(|| ExtensionError::NotFound {
            public_key: public_key.clone(),
            name: name.clone(),
        })?;

    // Permission check
    SqlPermissionValidator::validate_sql(&state, &extension.id, sql).await?;

    // Parameter validation
    validate_params(sql, &params)?;

    // SQL parsing
    let mut ast_vec = parse_sql_statements(sql)?;

    if ast_vec.is_empty() {
        return Ok(vec![]);
    }

    // Validate that all statements are queries
    for stmt in &ast_vec {
        if !matches!(stmt, Statement::Query(_)) {
            return Err(ExtensionError::Database {
                source: DatabaseError::ExecutionError {
                    sql: sql.to_string(),
                    reason: "Only SELECT statements are allowed in extension_sql_select"
                        .to_string(),
                    table: None,
                },
            });
        }
    }

    // Database operation - return Vec<Vec<JsonValue>> like sql_select_with_crdt
    with_connection(&state.db, |conn| {
        let sql_params = ValueConverter::convert_params(&params)?;
        let stmt_to_execute = ast_vec.pop().unwrap();
        let transformed_sql = stmt_to_execute.to_string();

        // Prepare and execute query
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
    .map_err(ExtensionError::from)
}

/// Validiert Parameter gegen SQL-Platzhalter
fn validate_params(sql: &str, params: &[JsonValue]) -> Result<(), DatabaseError> {
    let total_placeholders = count_sql_placeholders(sql);

    if total_placeholders != params.len() {
        return Err(DatabaseError::ParameterMismatchError {
            expected: total_placeholders,
            provided: params.len(),
            sql: sql.to_string(),
        });
    }

    Ok(())
}

/// Zählt SQL-Platzhalter (verbesserte Version)
fn count_sql_placeholders(sql: &str) -> usize {
    sql.matches('?').count()
}

/// Result of applying extension migrations
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MigrationResult {
    pub applied_count: usize,
    pub already_applied_count: usize,
    pub applied_migrations: Vec<String>,
}

const STATEMENT_BREAKPOINT: &str = "--> statement-breakpoint";

/// Registers and applies extension migrations
///
/// For production extensions:
/// 1. Validates all SQL statements (ensures only extension's own tables are accessed)
/// 2. Stores all migrations in the database with applied_at = NULL
/// 3. Queries for pending migrations (applied_at IS NULL) sorted by name
/// 4. Applies pending migrations using extension_sql_execute (handles CRDT triggers)
/// 5. Marks successful migrations with applied_at timestamp
///
/// For dev extensions:
/// - Validates and executes all migrations directly without database tracking
/// - Uses CREATE TABLE IF NOT EXISTS to skip already-created tables
/// - No foreign key constraint issues since dev extensions aren't in haex_extensions table
#[tauri::command]
pub async fn register_extension_migrations(
    public_key: String,
    extension_name: String,
    extension_version: String,
    migrations: Vec<serde_json::Map<String, JsonValue>>,
    state: State<'_, AppState>,
) -> Result<MigrationResult, ExtensionError> {
    println!(
        "[EXT_MIGRATIONS] register_extension_migrations called for {}::{}",
        public_key, extension_name
    );

    // Get extension to retrieve its ID and check if dev mode
    let extension = state
        .extension_manager
        .get_extension_by_public_key_and_name(&public_key, &extension_name)?
        .ok_or_else(|| ExtensionError::NotFound {
            public_key: public_key.clone(),
            name: extension_name.clone(),
        })?;

    let extension_id = extension.id.clone();
    let is_dev_mode = matches!(extension.source, ExtensionSource::Development { .. });

    // Dev mode: Execute migrations directly without database tracking
    if is_dev_mode {
        println!("[EXT_MIGRATIONS] Dev mode detected - executing migrations without DB tracking");
        return execute_dev_mode_migrations(
            &public_key,
            &extension_name,
            &extension_id,
            migrations,
            state,
        )
        .await;
    }

    // Production mode: Store and track migrations in database
    // Step 1: Validate and store all migrations with applied_at = NULL
    for migration_obj in &migrations {
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

        println!("[EXT_MIGRATIONS] Validating migration: {}", migration_name);

        // Validate each SQL statement in the migration
        let statements: Vec<&str> = sql_statement
            .split(STATEMENT_BREAKPOINT)
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .collect();

        println!(
            "[EXT_MIGRATIONS] Found {} statements in migration",
            statements.len()
        );

        for (idx, stmt) in statements.iter().enumerate() {
            println!(
                "[EXT_MIGRATIONS] Validating statement {}/{}",
                idx + 1,
                statements.len()
            );
            if let Err(e) =
                SqlPermissionValidator::validate_sql(&state, &extension_id, stmt).await
            {
                println!("[EXT_MIGRATIONS] Validation FAILED: {:?}", e);
                return Err(e);
            }
            println!("[EXT_MIGRATIONS] Statement {} validated OK", idx + 1);
        }

        // Store migration with applied_at = NULL (upsert to avoid duplicates)
        println!(
            "[EXT_MIGRATIONS] Storing migration '{}' in database...",
            migration_name
        );
        if let Err(e) = with_connection(&state.db, |conn| {
            let migration_id = uuid::Uuid::new_v4().to_string();

            // Try to insert, ignore if already exists (unique constraint on extension_id + migration_name)
            conn.execute(
                &format!(
                    "INSERT OR IGNORE INTO {TABLE_EXTENSION_MIGRATIONS}
                     (id, extension_id, extension_version, migration_name, sql_statement, applied_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, NULL)"
                ),
                rusqlite::params![
                    migration_id,
                    extension_id,
                    extension_version,
                    migration_name,
                    sql_statement,
                ],
            )
            .map_err(DatabaseError::from)?;

            Ok::<(), DatabaseError>(())
        }) {
            println!("[EXT_MIGRATIONS] Failed to store migration: {:?}", e);
            return Err(e.into());
        }
        println!(
            "[EXT_MIGRATIONS] Migration '{}' stored successfully",
            migration_name
        );
    }

    println!("[EXT_MIGRATIONS] All migrations validated and stored");

    // Step 2: Query pending migrations sorted by name
    let pending_migrations: Vec<(String, String, String)> = with_connection(&state.db, |conn| {
        let mut stmt = conn.prepare(&format!(
            "SELECT id, migration_name, sql_statement FROM {TABLE_EXTENSION_MIGRATIONS}
             WHERE extension_id = ?1 AND applied_at IS NULL AND haex_tombstone = 0
             ORDER BY migration_name ASC"
        ))?;

        let rows = stmt.query_map([&extension_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?;

        let result: Result<Vec<_>, _> = rows.collect();
        Ok(result.map_err(DatabaseError::from)?)
    })?;

    // Get count of already applied migrations
    let already_applied_count: usize = with_connection(&state.db, |conn| {
        let count: i64 = conn.query_row(
            &format!(
                "SELECT COUNT(*) FROM {TABLE_EXTENSION_MIGRATIONS}
                 WHERE extension_id = ?1 AND applied_at IS NOT NULL AND haex_tombstone = 0"
            ),
            [&extension_id],
            |row| row.get(0),
        )?;
        Ok(count as usize)
    })?;

    if pending_migrations.is_empty() {
        println!("[EXT_MIGRATIONS] No pending migrations");
        return Ok(MigrationResult {
            applied_count: 0,
            already_applied_count,
            applied_migrations: vec![],
        });
    }

    println!(
        "[EXT_MIGRATIONS] Found {} pending migrations: {:?}",
        pending_migrations.len(),
        pending_migrations
            .iter()
            .map(|(_, n, _)| n)
            .collect::<Vec<_>>()
    );

    // Step 3: Apply each pending migration using extension_sql_execute
    let mut applied_names: Vec<String> = Vec::new();

    for (migration_id, migration_name, sql_content) in &pending_migrations {
        println!("[EXT_MIGRATIONS] Applying migration: {}", migration_name);

        // Split SQL by statement breakpoint and execute each statement
        let statements: Vec<&str> = sql_content
            .split(STATEMENT_BREAKPOINT)
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .collect();

        for (idx, statement) in statements.iter().enumerate() {
            println!(
                "[EXT_MIGRATIONS] Executing statement {}/{} of {}",
                idx + 1,
                statements.len(),
                migration_name
            );

            // Use extension_sql_execute which handles CRDT triggers for CREATE TABLE
            extension_sql_execute(
                statement,
                vec![],
                public_key.clone(),
                extension_name.clone(),
                state.clone(),
            )
            .await?;
        }

        // Step 4: Mark migration as applied
        with_connection(&state.db, |conn| {
            conn.execute(
                &format!(
                    "UPDATE {TABLE_EXTENSION_MIGRATIONS}
                     SET applied_at = datetime('now')
                     WHERE id = ?1"
                ),
                rusqlite::params![migration_id],
            )
            .map_err(DatabaseError::from)?;

            Ok::<(), DatabaseError>(())
        })?;

        applied_names.push(migration_name.clone());
        println!(
            "[EXT_MIGRATIONS] Migration '{}' applied successfully",
            migration_name
        );
    }

    println!(
        "[EXT_MIGRATIONS] ✅ Successfully applied {} migrations",
        applied_names.len()
    );

    Ok(MigrationResult {
        applied_count: applied_names.len(),
        already_applied_count,
        applied_migrations: applied_names,
    })
}

/// Execute migrations for dev mode extensions without database tracking
///
/// Dev extensions are not persisted to haex_extensions table, so we cannot
/// store migrations in haex_extension_migrations (foreign key constraint).
/// Instead, we validate and execute all migrations directly.
/// CREATE TABLE IF NOT EXISTS ensures idempotency across hot reloads.
async fn execute_dev_mode_migrations(
    public_key: &str,
    extension_name: &str,
    extension_id: &str,
    migrations: Vec<serde_json::Map<String, JsonValue>>,
    state: State<'_, AppState>,
) -> Result<MigrationResult, ExtensionError> {
    let mut applied_names: Vec<String> = Vec::new();

    for migration_obj in &migrations {
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

        // Split and validate each statement
        let statements: Vec<&str> = sql_statement
            .split(STATEMENT_BREAKPOINT)
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .collect();

        println!(
            "[EXT_MIGRATIONS/DEV] Found {} statements",
            statements.len()
        );

        // Validate all statements first
        for (idx, stmt) in statements.iter().enumerate() {
            println!(
                "[EXT_MIGRATIONS/DEV] Validating statement {}/{}",
                idx + 1,
                statements.len()
            );
            if let Err(e) =
                SqlPermissionValidator::validate_sql(&state, extension_id, stmt).await
            {
                println!("[EXT_MIGRATIONS/DEV] Validation FAILED: {:?}", e);
                return Err(e);
            }
        }

        // Execute all statements (CREATE TABLE IF NOT EXISTS handles idempotency)
        for (idx, statement) in statements.iter().enumerate() {
            println!(
                "[EXT_MIGRATIONS/DEV] Executing statement {}/{}",
                idx + 1,
                statements.len()
            );

            extension_sql_execute(
                statement,
                vec![],
                public_key.to_string(),
                extension_name.to_string(),
                state.clone(),
            )
            .await?;
        }

        applied_names.push(migration_name.to_string());
        println!(
            "[EXT_MIGRATIONS/DEV] Migration '{}' executed",
            migration_name
        );
    }

    println!(
        "[EXT_MIGRATIONS/DEV] ✅ Executed {} migrations (no DB tracking in dev mode)",
        applied_names.len()
    );

    Ok(MigrationResult {
        applied_count: applied_names.len(),
        already_applied_count: 0, // Can't track in dev mode
        applied_migrations: applied_names,
    })
}
