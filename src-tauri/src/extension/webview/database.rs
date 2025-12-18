use super::helpers::{emit_permission_prompt_if_needed, get_extension_id};
use crate::extension::database::{
    extension_sql_execute, extension_sql_select, register_extension_migrations, MigrationResult,
};
use crate::extension::error::ExtensionError;
use crate::AppState;
use tauri::{AppHandle, State, WebviewWindow};

#[tauri::command]
pub async fn webview_extension_db_query(
    window: WebviewWindow,
    state: State<'_, AppState>,
    app_handle: AppHandle,
    query: String,
    params: Vec<serde_json::Value>,
) -> Result<serde_json::Value, ExtensionError> {
    let extension_id = get_extension_id(&window, &state)?;

    // Get extension to retrieve public_key and name for existing database functions
    let extension = state
        .extension_manager
        .get_extension(&extension_id)
        .ok_or_else(|| ExtensionError::ValidationError {
            reason: format!("Extension with ID {} not found", extension_id),
        })?;

    let result = extension_sql_select(
        &query,
        params,
        extension.manifest.public_key.clone(),
        extension.manifest.name.clone(),
        state,
    )
    .await;

    // Emit permission prompt event if needed
    if let Err(ref e) = result {
        emit_permission_prompt_if_needed(&app_handle, e);
    }

    let rows = result.map_err(|e| ExtensionError::ValidationError {
        reason: format!("Database query failed: {}", e),
    })?;

    Ok(serde_json::json!({
        "rows": rows,
        "rowsAffected": 0,
        "lastInsertId": null
    }))
}

#[tauri::command]
pub async fn webview_extension_db_execute(
    window: WebviewWindow,
    state: State<'_, AppState>,
    app_handle: AppHandle,
    query: String,
    params: Vec<serde_json::Value>,
) -> Result<serde_json::Value, ExtensionError> {
    let extension_id = get_extension_id(&window, &state)?;

    // Get extension to retrieve public_key and name for existing database functions
    let extension = state
        .extension_manager
        .get_extension(&extension_id)
        .ok_or_else(|| ExtensionError::ValidationError {
            reason: format!("Extension with ID {} not found", extension_id),
        })?;

    let result = extension_sql_execute(
        &query,
        params,
        extension.manifest.public_key.clone(),
        extension.manifest.name.clone(),
        state,
    )
    .await;

    // Emit permission prompt event if needed
    if let Err(ref e) = result {
        emit_permission_prompt_if_needed(&app_handle, e);
    }

    let rows = result.map_err(|e| ExtensionError::ValidationError {
        reason: format!("Database execute failed: {}", e),
    })?;

    Ok(serde_json::json!({
        "rows": rows,
        "rowsAffected": rows.len(),
        "lastInsertId": null
    }))
}

#[tauri::command]
pub async fn webview_extension_db_register_migrations(
    window: WebviewWindow,
    state: State<'_, AppState>,
    app_handle: AppHandle,
    extension_version: String,
    migrations: Vec<serde_json::Map<String, serde_json::Value>>,
) -> Result<MigrationResult, ExtensionError> {
    let extension_id = get_extension_id(&window, &state)?;

    // Get extension to retrieve public_key and name
    let extension = state
        .extension_manager
        .get_extension(&extension_id)
        .ok_or_else(|| ExtensionError::ValidationError {
            reason: format!("Extension with ID {} not found", extension_id),
        })?;

    let result = register_extension_migrations(
        extension.manifest.public_key.clone(),
        extension.manifest.name.clone(),
        extension_version,
        migrations,
        state,
    )
    .await;

    // Emit permission prompt event if needed
    if let Err(ref e) = result {
        emit_permission_prompt_if_needed(&app_handle, e);
    }

    result
}
