// src-tauri/src/database/mod.rs

pub mod connection_context;
pub mod constants;
pub mod core;
pub mod error;
pub mod generated;
pub mod init;
pub mod migrations;
pub mod row;
pub mod stats;
pub mod vault_lock;

mod create;
mod identity_default;
mod import_delete;
mod listing;
mod maintenance;
mod open;
mod paths;

pub use create::*;
pub use import_delete::*;
pub use listing::*;
pub use maintenance::*;
pub use open::*;
pub use paths::*;

use crate::database::error::DatabaseError;
use crate::AppState;
use rusqlite::Connection;
use serde_json::Value as JsonValue;
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, State};

pub struct DbConnection(pub Arc<Mutex<Option<Connection>>>);

#[tauri::command]
pub fn sql_select(
    sql: String,
    params: Vec<JsonValue>,
    state: State<'_, AppState>,
) -> Result<Vec<Vec<JsonValue>>, DatabaseError> {
    core::select(sql, params, &state.db)
}

#[tauri::command]
pub fn sql_execute(
    sql: String,
    params: Vec<JsonValue>,
    state: State<'_, AppState>,
) -> Result<Vec<Vec<JsonValue>>, DatabaseError> {
    core::execute(sql, params, &state.db)
}

#[tauri::command]
pub fn sql_select_with_crdt(
    sql: String,
    params: Vec<JsonValue>,
    state: State<'_, AppState>,
) -> Result<Vec<Vec<JsonValue>>, DatabaseError> {
    core::select_with_crdt(sql, params, &state.db)
}

#[tauri::command]
pub fn sql_execute_with_crdt(
    sql: String,
    params: Vec<JsonValue>,
    app_handle: AppHandle,
    state: State<'_, AppState>,
) -> Result<Vec<Vec<JsonValue>>, DatabaseError> {
    let hlc_service = state.lock_or_fail(
        &state.hlc,
        crate::critical::CriticalFailureCode::HlcMutexPoisoned,
        "database::sql_execute_with_crdt",
        serde_json::json!({}),
    )?;
    let result = core::execute_with_crdt(sql, params, &state.db, &hlc_service)?;

    // Emit event to notify frontend that dirty tables may have changed
    crate::crdt::notify_dirty_tables_changed(&app_handle);

    Ok(result)
}

/// Unified SQL command with CRDT support
///
/// This command automatically detects the SQL statement type using AST parsing:
/// - SELECT: Executes with tombstone filtering (select_with_crdt)
/// - INSERT/UPDATE/DELETE: Executes with CRDT timestamps (execute_with_crdt)
///   - If RETURNING clause is present, returns the result rows
///   - Otherwise returns empty array
///
/// This replaces the need for separate sql_select_with_crdt and
/// sql_execute_with_crdt commands in the frontend.
#[tauri::command]
pub fn sql_with_crdt(
    sql: String,
    params: Vec<JsonValue>,
    app_handle: AppHandle,
    state: State<'_, AppState>,
) -> Result<Vec<Vec<JsonValue>>, DatabaseError> {
    use sqlparser::ast::Statement;

    // Parse the SQL statement using AST (no string matching!)
    let statement = core::parse_single_statement(&sql)?;

    match statement {
        // SELECT statements: use select_with_crdt (adds tombstone filter)
        Statement::Query(_) => core::select_with_crdt(sql, params, &state.db),
        // INSERT/UPDATE/DELETE: use execute_with_crdt (handles RETURNING via AST)
        Statement::Insert(_) | Statement::Update { .. } | Statement::Delete(_) => {
            let hlc_service = state.lock_or_fail(
                &state.hlc,
                crate::critical::CriticalFailureCode::HlcMutexPoisoned,
                "database::sql_with_crdt",
                serde_json::json!({}),
            )?;

            let result = core::execute_with_crdt(sql, params, &state.db, &hlc_service)?;

            // Emit event to notify frontend that dirty tables may have changed
            crate::crdt::notify_dirty_tables_changed(&app_handle);

            Ok(result)
        }
        // Other statements (CREATE TABLE, etc.) - execute without CRDT
        _ => core::execute(sql, params, &state.db),
    }
}
