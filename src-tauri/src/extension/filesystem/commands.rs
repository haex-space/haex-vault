// src-tauri/src/extension/filesystem/commands.rs
//!
//! Tauri Commands for FileSync
//!
//! These commands are called from extensions via the SDK's FileSyncAPI.
//!

use crate::database::core;
use crate::database::error::DatabaseError;
use crate::extension::filesystem::error::FileSyncError;
use crate::extension::filesystem::types::*;
use crate::table_names::{
    TABLE_FILE_BACKENDS, TABLE_FILE_SPACES, TABLE_FILE_SYNC_RULES, TABLE_FILE_SYNC_RULE_BACKENDS,
};
use crate::AppState;
use serde_json::Value as JsonValue;
use tauri::{AppHandle, Emitter, State};

// Helper to convert DatabaseError to FileSyncError
impl From<DatabaseError> for FileSyncError {
    fn from(e: DatabaseError) -> Self {
        FileSyncError::DatabaseError {
            reason: e.to_string(),
        }
    }
}

// Helper to parse row data from Vec<JsonValue>
fn get_string(row: &[JsonValue], idx: usize) -> String {
    row.get(idx)
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string()
}

fn get_bool(row: &[JsonValue], idx: usize) -> bool {
    row.get(idx)
        .and_then(|v| v.as_i64())
        .map(|v| v != 0)
        .unwrap_or(false)
}

fn get_u64(row: &[JsonValue], idx: usize) -> u64 {
    row.get(idx).and_then(|v| v.as_i64()).unwrap_or(0) as u64
}

// ============================================================================
// Spaces Commands
// ============================================================================

/// List all file spaces
#[tauri::command]
pub async fn filesync_list_spaces(
    state: State<'_, AppState>,
) -> Result<Vec<FileSpace>, FileSyncError> {
    let sql = format!(
        "SELECT id, name, is_personal, file_count, total_size, created_at, updated_at
         FROM {TABLE_FILE_SPACES}
         ORDER BY name"
    );

    let rows = core::select_with_crdt(sql, vec![], &state.db)?;

    let spaces = rows
        .into_iter()
        .map(|row| FileSpace {
            id: get_string(&row, 0),
            name: get_string(&row, 1),
            is_personal: get_bool(&row, 2),
            file_count: get_u64(&row, 3),
            total_size: get_u64(&row, 4),
            created_at: get_string(&row, 5),
            updated_at: get_string(&row, 6),
        })
        .collect();

    Ok(spaces)
}

/// Create a new file space
#[tauri::command]
pub async fn filesync_create_space(
    app_handle: AppHandle,
    state: State<'_, AppState>,
    request: CreateSpaceRequest,
) -> Result<FileSpace, FileSyncError> {
    let id = uuid::Uuid::new_v4().to_string();
    // TODO: Generate and wrap space_key with vault_key
    let wrapped_key = "TODO_IMPLEMENT_KEY_WRAPPING".to_string();

    let sql = format!(
        "INSERT INTO {TABLE_FILE_SPACES} (id, name, wrapped_key, is_personal, file_count, total_size)
         VALUES (?, ?, ?, 1, 0, 0)
         RETURNING id, name, is_personal, file_count, total_size, created_at, updated_at"
    );

    let hlc_service = state.hlc.lock().map_err(|_| FileSyncError::Internal {
        reason: "Failed to lock HLC service".to_string(),
    })?;

    let rows = core::execute_with_crdt(
        sql,
        vec![
            JsonValue::String(id),
            JsonValue::String(request.name),
            JsonValue::String(wrapped_key),
        ],
        &state.db,
        &hlc_service,
    )?;

    let _ = app_handle.emit("crdt:dirty-tables-changed", ());

    let row = rows.first().ok_or(FileSyncError::Internal {
        reason: "INSERT did not return created row".to_string(),
    })?;

    Ok(FileSpace {
        id: get_string(row, 0),
        name: get_string(row, 1),
        is_personal: get_bool(row, 2),
        file_count: get_u64(row, 3),
        total_size: get_u64(row, 4),
        created_at: get_string(row, 5),
        updated_at: get_string(row, 6),
    })
}

