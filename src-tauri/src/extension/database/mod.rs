// src-tauri/src/extension/database/mod.rs

pub mod executor;
pub mod helpers;
pub mod planner;
#[cfg(test)]
mod tests;

pub use helpers::{
    execute_migration_statements, execute_sql_with_context, validate_sql_table_prefix,
    ExtensionSqlContext,
};

use crate::database::core::{
    parse_sql_statements, with_connection, ValueConverter, DRIZZLE_STATEMENT_BREAKPOINT,
};
use crate::database::error::DatabaseError;
use crate::extension::core::types::ExtensionSource;
use crate::extension::database::executor::SqlExecutor;
use crate::extension::error::ExtensionError;
use crate::extension::permissions::validator::SqlPermissionValidator;
use crate::table_names::{TABLE_CRDT_MIGRATIONS, TABLE_EXTENSIONS, TABLE_EXTENSION_MIGRATIONS};
use crate::AppState;

use rusqlite::params_from_iter;
use serde_json::Value as JsonValue;
use sqlparser::ast::Statement;
use tauri::State;

/// Executes a SQL statement for an extension with full permission validation.
///
/// This is the main entry point for extension SQL execution from the frontend.
/// It validates permissions against the extension's granted permissions before executing.
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

    // Full permission validation against extension's granted permissions
    SqlPermissionValidator::validate_sql(&state, &extension.id, sql).await?;

    // Create context and delegate to helper function
    let ctx = ExtensionSqlContext::new(public_key, name, is_dev_mode);
    execute_sql_with_context(&ctx, sql, &params, state.inner())
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

        eprintln!("DEBUG: [extension_sql_select] SQL to execute: {transformed_sql}");

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

/// Splits a migration SQL content into individual statements
pub fn split_migration_statements(sql: &str) -> Vec<&str> {
    sql.split(DRIZZLE_STATEMENT_BREAKPOINT)
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect()
}

