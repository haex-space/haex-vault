// src-tauri/src/extension/database/commands.rs
//!
//! Tauri commands for extension database operations
//!
//! These commands work for both WebView and iframe extensions:
//! - WebView: extension_id is resolved from the window context
//! - iframe: extension_id is resolved from public_key/name parameters
//!           (verified by frontend via origin check)

use crate::crdt::transformer::CrdtTransformer;
use crate::database::core::{parse_sql_statements, with_connection, ValueConverter};
use crate::database::error::DatabaseError;
use crate::extension::core::types::ExtensionSource;
use crate::extension::database::executor::SqlExecutor;
use crate::extension::database::helpers::{
    execute_dev_mode_migrations, execute_migration_statements, execute_sql_with_context,
    split_migration_statements, validate_sql_table_prefix, ExtensionSqlContext,
};
use crate::extension::database::queries::{
    SQL_COUNT_APPLIED_MIGRATIONS, SQL_GET_PENDING_MIGRATIONS, SQL_GET_SYNCED_PENDING_MIGRATIONS,
    SQL_INSERT_CRDT_MIGRATION, SQL_INSERT_EXTENSION_MIGRATION,
};
use crate::extension::database::types::{DatabaseQueryResult, MigrationResult};
use crate::extension::error::ExtensionError;
use crate::extension::permissions::validator::SqlPermissionValidator;
use crate::extension::utils::resolve_extension_id;
use crate::AppState;

use rusqlite::params_from_iter;
use serde_json::Value as JsonValue;
use sqlparser::ast::Statement;
use tauri::{State, WebviewWindow};

/// Executes a SQL statement for an extension with full permission validation.
#[tauri::command]
pub async fn extension_database_execute(
    window: WebviewWindow,
    state: State<'_, AppState>,
    sql: String,
    params: Vec<JsonValue>,
    // Optional parameters for iframe mode (verified by frontend via origin)
    public_key: Option<String>,
    name: Option<String>,
) -> Result<DatabaseQueryResult, ExtensionError> {
    let extension_id = resolve_extension_id(&window, &state, public_key, name)?;

    let extension = state
        .extension_manager
        .get_extension(&extension_id)
        .ok_or_else(|| ExtensionError::ValidationError {
            reason: format!("Extension with ID {} not found", extension_id),
        })?;

    let is_dev_mode = matches!(extension.source, ExtensionSource::Development { .. });

    SqlPermissionValidator::validate_sql(&state, &extension_id, &sql).await?;

    let ctx = ExtensionSqlContext::new(
        extension.manifest.public_key.clone(),
        extension.manifest.name.clone(),
        is_dev_mode,
    );
    let rows = execute_sql_with_context(&ctx, &sql, &params, state.inner())?;

    Ok(DatabaseQueryResult {
        rows_affected: rows.len(),
        rows,
        last_insert_id: None,
    })
}