/// Delete a file space
#[tauri::command]
pub async fn filesync_delete_space(
    app_handle: AppHandle,
    state: State<'_, AppState>,
    space_id: String,
) -> Result<(), FileSyncError> {
    // Soft delete via CRDT (sets haex_tombstone = 1)
    let sql = format!("DELETE FROM {TABLE_FILE_SPACES} WHERE id = ?");

    let hlc_service = state.hlc.lock().map_err(|_| FileSyncError::Internal {
        reason: "Failed to lock HLC service".to_string(),
    })?;

    core::execute_with_crdt(
        sql,
        vec![JsonValue::String(space_id)],
        &state.db,
        &hlc_service,
    )?;

    let _ = app_handle.emit("crdt:dirty-tables-changed", ());

    Ok(())
}

// ============================================================================
// Files Commands
// ============================================================================

/// List files in a space
#[tauri::command]
pub async fn filesync_list_files(
    _state: State<'_, AppState>,
    request: ListFilesRequest,
) -> Result<Vec<FileInfo>, FileSyncError> {
    // TODO: Query files from database with filtering
    let _request = request;
    Ok(vec![])
}

/// Get file info by ID
#[tauri::command]
pub async fn filesync_get_file(
    _state: State<'_, AppState>,
    file_id: String,
) -> Result<FileInfo, FileSyncError> {
    // TODO: Query single file from database
    Err(FileSyncError::FileNotFound { id: file_id })
}

/// Upload a file to the space
#[tauri::command]
pub async fn filesync_upload_file(
    _state: State<'_, AppState>,
    request: UploadFileRequest,
) -> Result<FileInfo, FileSyncError> {
    // TODO: Implement file upload flow:
    // 1. Read file from local_path
    // 2. Generate file_key
    // 3. Encrypt file with FileEncryption
    // 4. Upload to backend(s)
    // 5. Store metadata in database
    let _request = request;
    Err(FileSyncError::NotInitialized)
}

/// Download a file from the space
#[tauri::command]
pub async fn filesync_download_file(
    _state: State<'_, AppState>,
    request: DownloadFileRequest,
) -> Result<(), FileSyncError> {
    // TODO: Implement file download flow:
    // 1. Get file metadata from database
    // 2. Download encrypted data from backend
    // 3. Decrypt with file_key
    // 4. Write to local_path
    let _request = request;
    Err(FileSyncError::NotInitialized)
}

/// Delete a file
#[tauri::command]
pub async fn filesync_delete_file(
    _state: State<'_, AppState>,
    file_id: String,
) -> Result<(), FileSyncError> {
    // TODO: Delete file from backends and database
    let _id = file_id;
    Err(FileSyncError::NotInitialized)
}

// ============================================================================
// Backends Commands
// ============================================================================

/// List all configured storage backends
#[tauri::command]
pub async fn filesync_list_backends(
    state: State<'_, AppState>,
) -> Result<Vec<StorageBackendInfo>, FileSyncError> {
    let sql = format!(
        "SELECT id, type, name, enabled, created_at
         FROM {TABLE_FILE_BACKENDS}
         ORDER BY name"
    );

    let rows = core::select_with_crdt(sql, vec![], &state.db)?;

    let backends = rows
        .into_iter()
        .filter_map(|row| {
            let type_str = get_string(&row, 1);
            let backend_type = parse_backend_type(&type_str)?;
            Some(StorageBackendInfo {
                id: get_string(&row, 0),
                backend_type,
                name: get_string(&row, 2),
                enabled: get_bool(&row, 3),
                created_at: get_string(&row, 4),
            })
        })
        .collect();

    Ok(backends)
}

