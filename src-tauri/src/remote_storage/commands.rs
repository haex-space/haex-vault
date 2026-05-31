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
    StorageListDirResponse, StorageListRequest, StorageObjectInfo, StorageUploadRequest,
    UpdateStorageBackendRequest,
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
    // Validate the config and verify the backend is actually reachable
    // before persisting — surfaces credential/region/endpoint problems
    // immediately instead of failing later inside sync rules.
    let backend = create_backend(&request.r#type, &request.config).await?;
    backend.test_connection().await?;

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

    // Validate the merged config and verify the backend is reachable
    // before persisting changes.
    if let Some(ref config) = merged_config {
        let backend = create_backend(&backend_type, config).await?;
        backend.test_connection().await?;
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

/// List a single hierarchy level (folders + objects) in a remote storage
/// backend. Used by the file browser to navigate buckets without loading the
/// entire key space.
#[tauri::command]
pub async fn remote_storage_list_dir(
    state: State<'_, AppState>,
    request: StorageListRequest,
) -> Result<StorageListDirResponse, StorageError> {
    let backend = get_backend_instance(&state, &request.backend_id).await?;
    backend.list_dir(request.prefix.as_deref()).await
}

// ============================================================================
// Resumable Download
// ============================================================================

/// Download an object directly to a local path, resuming any existing
/// partial file at that path.
///
/// Why this exists separately from `remote_storage_download`:
///   - The base64 round-trip used by `remote_storage_download` loads the
///     whole file into RAM (and shovels it through IPC) — fine for small
///     blobs, unusable for video / multi-GiB objects.
///   - The `<a download>` shim that callers used to invoke after the
///     base64 trip doesn't reliably trigger a real save in Tauri's WebView
///     (WebKitGTK drops the `download` attribute), so the data was being
///     thrown away anyway.
///
/// Progress reports + cancellation flow through `AppState.transfer_tokens`
/// (same pool the P2P transfer commands use). Frontend listens for the
/// `storage:transfer:progress` / `storage:transfer:complete` /
/// `storage:transfer:cancelled` events keyed by `transfer_id`.
///
/// `transfer_id` is supplied by the caller so the UI can pair the lifecycle
/// events to the row that requested the download (and so the user can
/// cancel via `remote_storage_cancel_transfer`).
#[tauri::command]
pub async fn remote_storage_download_to_path(
    state: State<'_, AppState>,
    app_handle: tauri::AppHandle,
    request: super::types::DownloadToPathRequest,
) -> Result<u64, StorageError> {
    use std::path::PathBuf;
    use std::sync::atomic::AtomicBool;
    use std::sync::Arc;
    use tauri::Emitter;
    use tokio_util::sync::CancellationToken;

    let backend = get_backend_instance(&state, &request.backend_id).await?;
    let output = PathBuf::from(&request.output_path);

    // Register a cancellation token under the transfer id. The pause flag
    // is unused for now (cancel-on-resume is functionally equivalent) but
    // lives alongside the cancel token so the `transfer_tokens` map keeps
    // a single uniform shape with the P2P side.
    //
    // Reject duplicates: silently overwriting would orphan the previous
    // token (cancel requests would only reach the new transfer).
    let cancel = CancellationToken::new();
    let pause = Arc::new(AtomicBool::new(false));
    {
        let mut tokens = state.transfer_tokens.lock().await;
        if tokens.contains_key(&request.transfer_id) {
            return Err(StorageError::DownloadFailed {
                reason: format!("transferId {} already in flight", request.transfer_id),
            });
        }
        tokens.insert(request.transfer_id.clone(), (cancel.clone(), pause));
    }

    // Progress callback emits a Tauri event each throttle window. Cloning
    // the AppHandle is cheap (Arc inside) so the closure can outlive this
    // function if rust-s3 holds onto the writer.
    let app_for_cb = app_handle.clone();
    let tid_for_cb = request.transfer_id.clone();
    let cb: super::progress::ProgressCallback = Arc::new(move |done, total| {
        let _ = app_for_cb.emit(
            "storage:transfer:progress",
            serde_json::json!({
                "transferId": tid_for_cb,
                "bytesDone": done,
                "bytesTotal": total,
            }),
        );
    });

    let result = tokio::select! {
        r = backend.download_to_path_resumable(&request.key, &output, Some(cb)) => r,
        _ = cancel.cancelled() => {
            Err(StorageError::DownloadFailed {
                reason: "cancelled".to_string(),
            })
        }
    };

    // Always drop the token before returning so a follow-up resume doesn't
    // collide with a stale entry.
    state.transfer_tokens.lock().await.remove(&request.transfer_id);

    match result {
        Ok(bytes) => {
            let _ = app_handle.emit(
                "storage:transfer:complete",
                serde_json::json!({
                    "transferId": request.transfer_id,
                    "bytesDone": bytes,
                }),
            );
            Ok(bytes)
        }
        Err(err) => {
            let is_cancelled = matches!(
                &err,
                StorageError::DownloadFailed { reason } if reason == "cancelled"
            );
            let event = if is_cancelled {
                "storage:transfer:cancelled"
            } else {
                "storage:transfer:failed"
            };
            let _ = app_handle.emit(
                event,
                serde_json::json!({
                    "transferId": request.transfer_id,
                    "reason": err.to_string(),
                }),
            );
            Err(err)
        }
    }
}

/// Cancel an in-flight resumable download. Idempotent — calling on an
/// unknown id is a no-op (the transfer may have just finished). After a
/// cancel the partial file on disk is left intact so the caller can
/// resume by invoking `remote_storage_download_to_path` again with the
/// same output path.
#[tauri::command]
pub async fn remote_storage_cancel_transfer(
    state: State<'_, AppState>,
    transfer_id: String,
) -> Result<(), StorageError> {
    if let Some((cancel, _pause)) = state.transfer_tokens.lock().await.get(&transfer_id) {
        cancel.cancel();
    }
    Ok(())
}

// ============================================================================
// Resumable Upload
// ============================================================================

/// Stream a local file up to a remote storage backend, with progress events
/// and cancellation. Mirrors [`remote_storage_download_to_path`] in shape so
/// the frontend reuses the existing `storage:transfer:*` listeners and the
/// shared `remote_storage_cancel_transfer` command.
///
/// Unlike the download counterpart, cancellation flows **into** the backend
/// via a [`CancellationToken`] (not a `tokio::select!` race) so the S3 impl
/// can issue `AbortMultipartUpload` and avoid orphaning in-flight chunks the
/// bucket would otherwise be billed for. See
/// [`StorageBackend::upload_from_path_cancellable`].
#[tauri::command]
pub async fn remote_storage_upload_from_path(
    state: State<'_, AppState>,
    app_handle: tauri::AppHandle,
    request: super::types::UploadFromPathRequest,
) -> Result<u64, StorageError> {
    use std::path::PathBuf;
    use std::sync::atomic::AtomicBool;
    use std::sync::Arc;
    use tauri::Emitter;
    use tokio_util::sync::CancellationToken;

    let backend = get_backend_instance(&state, &request.backend_id).await?;
    let source = PathBuf::from(&request.source_path);

    // Reject duplicate transfer ids. The token map is keyed by caller-supplied
    // UUID; silently overwriting would orphan the previous token (cancel
    // requests then only reach the new transfer).
    let cancel = CancellationToken::new();
    let pause = Arc::new(AtomicBool::new(false));
    {
        let mut tokens = state.transfer_tokens.lock().await;
        if tokens.contains_key(&request.transfer_id) {
            return Err(StorageError::UploadFailed {
                reason: format!("transferId {} already in flight", request.transfer_id),
            });
        }
        tokens.insert(request.transfer_id.clone(), (cancel.clone(), pause));
    }

    let app_for_cb = app_handle.clone();
    let tid_for_cb = request.transfer_id.clone();
    let cb: super::progress::ProgressCallback = Arc::new(move |done, total| {
        let _ = app_for_cb.emit(
            "storage:transfer:progress",
            serde_json::json!({
                "transferId": tid_for_cb,
                "bytesDone": done,
                "bytesTotal": total,
            }),
        );
    });

    let result = backend
        .upload_from_path_cancellable(&request.key, &source, Some(cb), Some(cancel))
        .await;

    state.transfer_tokens.lock().await.remove(&request.transfer_id);

    match result {
        Ok(bytes) => {
            let _ = app_handle.emit(
                "storage:transfer:complete",
                serde_json::json!({
                    "transferId": request.transfer_id,
                    "bytesDone": bytes,
                }),
            );
            Ok(bytes)
        }
        Err(err) => {
            let is_cancelled = matches!(
                &err,
                StorageError::UploadFailed { reason } if reason == "cancelled"
            );
            let event = if is_cancelled {
                "storage:transfer:cancelled"
            } else {
                "storage:transfer:failed"
            };
            let _ = app_handle.emit(
                event,
                serde_json::json!({
                    "transferId": request.transfer_id,
                    "reason": err.to_string(),
                }),
            );
            Err(err)
        }
    }
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Get a backend instance by ID, using a `DbConnection` directly.
///
/// This is the shared implementation used by both the Tauri command helper
/// and the file-sync provider factory.
pub async fn get_backend_instance_from_db(
    db: &crate::database::DbConnection,
    backend_id: &str,
) -> Result<Box<dyn super::backend::StorageBackend>, StorageError> {
    get_backend_instance_from_db_with_overrides(db, backend_id, None).await
}

/// Like `get_backend_instance_from_db`, but allows a per-rule override of the
/// bucket name without persisting the change to the backend's stored config.
/// Used by file-sync rules that want to point at a different bucket than the
/// backend's default while sharing credentials/endpoint/region.
pub async fn get_backend_instance_from_db_with_overrides(
    db: &crate::database::DbConnection,
    backend_id: &str,
    bucket_override: Option<&str>,
) -> Result<Box<dyn super::backend::StorageBackend>, StorageError> {
    let rows = core::select_with_crdt(
        SQL_GET_BACKEND_CONFIG.clone(),
        vec![JsonValue::String(backend_id.to_string())],
        db,
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

    let mut config: serde_json::Value =
        serde_json::from_str(&config_str).map_err(|e| StorageError::InvalidConfig {
            reason: format!("Failed to parse config: {}", e),
        })?;

    if let Some(bucket) = bucket_override {
        if !bucket.is_empty() {
            if let Some(obj) = config.as_object_mut() {
                obj.insert("bucket".to_string(), JsonValue::String(bucket.to_string()));
            }
        }
    }

    create_backend(&backend_type, &config).await
}

/// Get a backend instance by ID (from Tauri State)
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
