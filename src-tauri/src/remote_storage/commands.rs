// src-tauri/src/remote_storage/commands.rs
//!
//! Remote Storage Tauri Commands
//!

use super::backend::create_backend;
use super::error::StorageError;
use super::queries::{
    SQL_DELETE_BACKEND, SQL_GET_BACKEND_CONFIG, SQL_INSERT_BACKEND, SQL_LIST_BACKENDS,
};
use super::types::{
    AddStorageBackendRequest, StorageBackendInfo, StorageDeleteRequest, StorageDownloadRequest,
    StorageListRequest, StorageObjectInfo, StorageUploadRequest, UpdateStorageBackendRequest,
};
use crate::database::core;
use crate::database::row::{get_bool, get_string};
use crate::AppState;
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use serde_json::Value as JsonValue;
use tauri::State;

// ============================================================================
// Backend Management Commands
// ============================================================================

/// List all remote storage backends
#[tauri::command]
pub async fn remote_storage_list_backends(
    state: State<'_, AppState>,
) -> Result<Vec<StorageBackendInfo>, StorageError> {
    let rows =
        core::select_with_crdt(SQL_LIST_BACKENDS.clone(), vec![], &state.db).map_err(|e| {
            StorageError::DatabaseError {
                reason: e.to_string(),
            }
        })?;

    let backends = rows
        .iter()
        .map(|row| {
            let config_str = get_string(row, 5);
            let public_config = parse_public_config(&config_str);

            StorageBackendInfo {
                id: get_string(row, 0),
                r#type: get_string(row, 1),
                name: get_string(row, 2),
                enabled: get_bool(row, 3),
                created_at: get_string(row, 4),
                config: public_config,
            }
        })
        .collect();

    Ok(backends)
}

/// Parse config JSON and extract public fields (without secrets)
fn parse_public_config(config_str: &str) -> Option<super::types::S3PublicConfig> {
    if config_str.is_empty() {
        return None;
    }

    let config: serde_json::Value = serde_json::from_str(config_str).ok()?;

    Some(super::types::S3PublicConfig {
        endpoint: config
            .get("endpoint")
            .and_then(|v| v.as_str())
            .map(String::from),
        region: config
            .get("region")
            .and_then(|v| v.as_str())
            .unwrap_or("auto")
            .to_string(),
        bucket: config
            .get("bucket")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        path_style: config.get("pathStyle").and_then(|v| v.as_bool()),
    })
}