fn parse_backend_type(s: &str) -> Option<StorageBackendType> {
    match s {
        "s3" => Some(StorageBackendType::S3),
        "r2" => Some(StorageBackendType::R2),
        "minio" => Some(StorageBackendType::Minio),
        "gdrive" => Some(StorageBackendType::GDrive),
        "dropbox" => Some(StorageBackendType::Dropbox),
        _ => None,
    }
}

/// Add a new storage backend
#[tauri::command]
pub async fn filesync_add_backend(
    app_handle: AppHandle,
    state: State<'_, AppState>,
    request: AddBackendRequest,
) -> Result<StorageBackendInfo, FileSyncError> {
    let id = uuid::Uuid::new_v4().to_string();

    // TODO: Encrypt config with vault_key before storing
    let encrypted_config = serde_json::to_string(&request.config)
        .map_err(|e| FileSyncError::Internal { reason: e.to_string() })?;

    let sql = format!(
        "INSERT INTO {TABLE_FILE_BACKENDS} (id, type, name, encrypted_config, enabled)
         VALUES (?, ?, ?, ?, 1)
         RETURNING id, type, name, enabled, created_at"
    );

    let hlc_service = state.hlc.lock().map_err(|_| FileSyncError::Internal {
        reason: "Failed to lock HLC service".to_string(),
    })?;

    let rows = core::execute_with_crdt(
        sql,
        vec![
            JsonValue::String(id),
            JsonValue::String(request.backend_type.to_string()),
            JsonValue::String(request.name),
            JsonValue::String(encrypted_config),
        ],
        &state.db,
        &hlc_service,
    )?;

    let _ = app_handle.emit("crdt:dirty-tables-changed", ());

    let row = rows.first().ok_or(FileSyncError::Internal {
        reason: "INSERT did not return created row".to_string(),
    })?;

    let type_str = get_string(row, 1);
    let backend_type = parse_backend_type(&type_str).ok_or(FileSyncError::Internal {
        reason: format!("Unknown backend type: {}", type_str),
    })?;

    Ok(StorageBackendInfo {
        id: get_string(row, 0),
        backend_type,
        name: get_string(row, 2),
        enabled: get_bool(row, 3),
        created_at: get_string(row, 4),
    })
}

/// Remove a storage backend
#[tauri::command]
pub async fn filesync_remove_backend(
    app_handle: AppHandle,
    state: State<'_, AppState>,
    backend_id: String,
) -> Result<(), FileSyncError> {
    let sql = format!("DELETE FROM {TABLE_FILE_BACKENDS} WHERE id = ?");

    let hlc_service = state.hlc.lock().map_err(|_| FileSyncError::Internal {
        reason: "Failed to lock HLC service".to_string(),
    })?;

    core::execute_with_crdt(
        sql,
        vec![JsonValue::String(backend_id)],
        &state.db,
        &hlc_service,
    )?;

    let _ = app_handle.emit("crdt:dirty-tables-changed", ());

    Ok(())
}

/// Test connection to a storage backend
#[tauri::command]
pub async fn filesync_test_backend(
    state: State<'_, AppState>,
    backend_id: String,
) -> Result<(), FileSyncError> {
    // Get backend config from database
    let sql = format!(
        "SELECT type, encrypted_config FROM {TABLE_FILE_BACKENDS} WHERE id = ?"
    );

    let rows = core::select_with_crdt(sql, vec![JsonValue::String(backend_id.clone())], &state.db)?;

    let row = rows.first().ok_or(FileSyncError::BackendNotFound {
        id: backend_id.clone(),
    })?;

    let backend_type = get_string(row, 0);
    let encrypted_config = get_string(row, 1);

    // TODO: Decrypt config with vault_key
    // For now, parse as plain JSON
    let _config: serde_json::Value = serde_json::from_str(&encrypted_config)
        .map_err(|e| FileSyncError::Internal { reason: e.to_string() })?;

    // Test connection based on backend type
    match backend_type.as_str() {
        "s3" => {
            // TODO: Create S3Backend and test connection
            // let s3_config = S3Config::from_json(&_config)?;
            // let backend = S3Backend::new(s3_config).await?;
            // backend.test_connection().await?;
            Ok(())
        }
        _ => Err(FileSyncError::Internal {
            reason: format!("Unknown backend type: {}", backend_type),
        }),
    }
}

