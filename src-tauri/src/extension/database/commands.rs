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
use crate::event_names::EVENT_CRDT_DIRTY_TABLES_CHANGED;
use crate::extension::database::executor::SqlExecutor;
use crate::extension::database::helpers::{
    execute_migration_statements, execute_sql_with_context, is_allowed_pragma,
    is_pragma_statement, split_migration_statements, validate_sql_table_prefix,
    ExtensionSqlContext,
};
use crate::extension::database::queries::{
    SQL_COUNT_APPLIED_MIGRATIONS, SQL_GET_PENDING_MIGRATIONS, SQL_GET_SYNCED_PENDING_MIGRATIONS,
    SQL_INSERT_CRDT_MIGRATION, SQL_INSERT_EXTENSION_MIGRATION,
};
use crate::extension::database::types::{DatabaseQueryResult, MigrationResult};
use crate::extension::error::ExtensionError;
use crate::extension::limits::LimitError;
use crate::extension::permissions::validator::SqlPermissionValidator;
use crate::extension::utils::resolve_extension_id;
use crate::AppState;

use rusqlite::params_from_iter;
use serde_json::Value as JsonValue;
use sqlparser::ast::Statement;
use tauri::{Emitter, Manager, State, WebviewWindow};

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

    // Get extension limits
    let limits = with_connection(&state.db, |conn| {
        state.limits.get_limits(conn, &extension_id)
    })?;

    // Validate query size
    state
        .limits
        .database()
        .validate_query_size(&sql, &limits.database)
        .map_err(|e: LimitError| ExtensionError::Database { source: e.into() })?;

    // Acquire concurrent query slot (released when guard is dropped)
    let _query_guard = state
        .limits
        .database()
        .acquire_query_slot(&extension_id, &limits.database)
        .map_err(|e: LimitError| ExtensionError::Database { source: e.into() })?;

    SqlPermissionValidator::validate_sql(&state, &extension_id, &sql).await?;

    let ctx = ExtensionSqlContext::new(
        extension.manifest.public_key.clone(),
        extension.manifest.name.clone(),
    );
    let rows = execute_sql_with_context(&ctx, &sql, &params, state.inner())?;

    // Emit event to notify frontend that dirty tables may have changed
    // This triggers the sync orchestrator to push changes to the server
    let app_handle = window.app_handle();
    let _ = app_handle.emit(EVENT_CRDT_DIRTY_TABLES_CHANGED, ());

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

    let _extension = state
        .extension_manager
        .get_extension(&extension_id)
        .ok_or_else(|| ExtensionError::ValidationError {
            reason: format!("Extension with ID {} not found", extension_id),
        })?;

    // Get extension limits
    let limits = with_connection(&state.db, |conn| {
        state.limits.get_limits(conn, &extension_id)
    })?;

    // Validate query size
    state
        .limits
        .database()
        .validate_query_size(&sql, &limits.database)
        .map_err(|e: LimitError| ExtensionError::Database { source: e.into() })?;

    // Acquire concurrent query slot (released when guard is dropped)
    let _query_guard = state
        .limits
        .database()
        .acquire_query_slot(&extension_id, &limits.database)
        .map_err(|e: LimitError| ExtensionError::Database { source: e.into() })?;

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

    // Store max_result_rows for use inside the closure
    let max_result_rows = limits.database.max_result_rows;

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
            // Check result row limit
            if result_vec.len() as i64 >= max_result_rows {
                return Err(DatabaseError::LimitExceeded {
                    reason: format!(
                        "Query result exceeds maximum rows: {} (limit: {})",
                        result_vec.len() + 1,
                        max_result_rows
                    ),
                });
            }

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

    // Store and track migrations in database
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
        let ctx = ExtensionSqlContext::new(ext_public_key.clone(), ext_name.clone());

        for stmt in statements.iter() {
            // Skip PRAGMA validation (handled separately during execution)
            // but still verify allowed PRAGMAs here for early rejection
            if is_pragma_statement(stmt) {
                if !is_allowed_pragma(stmt) {
                    return Err(ExtensionError::ValidationError {
                        reason: format!(
                            "PRAGMA statement not allowed: '{}'. Only 'PRAGMA foreign_keys=OFF/ON' is permitted for migrations.",
                            stmt.chars().take(50).collect::<String>()
                        ),
                    });
                }
                continue; // Skip table prefix validation for allowed PRAGMAs
            }
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
        // Signal extension ready even if no migrations to apply
        // This is crucial for ExternalBridge to know the extension is ready
        #[cfg(not(any(target_os = "android", target_os = "ios")))]
        {
            let bridge = state.external_bridge.lock().await;
            bridge.signal_extension_ready(&extension_id).await;
            eprintln!(
                "[ExtensionDatabase] Extension {} signaled ready (no pending migrations)",
                extension_id
            );
        }

        return Ok(MigrationResult {
            applied_count: 0,
            already_applied_count,
            applied_migrations: vec![],
        });
    }

    // Apply pending migrations
    let mut applied_names: Vec<String> = Vec::new();
    let exec_ctx = ExtensionSqlContext::new(ext_public_key.clone(), ext_name.clone());

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

    // Signal that the extension is ready after successful migration registration
    // This is for native webview mode - iframe mode signals from the frontend
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        let bridge = state.external_bridge.lock().await;
        bridge.signal_extension_ready(&extension_id).await;
        eprintln!(
            "[ExtensionDatabase] Extension {} signaled ready after migrations",
            extension_id
        );
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
                "SELECT COUNT(*) FROM haex_extensions WHERE haex_tombstone = 0 OR haex_tombstone IS NULL",
                [],
                |row| row.get(0),
            )
            .unwrap_or(-1);
        let mig_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM haex_extension_migrations WHERE haex_tombstone = 0 OR haex_tombstone IS NULL",
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
        let ctx = ExtensionSqlContext::new(public_key.clone(), ext_name.clone());
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