/// Add a new remote storage backend
#[tauri::command]
pub async fn remote_storage_add_backend(
    state: State<'_, AppState>,
    request: AddStorageBackendRequest,
) -> Result<StorageBackendInfo, StorageError> {
    // Validate the config by trying to create a backend
    let _backend = create_backend(&request.r#type, &request.config).await?;

    let id = uuid::Uuid::new_v4().to_string();
    let config_json =
        serde_json::to_string(&request.config).map_err(|e| StorageError::InvalidConfig {
            reason: format!("Failed to serialize config: {}", e),
        })?;

    let hlc_service = state.hlc.lock().map_err(|_| StorageError::Internal {
        reason: "Failed to lock HLC service".to_string(),
    })?;

    let rows = core::execute_with_crdt(
        SQL_INSERT_BACKEND.clone(),
        vec![
            JsonValue::String(id.clone()),
            JsonValue::String(request.r#type.clone()),
            JsonValue::String(request.name.clone()),
            JsonValue::String(config_json),
        ],
        &state.db,
        &hlc_service,
    )
    .map_err(|e| StorageError::DatabaseError {
        reason: e.to_string(),
    })?;

    if rows.is_empty() {
        return Err(StorageError::Internal {
            reason: "Insert returned no rows".to_string(),
        });
    }

    let row = &rows[0];

    // Extract public config from the request
    let public_config = Some(super::types::S3PublicConfig {
        endpoint: request
            .config
            .get("endpoint")
            .and_then(|v| v.as_str())
            .map(String::from),
        region: request
            .config
            .get("region")
            .and_then(|v| v.as_str())
            .unwrap_or("auto")
            .to_string(),
        bucket: request
            .config
            .get("bucket")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        path_style: request.config.get("pathStyle").and_then(|v| v.as_bool()),
    });

    Ok(StorageBackendInfo {
        id: get_string(row, 0),
        r#type: get_string(row, 1),
        name: get_string(row, 2),
        enabled: get_bool(row, 3),
        created_at: get_string(row, 4),
        config: public_config,
    })
}

/// Remove a remote storage backend
#[tauri::command]
pub async fn remote_storage_remove_backend(
    state: State<'_, AppState>,
    backend_id: String,
) -> Result<(), StorageError> {
    let hlc_service = state.hlc.lock().map_err(|_| StorageError::Internal {
        reason: "Failed to lock HLC service".to_string(),
    })?;

    core::execute_with_crdt(
        SQL_DELETE_BACKEND.clone(),
        vec![JsonValue::String(backend_id)],
        &state.db,
        &hlc_service,
    )
    .map_err(|e| StorageError::DatabaseError {
        reason: e.to_string(),
    })?;

    Ok(())
}

/// Test a remote storage backend connection
#[tauri::command]
pub async fn remote_storage_test_backend(
    state: State<'_, AppState>,
    backend_id: String,
) -> Result<(), StorageError> {
    let backend = get_backend_instance(&state, &backend_id).await?;
    backend.test_connection().await
}

/// Update a remote storage backend
/// If credentials are not provided in the update, existing credentials are preserved
#[tauri::command]
pub async fn remote_storage_update_backend(
    state: State<'_, AppState>,
    request: UpdateStorageBackendRequest,
) -> Result<StorageBackendInfo, StorageError> {
    // Get existing backend config first
    let existing_rows = core::select_with_crdt(
        SQL_GET_BACKEND_CONFIG.clone(),
        vec![JsonValue::String(request.backend_id.clone())],
        &state.db,
    )
    .map_err(|e| StorageError::DatabaseError {
        reason: e.to_string(),
    })?;

    if existing_rows.is_empty() {
        return Err(StorageError::BackendNotFound {
            id: request.backend_id.clone(),
        });
    }

    let backend_type = get_string(&existing_rows[0], 0);
    let existing_config_str = get_string(&existing_rows[0], 1);

    // Merge: use new config but preserve credentials from existing if not provided
    let merged_config = if let Some(ref new_config) = request.config {
        let existing_config: serde_json::Value =
            serde_json::from_str(&existing_config_str).unwrap_or(serde_json::json!({}));

        let mut merged = new_config.clone();
        if let Some(merged_obj) = merged.as_object_mut() {
            if let Some(existing_obj) = existing_config.as_object() {
                // Only preserve credentials if not provided in new config
                if !merged_obj.contains_key("accessKeyId") {
                    if let Some(val) = existing_obj.get("accessKeyId") {
                        merged_obj.insert("accessKeyId".to_string(), val.clone());
                    }
                }
                if !merged_obj.contains_key("secretAccessKey") {
                    if let Some(val) = existing_obj.get("secretAccessKey") {
                        merged_obj.insert("secretAccessKey".to_string(), val.clone());
                    }
                }
            }
        }
        Some(merged)
    } else {
        None
    };

    // Validate the merged config by trying to create a backend
    if let Some(ref config) = merged_config {
        let _backend = create_backend(&backend_type, config).await?;
    }

    // Build update query dynamically based on what's provided
    let (query, params) = build_update_query(&request, merged_config)?;

    let hlc_service = state.hlc.lock().map_err(|_| StorageError::Internal {
        reason: "Failed to lock HLC service".to_string(),
    })?;

    let rows = core::execute_with_crdt(query, params, &state.db, &hlc_service).map_err(|e| {
        StorageError::DatabaseError {
            reason: e.to_string(),
        }
    })?;

    if rows.is_empty() {
        return Err(StorageError::BackendNotFound {
            id: request.backend_id,
        });
    }

    let row = &rows[0];
    let config_str = get_string(row, 5);
    let public_config = parse_public_config(&config_str);

    Ok(StorageBackendInfo {
        id: get_string(row, 0),
        r#type: get_string(row, 1),
        name: get_string(row, 2),
        enabled: get_bool(row, 3),
        created_at: get_string(row, 4),
        config: public_config,
    })
}

/// Build UPDATE query based on which fields are provided
fn build_update_query(
    request: &UpdateStorageBackendRequest,
    merged_config: Option<serde_json::Value>,
) -> Result<(String, Vec<JsonValue>), StorageError> {
    use crate::table_names::*;

    let mut set_clauses = Vec::new();
    let mut params: Vec<JsonValue> = vec![JsonValue::String(request.backend_id.clone())];
    let mut param_index = 2;

    if let Some(ref name) = request.name {
        set_clauses.push(format!("{} = ?{}", COL_STORAGE_BACKENDS_NAME, param_index));
        params.push(JsonValue::String(name.clone()));
        param_index += 1;
    }

    // Use merged config (with preserved credentials) instead of request.config
    if let Some(ref config) = merged_config {
        let config_json =
            serde_json::to_string(config).map_err(|e| StorageError::InvalidConfig {
                reason: format!("Failed to serialize config: {}", e),
            })?;
        set_clauses.push(format!(
            "{} = ?{}",
            COL_STORAGE_BACKENDS_CONFIG, param_index
        ));
        params.push(JsonValue::String(config_json));
    }

    if set_clauses.is_empty() {
        return Err(StorageError::InvalidConfig {
            reason: "No fields to update".to_string(),
        });
    }

    let query = format!(
        "UPDATE {} SET {} WHERE {} = ?1 \
         RETURNING {}, {}, {}, {}, {}, {}",
        TABLE_STORAGE_BACKENDS,
        set_clauses.join(", "),
        COL_STORAGE_BACKENDS_ID,
        COL_STORAGE_BACKENDS_ID,
        COL_STORAGE_BACKENDS_TYPE,
        COL_STORAGE_BACKENDS_NAME,
        COL_STORAGE_BACKENDS_ENABLED,
        COL_STORAGE_BACKENDS_CREATED_AT,
        COL_STORAGE_BACKENDS_CONFIG
    );

    Ok((query, params))
}

// ============================================================================
// Remote Storage Operations Commands
// ============================================================================

/// Upload data to a remote storage backend
#[tauri::command]
pub async fn remote_storage_upload(
    state: State<'_, AppState>,
    request: StorageUploadRequest,
) -> Result<(), StorageError> {
    let backend = get_backend_instance(&state, &request.backend_id).await?;

    let data = BASE64
        .decode(&request.data)
        .map_err(|e| StorageError::InvalidConfig {
            reason: format!("Invalid base64 data: {}", e),
        })?;

    backend.upload(&request.key, &data).await
}

/// Download data from a remote storage backend
#[tauri::command]
pub async fn remote_storage_download(
    state: State<'_, AppState>,
    request: StorageDownloadRequest,
) -> Result<String, StorageError> {
    let backend = get_backend_instance(&state, &request.backend_id).await?;

    let data = backend.download(&request.key).await?;
    Ok(BASE64.encode(&data))
}

/// Delete an object from a remote storage backend
#[tauri::command]
pub async fn remote_storage_delete(
    state: State<'_, AppState>,
    request: StorageDeleteRequest,
) -> Result<(), StorageError> {
    let backend = get_backend_instance(&state, &request.backend_id).await?;
    backend.delete(&request.key).await
}

/// List objects in a remote storage backend
#[tauri::command]
pub async fn remote_storage_list(
    state: State<'_, AppState>,
    request: StorageListRequest,
) -> Result<Vec<StorageObjectInfo>, StorageError> {
    let backend = get_backend_instance(&state, &request.backend_id).await?;
    backend.list(request.prefix.as_deref()).await
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Get a backend instance by ID
async fn get_backend_instance(
    state: &State<'_, AppState>,
    backend_id: &str,
) -> Result<Box<dyn super::backend::StorageBackend>, StorageError> {
    let rows = core::select_with_crdt(
        SQL_GET_BACKEND_CONFIG.clone(),
        vec![JsonValue::String(backend_id.to_string())],
        &state.db,
    )
    .map_err(|e| StorageError::DatabaseError {
        reason: e.to_string(),
    })?;

    if rows.is_empty() {
        return Err(StorageError::BackendNotFound {
            id: backend_id.to_string(),
        });
    }

    let row = &rows[0];
    let backend_type = get_string(row, 0);
    if backend_type.is_empty() {
        return Err(StorageError::Internal {
            reason: "Missing backend type".to_string(),
        });
    }

    let config_str = get_string(row, 1);
    if config_str.is_empty() {
        return Err(StorageError::Internal {
            reason: "Missing backend config".to_string(),
        });
    }

    let config: serde_json::Value =
        serde_json::from_str(&config_str).map_err(|e| StorageError::InvalidConfig {
            reason: format!("Failed to parse config: {}", e),
        })?;

    create_backend(&backend_type, &config).await
}