// ============================================================================
// Sync Rules Commands
// ============================================================================

fn parse_sync_direction(s: &str) -> SyncDirection {
    match s {
        "up" => SyncDirection::Up,
        "down" => SyncDirection::Down,
        _ => SyncDirection::Both,
    }
}

/// List all sync rules
#[tauri::command]
pub async fn filesync_list_sync_rules(
    state: State<'_, AppState>,
) -> Result<Vec<SyncRule>, FileSyncError> {
    let sql = format!(
        "SELECT id, space_id, local_path, direction, enabled, created_at, updated_at
         FROM {TABLE_FILE_SYNC_RULES}
         ORDER BY local_path"
    );

    let rows = core::select_with_crdt(sql, vec![], &state.db)?;

    let mut rules = Vec::new();
    for row in rows {
        let rule_id = get_string(&row, 0);

        // Get backend IDs for this rule
        let backend_sql = format!(
            "SELECT backend_id FROM {TABLE_FILE_SYNC_RULE_BACKENDS} WHERE rule_id = ?"
        );
        let backend_rows = core::select_with_crdt(
            backend_sql,
            vec![JsonValue::String(rule_id.clone())],
            &state.db,
        )?;
        let backend_ids: Vec<String> = backend_rows
            .into_iter()
            .map(|r| get_string(&r, 0))
            .collect();

        rules.push(SyncRule {
            id: rule_id,
            space_id: get_string(&row, 1),
            local_path: get_string(&row, 2),
            direction: parse_sync_direction(&get_string(&row, 3)),
            backend_ids,
            enabled: get_bool(&row, 4),
            created_at: get_string(&row, 5),
            updated_at: get_string(&row, 6),
        });
    }

    Ok(rules)
}

fn sync_direction_to_string(d: &SyncDirection) -> String {
    match d {
        SyncDirection::Up => "up".to_string(),
        SyncDirection::Down => "down".to_string(),
        SyncDirection::Both => "both".to_string(),
    }
}

/// Add a new sync rule
#[tauri::command]
pub async fn filesync_add_sync_rule(
    app_handle: AppHandle,
    state: State<'_, AppState>,
    request: AddSyncRuleRequest,
) -> Result<SyncRule, FileSyncError> {
    let id = uuid::Uuid::new_v4().to_string();
    let direction = request.direction.clone().unwrap_or(SyncDirection::Both);

    // Insert the sync rule with RETURNING
    let sql = format!(
        "INSERT INTO {TABLE_FILE_SYNC_RULES} (id, space_id, local_path, direction, enabled)
         VALUES (?, ?, ?, ?, 1)
         RETURNING id, space_id, local_path, direction, enabled, created_at, updated_at"
    );

    let hlc_service = state.hlc.lock().map_err(|_| FileSyncError::Internal {
        reason: "Failed to lock HLC service".to_string(),
    })?;

    let rows = core::execute_with_crdt(
        sql,
        vec![
            JsonValue::String(id),
            JsonValue::String(request.space_id),
            JsonValue::String(request.local_path),
            JsonValue::String(sync_direction_to_string(&direction)),
        ],
        &state.db,
        &hlc_service,
    )?;

    let row = rows.first().ok_or(FileSyncError::Internal {
        reason: "INSERT did not return created row".to_string(),
    })?;

    let rule_id = get_string(row, 0);

    // Insert backend associations
    for backend_id in &request.backend_ids {
        let assoc_id = uuid::Uuid::new_v4().to_string();
        let assoc_sql = format!(
            "INSERT INTO {TABLE_FILE_SYNC_RULE_BACKENDS} (id, rule_id, backend_id)
             VALUES (?, ?, ?)"
        );
        core::execute_with_crdt(
            assoc_sql,
            vec![
                JsonValue::String(assoc_id),
                JsonValue::String(rule_id.clone()),
                JsonValue::String(backend_id.clone()),
            ],
            &state.db,
            &hlc_service,
        )?;
    }

    let _ = app_handle.emit("crdt:dirty-tables-changed", ());

    Ok(SyncRule {
        id: rule_id,
        space_id: get_string(row, 1),
        local_path: get_string(row, 2),
        direction: parse_sync_direction(&get_string(row, 3)),
        backend_ids: request.backend_ids,
        enabled: get_bool(row, 4),
        created_at: get_string(row, 5),
        updated_at: get_string(row, 6),
    })
}