/// Executes a SELECT statement for an extension
#[tauri::command]
pub async fn extension_database_query(
    window: WebviewWindow,
    state: State<'_, AppState>,
    sql: String,
    params: Vec<JsonValue>,
    // Optional parameters for iframe mode (verified by frontend via origin)
    public_key: Option<String>,
    name: Option<String>,
) -> Result<DatabaseQueryResult, ExtensionError> {
    eprintln!("=== [EXT_QUERY] ENTRY === sql: {}", sql);
    let extension_id = resolve_extension_id(&window, &state, public_key.clone(), name.clone())?;
    eprintln!("[EXT_QUERY] extension_id: {}, public_key: {:?}, name: {:?}", extension_id, public_key, name);

    let extension = state
        .extension_manager
        .get_extension(&extension_id)
        .ok_or_else(|| ExtensionError::ValidationError {
            reason: format!("Extension with ID {} not found", extension_id),
        })?;

    SqlPermissionValidator::validate_sql(&state, &extension_id, &sql).await?;

    let placeholder_count = sql.matches('?').count();
    if placeholder_count != params.len() {
        return Err(ExtensionError::Database {
            source: DatabaseError::ParameterMismatchError {
                expected: placeholder_count,
                provided: params.len(),
                sql: sql.to_string(),
            },
        });
    }

    let mut ast_vec = parse_sql_statements(&sql)?;

    if ast_vec.is_empty() {
        return Ok(DatabaseQueryResult {
            rows: vec![],
            rows_affected: 0,
            last_insert_id: None,
        });
    }

    for stmt in &ast_vec {
        if !matches!(stmt, Statement::Query(_)) {
            return Err(ExtensionError::Database {
                source: DatabaseError::ExecutionError {
                    sql: sql.to_string(),
                    reason: "Only SELECT statements are allowed in extension_database_query"
                        .to_string(),
                    table: None,
                },
            });
        }
    }

    let rows = with_connection(&state.db, |conn| {
        let sql_params = ValueConverter::convert_params(&params)?;
        let mut stmt_to_execute = ast_vec.pop().unwrap();

        // Apply CRDT tombstone filter to SELECT queries
        // This ensures tombstoned (soft-deleted) rows are filtered out
        if let Statement::Query(ref mut query) = stmt_to_execute {
            let transformer = CrdtTransformer::new();
            transformer.transform_query(query);
        }

        let transformed_sql = stmt_to_execute.to_string();
        eprintln!("[EXT_QUERY] Original SQL: {}", sql);
        eprintln!("[EXT_QUERY] Transformed SQL: {}", transformed_sql);

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
    .map_err(ExtensionError::from)?;

    eprintln!("[EXT_QUERY] Result: {} rows returned", rows.len());
    Ok(DatabaseQueryResult {
        rows,
        rows_affected: 0,
        last_insert_id: None,
    })
}

/// Registers and applies extension migrations
#[tauri::command]
pub async fn extension_database_register_migrations(
    window: WebviewWindow,
    state: State<'_, AppState>,
    extension_version: String,
    migrations: Vec<serde_json::Map<String, JsonValue>>,
    // Optional parameters for iframe mode (verified by frontend via origin)
    public_key: Option<String>,
    name: Option<String>,
) -> Result<MigrationResult, ExtensionError> {
    let extension_id = resolve_extension_id(&window, &state, public_key, name)?;

    let extension = state
        .extension_manager
        .get_extension(&extension_id)
        .ok_or_else(|| ExtensionError::ValidationError {
            reason: format!("Extension with ID {} not found", extension_id),
        })?;

    let ext_public_key = extension.manifest.public_key.clone();
    let ext_name = extension.manifest.name.clone();
    let is_dev_mode = matches!(extension.source, ExtensionSource::Development { .. });

    // Dev mode: Execute migrations directly without database tracking
    if is_dev_mode {
        return execute_dev_mode_migrations(&ext_public_key, &ext_name, &migrations, state.inner())
            .map_err(Into::into);
    }

    // Production mode: Store and track migrations in database
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

        let statements = split_migration_statements(sql_statement);
        let ctx = ExtensionSqlContext::new(ext_public_key.clone(), ext_name.clone(), is_dev_mode);

        for stmt in statements.iter() {
            validate_sql_table_prefix(&ctx, stmt)?;
        }

        // Store migration in synced table
        with_connection(&state.db, |conn| {
            let tx = conn.transaction().map_err(DatabaseError::from)?;
            let migration_id = uuid::Uuid::new_v4().to_string();

            let hlc_service = state.hlc.lock().map_err(|_| DatabaseError::MutexPoisoned {
                reason: "Failed to lock HLC service".to_string(),
            })?;

            let params: Vec<JsonValue> = vec![
                JsonValue::String(migration_id),
                JsonValue::String(extension_id.clone()),
                JsonValue::String(extension_version.clone()),
                JsonValue::String(migration_name.to_string()),
                JsonValue::String(sql_statement.to_string()),
            ];
            SqlExecutor::execute_internal(&tx, &hlc_service, &SQL_INSERT_EXTENSION_MIGRATION, &params)?;

            tx.commit().map_err(DatabaseError::from)?;
            Ok::<(), DatabaseError>(())
        })?;
    }

    // Query pending migrations
    let pending_migrations: Vec<(String, String)> = with_connection(&state.db, |conn| {
        let mut stmt = conn.prepare(&SQL_GET_PENDING_MIGRATIONS)?;
        let rows = stmt.query_map([&extension_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(DatabaseError::from)
    })?;

    let already_applied_count: usize = with_connection(&state.db, |conn| {
        let count: i64 =
            conn.query_row(&SQL_COUNT_APPLIED_MIGRATIONS, [&extension_id], |row| {
                row.get(0)
            })?;
        Ok(count as usize)
    })?;

    if pending_migrations.is_empty() {
        return Ok(MigrationResult {
            applied_count: 0,
            already_applied_count,
            applied_migrations: vec![],
        });
    }

    // Apply pending migrations
    let mut applied_names: Vec<String> = Vec::new();
    let exec_ctx = ExtensionSqlContext::new(ext_public_key.clone(), ext_name.clone(), false);

    for (migration_name, sql_content) in &pending_migrations {
        execute_migration_statements(&exec_ctx, sql_content, state.inner())?;

        // Record in local CRDT migrations table
        with_connection(&state.db, |conn| {
            let local_migration_id = uuid::Uuid::new_v4().to_string();
            conn.execute(
                &SQL_INSERT_CRDT_MIGRATION,
                rusqlite::params![local_migration_id, extension_id, migration_name, sql_content],
            )
            .map_err(DatabaseError::from)?;
            Ok::<(), DatabaseError>(())
        })?;

        applied_names.push(migration_name.clone());
    }

    Ok(MigrationResult {
        applied_count: applied_names.len(),
        already_applied_count,
        applied_migrations: applied_names,
    })
}

/// Applies pending extension migrations that were synced from another device
#[tauri::command]
pub fn apply_synced_extension_migrations(
    state: State<'_, AppState>,
) -> Result<MigrationResult, ExtensionError> {
    // Debug: Log counts in both tables before joining
    with_connection(&state.db, |conn| {
        let ext_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM haex_extensions WHERE haex_tombstone = 0",
                [],
                |row| row.get(0),
            )
            .unwrap_or(-1);
        let mig_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM haex_extension_migrations WHERE haex_tombstone = 0",
                [],
                |row| row.get(0),
            )
            .unwrap_or(-1);
        let applied_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM haex_crdt_migrations", [], |row| {
                row.get(0)
            })
            .unwrap_or(-1);
        eprintln!(
            "[SYNC MIGRATIONS] Table counts - haex_extensions: {}, haex_extension_migrations: {}, haex_crdt_migrations (applied): {}",
            ext_count, mig_count, applied_count
        );
        Ok::<(), DatabaseError>(())
    })
    .ok();

    let pending_migrations: Vec<(String, String, String, String, String)> =
        with_connection(&state.db, |conn| {
            let mut stmt = conn.prepare(&SQL_GET_SYNCED_PENDING_MIGRATIONS)?;
            let rows = stmt.query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                ))
            })?;
            rows.collect::<Result<Vec<_>, _>>()
                .map_err(DatabaseError::from)
        })?;

    eprintln!(
        "[SYNC MIGRATIONS] Found {} pending migrations after JOIN",
        pending_migrations.len()
    );

    if pending_migrations.is_empty() {
        return Ok(MigrationResult {
            applied_count: 0,
            already_applied_count: 0,
            applied_migrations: vec![],
        });
    }

    let mut applied_names: Vec<String> = Vec::new();

    for (extension_id, migration_name, sql_content, public_key, ext_name) in &pending_migrations {
        let ctx = ExtensionSqlContext::new(public_key.clone(), ext_name.clone(), false);
        execute_migration_statements(&ctx, sql_content, state.inner())?;

        with_connection(&state.db, |conn| {
            let local_migration_id = uuid::Uuid::new_v4().to_string();
            conn.execute(
                &SQL_INSERT_CRDT_MIGRATION,
                rusqlite::params![local_migration_id, extension_id, migration_name, sql_content],
            )
            .map_err(DatabaseError::from)?;
            Ok::<(), DatabaseError>(())
        })?;

        applied_names.push(migration_name.clone());
    }

    Ok(MigrationResult {
        applied_count: applied_names.len(),
        already_applied_count: 0,
        applied_migrations: applied_names,
    })
}