/// Registers and applies extension migrations
///
/// For production extensions:
/// 1. Validates all SQL statements (ensures only extension's own tables are accessed)
/// 2. Stores all migrations in haex_extension_migrations (synced table)
/// 3. Queries for pending migrations (NOT in haex_crdt_migrations = not applied locally)
/// 4. Applies pending migrations using extension_sql_execute (handles CRDT triggers)
/// 5. Records applied migrations in haex_crdt_migrations (local-only table)
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
        return execute_dev_mode_migrations(&public_key, &extension_name, migrations, state).await;
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
        let statements = split_migration_statements(sql_statement);

        println!(
            "[EXT_MIGRATIONS] Found {} statements in migration",
            statements.len()
        );

        // Create context for prefix validation
        // Note: We use validate_sql_table_prefix instead of SqlPermissionValidator::validate_sql
        // because migrations ARE allowed to do schema modifications (CREATE TABLE, ALTER TABLE, DROP)
        // We only need to ensure they use the correct table prefix
        let ctx = ExtensionSqlContext::new(public_key.clone(), extension_name.clone(), is_dev_mode);

        for (idx, stmt) in statements.iter().enumerate() {
            println!(
                "[EXT_MIGRATIONS] Validating statement {}/{}",
                idx + 1,
                statements.len()
            );
            if let Err(e) = validate_sql_table_prefix(&ctx, stmt) {
                println!("[EXT_MIGRATIONS] Validation FAILED: {:?}", e);
                return Err(e);
            }
            println!("[EXT_MIGRATIONS] Statement {} validated OK", idx + 1);
        }

        // Store migration in haex_extension_migrations (synced table)
        // Using SqlExecutor to ensure CRDT columns (haex_timestamp, haex_column_hlcs) are set for sync
        println!(
            "[EXT_MIGRATIONS] Storing migration '{}' in database...",
            migration_name
        );
        if let Err(e) = with_connection(&state.db, |conn| {
            let tx = conn.transaction().map_err(DatabaseError::from)?;
            let migration_id = uuid::Uuid::new_v4().to_string();

            let hlc_service = state.hlc.lock().map_err(|_| DatabaseError::MutexPoisoned {
                reason: "Failed to lock HLC service".to_string(),
            })?;

            // Use SqlExecutor to ensure CRDT columns are properly set for sync
            let insert_sql = format!(
                "INSERT OR IGNORE INTO {TABLE_EXTENSION_MIGRATIONS}
                 (id, extension_id, extension_version, migration_name, sql_statement)
                 VALUES (?, ?, ?, ?, ?)"
            );
            let params: Vec<JsonValue> = vec![
                JsonValue::String(migration_id),
                JsonValue::String(extension_id.clone()),
                JsonValue::String(extension_version.clone()),
                JsonValue::String(migration_name.to_string()),
                JsonValue::String(sql_statement.to_string()),
            ];
            SqlExecutor::execute_internal(&tx, &hlc_service, &insert_sql, &params)?;

            tx.commit().map_err(DatabaseError::from)?;
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

    // Step 2: Query pending migrations (not in haex_crdt_migrations = not applied locally)
    let pending_migrations: Vec<(String, String)> = with_connection(&state.db, |conn| {
        let mut stmt = conn.prepare(&format!(
            "SELECT m.migration_name, m.sql_statement FROM {TABLE_EXTENSION_MIGRATIONS} m
             WHERE m.extension_id = ?1 AND m.haex_tombstone = 0
               AND NOT EXISTS (
                   SELECT 1 FROM {TABLE_CRDT_MIGRATIONS} c
                   WHERE c.extension_id = m.extension_id
                     AND c.migration_name = m.migration_name
               )
             ORDER BY m.migration_name ASC"
        ))?;

        let rows = stmt.query_map([&extension_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;

        let result: Result<Vec<_>, _> = rows.collect();
        Ok(result.map_err(DatabaseError::from)?)
    })?;

    // Get count of already applied migrations (in haex_crdt_migrations)
    let already_applied_count: usize = with_connection(&state.db, |conn| {
        let count: i64 = conn.query_row(
            &format!(
                "SELECT COUNT(*) FROM {TABLE_CRDT_MIGRATIONS}
                 WHERE extension_id = ?1"
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
            .map(|(n, _)| n)
            .collect::<Vec<_>>()
    );

    // Step 3: Apply each pending migration using extension_sql_execute
    let mut applied_names: Vec<String> = Vec::new();

    // Create context for execution (validation already done above)
    // Production extensions need CRDT triggers (is_dev_mode = false)
    let exec_ctx = ExtensionSqlContext::new(public_key.clone(), extension_name.clone(), false);

    for (migration_name, sql_content) in &pending_migrations {
        println!("[EXT_MIGRATIONS] Applying migration: {}", migration_name);

        // Execute all statements using the helper function
        let stmt_count = execute_migration_statements(&exec_ctx, sql_content, state.inner())?;

        println!(
            "[EXT_MIGRATIONS] Migration '{}' executed ({} statements)",
            migration_name, stmt_count
        );

        // Record in haex_crdt_migrations (local-only, for tracking on this device)
        with_connection(&state.db, |conn| {
            let local_migration_id = uuid::Uuid::new_v4().to_string();
            conn.execute(
                &format!(
                    "INSERT OR IGNORE INTO {TABLE_CRDT_MIGRATIONS}
                     (id, extension_id, migration_name, migration_content, applied_at)
                     VALUES (?1, ?2, ?3, ?4, datetime('now'))"
                ),
                rusqlite::params![
                    local_migration_id,
                    extension_id,
                    migration_name,
                    sql_content
                ],
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
    migrations: Vec<serde_json::Map<String, JsonValue>>,
    state: State<'_, AppState>,
) -> Result<MigrationResult, ExtensionError> {
    // Create context for dev mode (no CRDT triggers)
    let ctx = ExtensionSqlContext::new(public_key.to_string(), extension_name.to_string(), true);

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

        // Execute all statements using the helper function
        // CREATE TABLE IF NOT EXISTS handles idempotency for dev hot reloads
        let stmt_count = execute_migration_statements(&ctx, sql_statement, state.inner())?;

        println!(
            "[EXT_MIGRATIONS/DEV] Migration '{}' executed ({} statements)",
            migration_name, stmt_count
        );

        applied_names.push(migration_name.to_string());
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

/// Applies pending extension migrations that were synced from another device
///
/// After sync, the haex_extension_migrations table contains migrations from other devices.
/// This command checks which migrations are NOT yet applied locally (not in haex_crdt_migrations)
/// and executes those migrations to create the extension tables.
///
/// Key difference from old approach:
/// - OLD: Checked `applied_at IS NULL` in haex_extension_migrations (but applied_at syncs!)
/// - NEW: Checks if migration is NOT in haex_crdt_migrations (local-only table)
///
/// Note: Prefix validation is done to ensure migrations only affect extension's own tables.
#[tauri::command]
pub fn apply_synced_extension_migrations(
    state: State<'_, AppState>,
) -> Result<MigrationResult, ExtensionError> {
    println!("[SYNC_MIGRATIONS] Applying synced extension migrations...");

    // Query all migrations from haex_extension_migrations that are NOT in haex_crdt_migrations
    // haex_crdt_migrations is local-only and tracks which migrations have been applied on THIS device
    let pending_migrations: Vec<(String, String, String, String, String)> =
        with_connection(&state.db, |conn| {
            let mut stmt = conn.prepare(&format!(
                "SELECT m.extension_id, m.migration_name, m.sql_statement, e.public_key, e.name
                 FROM {TABLE_EXTENSION_MIGRATIONS} m
                 JOIN {TABLE_EXTENSIONS} e ON m.extension_id = e.id
                 WHERE m.haex_tombstone = 0
                   AND NOT EXISTS (
                       SELECT 1 FROM {TABLE_CRDT_MIGRATIONS} c
                       WHERE c.extension_id = m.extension_id
                         AND c.migration_name = m.migration_name
                   )
                 ORDER BY m.extension_id ASC, m.migration_name ASC"
            ))?;

            let rows = stmt.query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?, // extension_id
                    row.get::<_, String>(1)?, // migration_name
                    row.get::<_, String>(2)?, // sql_statement
                    row.get::<_, String>(3)?, // public_key
                    row.get::<_, String>(4)?, // name
                ))
            })?;

            let result: Result<Vec<_>, _> = rows.collect();
            Ok(result.map_err(DatabaseError::from)?)
        })?;

    if pending_migrations.is_empty() {
        println!("[SYNC_MIGRATIONS] No pending migrations to apply");
        return Ok(MigrationResult {
            applied_count: 0,
            already_applied_count: 0,
            applied_migrations: vec![],
        });
    }

    println!(
        "[SYNC_MIGRATIONS] Found {} pending migrations",
        pending_migrations.len()
    );

    let mut applied_names: Vec<String> = Vec::new();

    for (extension_id, migration_name, sql_content, public_key, ext_name) in &pending_migrations {
        println!("[SYNC_MIGRATIONS] Applying migration: {}", migration_name);

        // Create context for production extensions (with CRDT triggers)
        let ctx = ExtensionSqlContext::new(public_key.clone(), ext_name.clone(), false);

        // Execute all statements using the helper function
        let stmt_count = execute_migration_statements(&ctx, sql_content, state.inner())?;

        println!(
            "[SYNC_MIGRATIONS] Migration '{}' executed ({} statements)",
            migration_name, stmt_count
        );

        // Record in haex_crdt_migrations (local-only) to mark as applied on this device
        with_connection(&state.db, |conn| {
            let local_migration_id = uuid::Uuid::new_v4().to_string();
            conn.execute(
                &format!(
                    "INSERT OR IGNORE INTO {TABLE_CRDT_MIGRATIONS}
                     (id, extension_id, migration_name, migration_content, applied_at)
                     VALUES (?1, ?2, ?3, ?4, datetime('now'))"
                ),
                rusqlite::params![
                    local_migration_id,
                    extension_id,
                    migration_name,
                    sql_content
                ],
            )
            .map_err(DatabaseError::from)?;
            Ok::<(), DatabaseError>(())
        })?;

        applied_names.push(migration_name.clone());
        println!(
            "[SYNC_MIGRATIONS] Migration '{}' applied successfully",
            migration_name
        );
    }

    println!(
        "[SYNC_MIGRATIONS] ✅ Successfully applied {} synced migrations",
        applied_names.len()
    );

    Ok(MigrationResult {
        applied_count: applied_names.len(),
        already_applied_count: 0,
        applied_migrations: applied_names,
    })
}