/// Remove a sync rule
#[tauri::command]
pub async fn filesync_remove_sync_rule(
    app_handle: AppHandle,
    state: State<'_, AppState>,
    rule_id: String,
) -> Result<(), FileSyncError> {
    let hlc_service = state.hlc.lock().map_err(|_| FileSyncError::Internal {
        reason: "Failed to lock HLC service".to_string(),
    })?;

    // Delete backend associations first
    let assoc_sql = format!("DELETE FROM {TABLE_FILE_SYNC_RULE_BACKENDS} WHERE rule_id = ?");
    core::execute_with_crdt(
        assoc_sql,
        vec![JsonValue::String(rule_id.clone())],
        &state.db,
        &hlc_service,
    )?;

    // Delete the sync rule
    let sql = format!("DELETE FROM {TABLE_FILE_SYNC_RULES} WHERE id = ?");
    core::execute_with_crdt(
        sql,
        vec![JsonValue::String(rule_id)],
        &state.db,
        &hlc_service,
    )?;

    let _ = app_handle.emit("crdt:dirty-tables-changed", ());

    Ok(())
}

// ============================================================================
// Sync Operations Commands
// ============================================================================

/// Get current sync status
#[tauri::command]
pub async fn filesync_get_sync_status(
    _state: State<'_, AppState>,
) -> Result<SyncStatus, FileSyncError> {
    // TODO: Return current sync status
    Ok(SyncStatus {
        is_syncing: false,
        pending_uploads: 0,
        pending_downloads: 0,
        last_sync: None,
        errors: vec![],
    })
}

/// Trigger a sync operation
#[tauri::command]
pub async fn filesync_trigger_sync(
    _state: State<'_, AppState>,
) -> Result<(), FileSyncError> {
    // TODO: Start sync engine
    Err(FileSyncError::NotInitialized)
}

/// Pause sync operations
#[tauri::command]
pub async fn filesync_pause_sync(
    _state: State<'_, AppState>,
) -> Result<(), FileSyncError> {
    // TODO: Pause sync engine
    Err(FileSyncError::NotInitialized)
}

/// Resume sync operations
#[tauri::command]
pub async fn filesync_resume_sync(
    _state: State<'_, AppState>,
) -> Result<(), FileSyncError> {
    // TODO: Resume sync engine
    Err(FileSyncError::NotInitialized)
}

// ============================================================================
// Conflict Resolution Commands
// ============================================================================

/// Resolve a file conflict
#[tauri::command]
pub async fn filesync_resolve_conflict(
    _state: State<'_, AppState>,
    request: ResolveConflictRequest,
) -> Result<(), FileSyncError> {
    // TODO: Apply conflict resolution
    let _request = request;
    Err(FileSyncError::NotInitialized)
}

// ============================================================================
// UI Helper Commands
// ============================================================================

/// Open a folder selection dialog
#[tauri::command]
pub async fn filesync_select_folder(
    app_handle: tauri::AppHandle,
) -> Result<Option<String>, FileSyncError> {
    use tauri_plugin_dialog::DialogExt;

    let folder = app_handle
        .dialog()
        .file()
        .blocking_pick_folder();

    Ok(folder.map(|p| p.to_string()))
}
