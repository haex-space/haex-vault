// src-tauri/src/extension/filesync/commands.rs
//!
//! Tauri Commands for FileSync
//!
//! These commands are called from extensions via the SDK's FileSyncAPI.
//!

use crate::database::core;
use crate::extension::filesync::encryption::FileEncryption;
use crate::extension::filesync::error::FileSyncError;
use crate::extension::filesync::file_io::*;
use crate::extension::filesync::helpers::*;
use crate::extension::filesync::queries::*;
use crate::extension::filesync::scanner::*;
use crate::extension::filesync::storage::s3::S3Backend;
use crate::extension::filesync::storage::StorageBackend;
use crate::extension::filesync::types::*;
use crate::extension::permissions::manager::PermissionManager;
use crate::extension::permissions::types::{Action, FileSyncAction, FileSyncTarget, FsAction};
use crate::table_names::{
    TABLE_FILE_BACKENDS, TABLE_FILE_SPACES, TABLE_FILE_SYNC_RULES, TABLE_FILE_SYNC_RULE_BACKENDS,
};
use crate::AppState;
use serde_json::Value as JsonValue;
use std::io::Cursor;
use std::path::Path;
use tauri::{AppHandle, Emitter, State};
#[cfg(desktop)]
use tauri_plugin_dialog::DialogExt;

/// Event emitted when CRDT tables have pending changes
const EVENT_CRDT_DIRTY_TABLES_CHANGED: &str = "crdt:dirty-tables-changed";

// ============================================================================
// Spaces Commands
// ============================================================================

/// List all file spaces
#[tauri::command]
pub async fn filesync_list_spaces(
    state: State<'_, AppState>,
    public_key: String,
    name: String,
) -> Result<Vec<FileSpace>, FileSyncError> {
    let extension = state
        .extension_manager
        .get_extension_by_public_key_and_name(&public_key, &name)
        .map_err(|e| FileSyncError::Internal { reason: e.to_string() })?
        .ok_or_else(|| FileSyncError::ExtensionNotFound {
            public_key: public_key.clone(),
            name: name.clone(),
        })?;

    PermissionManager::check_filesync_permission(
        &state,
        &extension.id,
        FileSyncAction::Read,
        FileSyncTarget::Spaces,
    )
    .await
    ?;

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
    public_key: String,
    name: String,
    request: CreateSpaceRequest,
) -> Result<FileSpace, FileSyncError> {
    let extension = state
        .extension_manager
        .get_extension_by_public_key_and_name(&public_key, &name)
        .map_err(|e| FileSyncError::Internal { reason: e.to_string() })?
        .ok_or_else(|| FileSyncError::ExtensionNotFound {
            public_key: public_key.clone(),
            name: name.clone(),
        })?;

    PermissionManager::check_filesync_permission(
        &state,
        &extension.id,
        FileSyncAction::ReadWrite,
        FileSyncTarget::Spaces,
    )
    .await
    ?;

    let id = uuid::Uuid::new_v4().to_string();
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

    let _ = app_handle.emit(EVENT_CRDT_DIRTY_TABLES_CHANGED, ());

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
    public_key: String,
    name: String,
    space_id: String,
) -> Result<(), FileSyncError> {
    let extension = state
        .extension_manager
        .get_extension_by_public_key_and_name(&public_key, &name)
        .map_err(|e| FileSyncError::Internal { reason: e.to_string() })?
        .ok_or_else(|| FileSyncError::ExtensionNotFound {
            public_key: public_key.clone(),
            name: name.clone(),
        })?;

    PermissionManager::check_filesync_permission(
        &state,
        &extension.id,
        FileSyncAction::ReadWrite,
        FileSyncTarget::Spaces,
    )
    .await
    ?;

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

    let _ = app_handle.emit(EVENT_CRDT_DIRTY_TABLES_CHANGED, ());

    Ok(())
}

// ============================================================================
// Files Commands
// ============================================================================

/// List files in a space
#[tauri::command]
pub async fn filesync_list_files(
    state: State<'_, AppState>,
    public_key: String,
    name: String,
    request: ListFilesRequest,
) -> Result<Vec<FileInfo>, FileSyncError> {
    let extension = state
        .extension_manager
        .get_extension_by_public_key_and_name(&public_key, &name)
        .map_err(|e| FileSyncError::Internal { reason: e.to_string() })?
        .ok_or_else(|| FileSyncError::ExtensionNotFound {
            public_key: public_key.clone(),
            name: name.clone(),
        })?;

    PermissionManager::check_filesync_permission(
        &state,
        &extension.id,
        FileSyncAction::Read,
        FileSyncTarget::Spaces,
    )
    .await
    ?;

    let (sql, params) = if let Some(path) = &request.path {
        if path == "/" || path.is_empty() {
            (
                SQL_LIST_FILES_ROOT.clone(),
                vec![JsonValue::String(request.space_id.clone())],
            )
        } else {
            (
                SQL_LIST_FILES_IN_FOLDER.clone(),
                vec![
                    JsonValue::String(request.space_id.clone()),
                    JsonValue::String(path.clone()),
                ],
            )
        }
    } else {
        (
            SQL_LIST_FILES.clone(),
            vec![JsonValue::String(request.space_id.clone())],
        )
    };

    let rows = core::select_with_crdt(sql, params, &state.db)?;

    let mut files = Vec::new();
    for row in rows {
        files.push(row_to_file_info(&row, &state)?);
    }

    Ok(files)
}

/// Get file info by ID
#[tauri::command]
pub async fn filesync_get_file(
    state: State<'_, AppState>,
    public_key: String,
    name: String,
    file_id: String,
) -> Result<FileInfo, FileSyncError> {
    let extension = state
        .extension_manager
        .get_extension_by_public_key_and_name(&public_key, &name)
        .map_err(|e| FileSyncError::Internal { reason: e.to_string() })?
        .ok_or_else(|| FileSyncError::ExtensionNotFound {
            public_key: public_key.clone(),
            name: name.clone(),
        })?;

    PermissionManager::check_filesync_permission(
        &state,
        &extension.id,
        FileSyncAction::Read,
        FileSyncTarget::Spaces,
    )
    .await
    ?;

    let rows = core::select_with_crdt(
        SQL_GET_FILE.clone(),
        vec![JsonValue::String(file_id.clone())],
        &state.db,
    )?;

    let row = rows.first().ok_or_else(|| FileSyncError::FileNotFound {
        id: file_id.clone(),
    })?;

    let backend_rows = core::select_with_crdt(
        SQL_GET_FILE_BACKENDS.clone(),
        vec![JsonValue::String(file_id.clone())],
        &state.db,
    )?;
    let backends: Vec<String> = backend_rows
        .into_iter()
        .map(|r| get_string(&r, 0))
        .collect();

    Ok(FileInfo {
        id: get_string(row, 0),
        space_id: get_string(row, 1),
        name: get_string(row, 2),
        path: get_string(row, 3),
        mime_type: {
            let mt = get_string(row, 4);
            if mt.is_empty() { None } else { Some(mt) }
        },
        is_directory: get_bool(row, 5),
        size: get_u64(row, 6),
        content_hash: get_string(row, 7),
        sync_state: parse_sync_state(&get_string(row, 9)),
        backends,
        created_at: get_string(row, 10),
        updated_at: get_string(row, 11),
    })
}

/// Upload a file to the space
#[tauri::command]
pub async fn filesync_upload_file(
    app_handle: AppHandle,
    state: State<'_, AppState>,
    public_key: String,
    name: String,
    request: UploadFileRequest,
) -> Result<FileInfo, FileSyncError> {
    let extension = state
        .extension_manager
        .get_extension_by_public_key_and_name(&public_key, &name)
        .map_err(|e| FileSyncError::Internal { reason: e.to_string() })?
        .ok_or_else(|| FileSyncError::ExtensionNotFound {
            public_key: public_key.clone(),
            name: name.clone(),
        })?;

    PermissionManager::check_filesync_permission(
        &state,
        &extension.id,
        FileSyncAction::ReadWrite,
        FileSyncTarget::Spaces,
    )
    .await
    ?;

    PermissionManager::check_filesystem_permission(
        &state,
        &extension.id,
        Action::Filesystem(FsAction::Read),
        Path::new(&request.local_path),
    )
    .await
    ?;

    #[cfg(desktop)]
    let file_data = read_file_bytes(&request.local_path)?;

    #[cfg(target_os = "android")]
    let file_data = read_file_bytes_android(&app_handle, &request.local_path)?;

    let file_size = file_data.len() as u64;
    let filename = extract_filename(&request.local_path);
    let remote_path = request.remote_path.unwrap_or_else(|| format!("/{}", filename));

    let mime_type = mime_guess::from_path(&filename)
        .first()
        .map(|m| m.to_string());

    let file_key = FileEncryption::generate_key();
    let file_key_hex = hex::encode(file_key);

    let file_id = uuid::Uuid::new_v4().to_string();
    let encryption = FileEncryption::new(file_key);

    let mut encrypted_data = Vec::new();
    let (chunk_count, content_hash) = encryption.encrypt_file(
        Cursor::new(&file_data),
        &mut encrypted_data,
        &file_id,
    )?;

    let backend_ids = if let Some(ids) = request.backend_ids {
        ids
    } else {
        let rows = core::select_with_crdt(SQL_LIST_ENABLED_BACKENDS.clone(), vec![], &state.db)?;
        rows.into_iter().map(|r| get_string(&r, 0)).collect()
    };

    if backend_ids.is_empty() {
        return Err(FileSyncError::NoBackendsConfigured);
    }

    let remote_id = format!("files/{}/{}", request.space_id, file_id);
    let mut uploaded_backends = Vec::new();

    for backend_id in &backend_ids {
        let (backend, _backend_type) = create_backend_from_db(&state, backend_id).await?;
        backend.upload(&remote_id, &encrypted_data, None).await?;
        uploaded_backends.push(backend_id.clone());
    }

    let hlc_service = state.hlc.lock().map_err(|_| FileSyncError::Internal {
        reason: "Failed to lock HLC service".to_string(),
    })?;

    let sync_state = if uploaded_backends.is_empty() {
        FileSyncState::LocalOnly
    } else {
        FileSyncState::Synced
    };

    core::execute_with_crdt(
        SQL_INSERT_FILE.clone(),
        vec![
            JsonValue::String(file_id.clone()),
            JsonValue::String(request.space_id.clone()),
            JsonValue::String(filename.clone()),
            JsonValue::String(remote_path.clone()),
            JsonValue::String(mime_type.clone().unwrap_or_default()),
            JsonValue::Bool(false),
            JsonValue::Number(file_size.into()),
            JsonValue::String(content_hash.clone()),
            JsonValue::String(file_key_hex),
            JsonValue::Number(chunk_count.into()),
            JsonValue::String(sync_state_to_string(&sync_state).to_string()),
        ],
        &state.db,
        &hlc_service,
    )?;

    for backend_id in &uploaded_backends {
        let mapping_id = uuid::Uuid::new_v4().to_string();
        core::execute_with_crdt(
            SQL_INSERT_BACKEND_MAPPING.clone(),
            vec![
                JsonValue::String(mapping_id),
                JsonValue::String(file_id.clone()),
                JsonValue::String(backend_id.clone()),
                JsonValue::String(remote_id.clone()),
            ],
            &state.db,
            &hlc_service,
        )?;
    }

    core::execute_with_crdt(
        SQL_UPDATE_SPACE_COUNTS.clone(),
        vec![
            JsonValue::Number(1.into()),
            JsonValue::Number((file_size as i64).into()),
            JsonValue::String(request.space_id.clone()),
        ],
        &state.db,
        &hlc_service,
    )?;

    let _ = app_handle.emit(EVENT_CRDT_DIRTY_TABLES_CHANGED, ());

    let rows = core::select_with_crdt(
        SQL_GET_FILE.clone(),
        vec![JsonValue::String(file_id.clone())],
        &state.db,
    )?;

    let row = rows.first().ok_or_else(|| FileSyncError::Internal {
        reason: "Failed to retrieve created file".to_string(),
    })?;

    Ok(FileInfo {
        id: get_string(row, 0),
        space_id: get_string(row, 1),
        name: get_string(row, 2),
        path: get_string(row, 3),
        mime_type: {
            let mt = get_string(row, 4);
            if mt.is_empty() { None } else { Some(mt) }
        },
        is_directory: get_bool(row, 5),
        size: get_u64(row, 6),
        content_hash: get_string(row, 7),
        sync_state: parse_sync_state(&get_string(row, 9)),
        backends: uploaded_backends,
        created_at: get_string(row, 10),
        updated_at: get_string(row, 11),
    })
}

/// Download a file from the space
#[tauri::command]
pub async fn filesync_download_file(
    #[allow(unused_variables)] app_handle: AppHandle,
    state: State<'_, AppState>,
    public_key: String,
    name: String,
    request: DownloadFileRequest,
) -> Result<(), FileSyncError> {
    let extension = state
        .extension_manager
        .get_extension_by_public_key_and_name(&public_key, &name)
        .map_err(|e| FileSyncError::Internal { reason: e.to_string() })?
        .ok_or_else(|| FileSyncError::ExtensionNotFound {
            public_key: public_key.clone(),
            name: name.clone(),
        })?;

    PermissionManager::check_filesync_permission(
        &state,
        &extension.id,
        FileSyncAction::Read,
        FileSyncTarget::Spaces,
    )
    .await
    ?;

    PermissionManager::check_filesystem_permission(
        &state,
        &extension.id,
        Action::Filesystem(FsAction::ReadWrite),
        Path::new(&request.local_path),
    )
    .await
    ?;

    let rows = core::select_with_crdt(
        SQL_GET_FILE.clone(),
        vec![JsonValue::String(request.file_id.clone())],
        &state.db,
    )?;

    let row = rows.first().ok_or_else(|| FileSyncError::FileNotFound {
        id: request.file_id.clone(),
    })?;

    let file_key_hex = get_string(row, 8);
    let file_key_bytes = hex::decode(&file_key_hex).map_err(|e| FileSyncError::Internal {
        reason: format!("Invalid file key format: {}", e),
    })?;

    if file_key_bytes.len() != 32 {
        return Err(FileSyncError::Internal {
            reason: format!("File key has wrong size: {} bytes", file_key_bytes.len()),
        });
    }

    let mut file_key = [0u8; 32];
    file_key.copy_from_slice(&file_key_bytes);

    let mapping_rows = core::select_with_crdt(
        SQL_GET_FILE_BACKENDS.clone(),
        vec![JsonValue::String(request.file_id.clone())],
        &state.db,
    )?;

    let backend_id = mapping_rows
        .first()
        .map(|r| get_string(r, 0))
        .ok_or_else(|| FileSyncError::Internal {
            reason: "No backend mapping found for file".to_string(),
        })?;

    let remote_id_rows = core::select_with_crdt(
        SQL_GET_FILE_REMOTE_ID.clone(),
        vec![
            JsonValue::String(request.file_id.clone()),
            JsonValue::String(backend_id.clone()),
        ],
        &state.db,
    )?;

    let remote_id = remote_id_rows
        .first()
        .map(|r| get_string(r, 0))
        .ok_or_else(|| FileSyncError::Internal {
            reason: "No remote_id found for file".to_string(),
        })?;

    let (backend, _backend_type) = create_backend_from_db(&state, &backend_id).await?;
    let encrypted_data = backend.download(&remote_id).await?;

    let encryption = FileEncryption::new(file_key);
    let mut decrypted_data = Vec::new();
    encryption.decrypt_file(
        Cursor::new(&encrypted_data),
        &mut decrypted_data,
    )?;

    #[cfg(desktop)]
    write_file_bytes(&request.local_path, &decrypted_data)?;

    #[cfg(target_os = "android")]
    write_file_bytes_android(&app_handle, &request.local_path, &decrypted_data)?;

    Ok(())
}

/// Delete a file
#[tauri::command]
pub async fn filesync_delete_file(
    state: State<'_, AppState>,
    public_key: String,
    name: String,
    file_id: String,
) -> Result<(), FileSyncError> {
    let extension = state
        .extension_manager
        .get_extension_by_public_key_and_name(&public_key, &name)
        .map_err(|e| FileSyncError::Internal { reason: e.to_string() })?
        .ok_or_else(|| FileSyncError::ExtensionNotFound {
            public_key: public_key.clone(),
            name: name.clone(),
        })?;

    PermissionManager::check_filesync_permission(
        &state,
        &extension.id,
        FileSyncAction::ReadWrite,
        FileSyncTarget::Spaces,
    )
    .await
    ?;

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
    public_key: String,
    name: String,
) -> Result<Vec<StorageBackendInfo>, FileSyncError> {
    let extension = state
        .extension_manager
        .get_extension_by_public_key_and_name(&public_key, &name)
        .map_err(|e| FileSyncError::Internal { reason: e.to_string() })?
        .ok_or_else(|| FileSyncError::ExtensionNotFound {
            public_key: public_key.clone(),
            name: name.clone(),
        })?;

    PermissionManager::check_filesync_permission(
        &state,
        &extension.id,
        FileSyncAction::Read,
        FileSyncTarget::Backends,
    )
    .await
    ?;

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

/// Add a new storage backend
#[tauri::command]
pub async fn filesync_add_backend(
    app_handle: AppHandle,
    state: State<'_, AppState>,
    public_key: String,
    name: String,
    request: AddBackendRequest,
) -> Result<StorageBackendInfo, FileSyncError> {
    let extension = state
        .extension_manager
        .get_extension_by_public_key_and_name(&public_key, &name)
        .map_err(|e| FileSyncError::Internal { reason: e.to_string() })?
        .ok_or_else(|| FileSyncError::ExtensionNotFound {
            public_key: public_key.clone(),
            name: name.clone(),
        })?;

    PermissionManager::check_filesync_permission(
        &state,
        &extension.id,
        FileSyncAction::ReadWrite,
        FileSyncTarget::Backends,
    )
    .await
    ?;

    let id = uuid::Uuid::new_v4().to_string();

    let backend_type = match &request.config {
        BackendConfig::S3(_) => StorageBackendType::S3,
        BackendConfig::R2(_) => StorageBackendType::R2,
        BackendConfig::Minio(_) => StorageBackendType::Minio,
    };

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
            JsonValue::String(backend_type.to_string()),
            JsonValue::String(request.name),
            JsonValue::String(encrypted_config),
        ],
        &state.db,
        &hlc_service,
    )?;

    let _ = app_handle.emit(EVENT_CRDT_DIRTY_TABLES_CHANGED, ());

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
    public_key: String,
    name: String,
    backend_id: String,
) -> Result<(), FileSyncError> {
    let extension = state
        .extension_manager
        .get_extension_by_public_key_and_name(&public_key, &name)
        .map_err(|e| FileSyncError::Internal { reason: e.to_string() })?
        .ok_or_else(|| FileSyncError::ExtensionNotFound {
            public_key: public_key.clone(),
            name: name.clone(),
        })?;

    PermissionManager::check_filesync_permission(
        &state,
        &extension.id,
        FileSyncAction::ReadWrite,
        FileSyncTarget::Backends,
    )
    .await
    ?;

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

    let _ = app_handle.emit(EVENT_CRDT_DIRTY_TABLES_CHANGED, ());

    Ok(())
}

/// Test connection to a storage backend
#[tauri::command]
pub async fn filesync_test_backend(
    state: State<'_, AppState>,
    public_key: String,
    name: String,
    backend_id: String,
) -> Result<(), FileSyncError> {
    let extension = state
        .extension_manager
        .get_extension_by_public_key_and_name(&public_key, &name)
        .map_err(|e| FileSyncError::Internal { reason: e.to_string() })?
        .ok_or_else(|| FileSyncError::ExtensionNotFound {
            public_key: public_key.clone(),
            name: name.clone(),
        })?;

    PermissionManager::check_filesync_permission(
        &state,
        &extension.id,
        FileSyncAction::Read,
        FileSyncTarget::Backends,
    )
    .await
    ?;

    let sql = format!(
        "SELECT type, encrypted_config FROM {TABLE_FILE_BACKENDS} WHERE id = ?"
    );

    let rows = core::select_with_crdt(sql, vec![JsonValue::String(backend_id.clone())], &state.db)?;

    let row = rows.first().ok_or(FileSyncError::BackendNotFound {
        id: backend_id.clone(),
    })?;

    let _backend_type = get_string(row, 0);
    let encrypted_config = get_string(row, 1);

    let config: BackendConfig = serde_json::from_str(&encrypted_config)
        .map_err(|e| FileSyncError::Internal {
            reason: format!("Failed to parse config: {}", e),
        })?;

    match config {
        BackendConfig::S3(s3_config) => {
            let backend = S3Backend::new(&s3_config, StorageBackendType::S3).await?;
            backend.test_connection().await?;
            Ok(())
        }
        BackendConfig::R2(s3_config) => {
            let backend = S3Backend::new(&s3_config, StorageBackendType::R2).await?;
            backend.test_connection().await?;
            Ok(())
        }
        BackendConfig::Minio(s3_config) => {
            let backend = S3Backend::new(&s3_config, StorageBackendType::Minio).await?;
            backend.test_connection().await?;
            Ok(())
        }
    }
}

// ============================================================================
// Sync Rules Commands
// ============================================================================

/// List all sync rules
#[tauri::command]
pub async fn filesync_list_sync_rules(
    state: State<'_, AppState>,
    public_key: String,
    name: String,
) -> Result<Vec<SyncRule>, FileSyncError> {
    let extension = state
        .extension_manager
        .get_extension_by_public_key_and_name(&public_key, &name)
        .map_err(|e| FileSyncError::Internal { reason: e.to_string() })?
        .ok_or_else(|| FileSyncError::ExtensionNotFound {
            public_key: public_key.clone(),
            name: name.clone(),
        })?;

    PermissionManager::check_filesync_permission(
        &state,
        &extension.id,
        FileSyncAction::Read,
        FileSyncTarget::Rules,
    )
    .await
    ?;

    let sql = format!(
        "SELECT id, space_id, local_path, direction, enabled, created_at, updated_at
         FROM {TABLE_FILE_SYNC_RULES}
         ORDER BY local_path"
    );

    let rows = core::select_with_crdt(sql, vec![], &state.db)?;

    let mut rules = Vec::new();
    for row in rows {
        let rule_id = get_string(&row, 0);

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

/// Add a new sync rule
#[tauri::command]
pub async fn filesync_add_sync_rule(
    app_handle: AppHandle,
    state: State<'_, AppState>,
    public_key: String,
    name: String,
    request: AddSyncRuleRequest,
) -> Result<SyncRule, FileSyncError> {
    let extension = state
        .extension_manager
        .get_extension_by_public_key_and_name(&public_key, &name)
        .map_err(|e| FileSyncError::Internal { reason: e.to_string() })?
        .ok_or_else(|| FileSyncError::ExtensionNotFound {
            public_key: public_key.clone(),
            name: name.clone(),
        })?;

    PermissionManager::check_filesync_permission(
        &state,
        &extension.id,
        FileSyncAction::ReadWrite,
        FileSyncTarget::Rules,
    )
    .await
    ?;

    let id = uuid::Uuid::new_v4().to_string();
    let direction = request.direction.clone().unwrap_or(SyncDirection::Both);

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

    let _ = app_handle.emit(EVENT_CRDT_DIRTY_TABLES_CHANGED, ());

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

/// Update a sync rule
#[tauri::command]
pub async fn filesync_update_sync_rule(
    app_handle: AppHandle,
    state: State<'_, AppState>,
    public_key: String,
    name: String,
    request: UpdateSyncRuleRequest,
) -> Result<SyncRule, FileSyncError> {
    let extension = state
        .extension_manager
        .get_extension_by_public_key_and_name(&public_key, &name)
        .map_err(|e| FileSyncError::Internal { reason: e.to_string() })?
        .ok_or_else(|| FileSyncError::ExtensionNotFound {
            public_key: public_key.clone(),
            name: name.clone(),
        })?;

    PermissionManager::check_filesync_permission(
        &state,
        &extension.id,
        FileSyncAction::ReadWrite,
        FileSyncTarget::Rules,
    )
    .await
    ?;

    let hlc_service = state.hlc.lock().map_err(|_| FileSyncError::Internal {
        reason: "Failed to lock HLC service".to_string(),
    })?;

    if let Some(direction) = &request.direction {
        core::execute_with_crdt(
            SQL_UPDATE_SYNC_RULE_DIRECTION.clone(),
            vec![
                JsonValue::String(sync_direction_to_string(direction)),
                JsonValue::String(request.rule_id.clone()),
            ],
            &state.db,
            &hlc_service,
        )?;
    }

    if let Some(enabled) = request.enabled {
        core::execute_with_crdt(
            SQL_UPDATE_SYNC_RULE_ENABLED.clone(),
            vec![
                JsonValue::Bool(enabled),
                JsonValue::String(request.rule_id.clone()),
            ],
            &state.db,
            &hlc_service,
        )?;
    }

    if let Some(backend_ids) = &request.backend_ids {
        core::execute_with_crdt(
            SQL_DELETE_RULE_BACKENDS.clone(),
            vec![JsonValue::String(request.rule_id.clone())],
            &state.db,
            &hlc_service,
        )?;

        for backend_id in backend_ids {
            let assoc_id = uuid::Uuid::new_v4().to_string();
            core::execute_with_crdt(
                SQL_INSERT_RULE_BACKEND.clone(),
                vec![
                    JsonValue::String(assoc_id),
                    JsonValue::String(request.rule_id.clone()),
                    JsonValue::String(backend_id.clone()),
                ],
                &state.db,
                &hlc_service,
            )?;
        }
    }

    let _ = app_handle.emit(EVENT_CRDT_DIRTY_TABLES_CHANGED, ());

    let rows = core::select_with_crdt(
        SQL_GET_SYNC_RULE.clone(),
        vec![JsonValue::String(request.rule_id.clone())],
        &state.db,
    )?;

    let row = rows.first().ok_or(FileSyncError::SyncRuleNotFound {
        id: request.rule_id.clone(),
    })?;

    let backend_rows = core::select_with_crdt(
        SQL_GET_RULE_BACKENDS.clone(),
        vec![JsonValue::String(request.rule_id.clone())],
        &state.db,
    )?;

    let backend_ids: Vec<String> = backend_rows
        .iter()
        .map(|r| get_string(r, 0))
        .collect();

    Ok(SyncRule {
        id: get_string(row, 0),
        space_id: get_string(row, 1),
        local_path: get_string(row, 2),
        direction: parse_sync_direction(&get_string(row, 3)),
        backend_ids,
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
    public_key: String,
    name: String,
    rule_id: String,
) -> Result<(), FileSyncError> {
    let extension = state
        .extension_manager
        .get_extension_by_public_key_and_name(&public_key, &name)
        .map_err(|e| FileSyncError::Internal { reason: e.to_string() })?
        .ok_or_else(|| FileSyncError::ExtensionNotFound {
            public_key: public_key.clone(),
            name: name.clone(),
        })?;

    PermissionManager::check_filesync_permission(
        &state,
        &extension.id,
        FileSyncAction::ReadWrite,
        FileSyncTarget::Rules,
    )
    .await
    ?;

    let hlc_service = state.hlc.lock().map_err(|_| FileSyncError::Internal {
        reason: "Failed to lock HLC service".to_string(),
    })?;

    let assoc_sql = format!("DELETE FROM {TABLE_FILE_SYNC_RULE_BACKENDS} WHERE rule_id = ?");
    core::execute_with_crdt(
        assoc_sql,
        vec![JsonValue::String(rule_id.clone())],
        &state.db,
        &hlc_service,
    )?;

    let sql = format!("DELETE FROM {TABLE_FILE_SYNC_RULES} WHERE id = ?");
    core::execute_with_crdt(
        sql,
        vec![JsonValue::String(rule_id)],
        &state.db,
        &hlc_service,
    )?;

    let _ = app_handle.emit(EVENT_CRDT_DIRTY_TABLES_CHANGED, ());

    Ok(())
}

// ============================================================================
// Sync Operations Commands
// ============================================================================

/// Get current sync status
#[tauri::command]
pub async fn filesync_get_sync_status(
    state: State<'_, AppState>,
    public_key: String,
    name: String,
) -> Result<SyncStatus, FileSyncError> {
    let extension = state
        .extension_manager
        .get_extension_by_public_key_and_name(&public_key, &name)
        .map_err(|e| FileSyncError::Internal { reason: e.to_string() })?
        .ok_or_else(|| FileSyncError::ExtensionNotFound {
            public_key: public_key.clone(),
            name: name.clone(),
        })?;

    PermissionManager::check_filesync_permission(
        &state,
        &extension.id,
        FileSyncAction::Read,
        FileSyncTarget::All,
    )
    .await
    ?;

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
    state: State<'_, AppState>,
    public_key: String,
    name: String,
) -> Result<(), FileSyncError> {
    let extension = state
        .extension_manager
        .get_extension_by_public_key_and_name(&public_key, &name)
        .map_err(|e| FileSyncError::Internal { reason: e.to_string() })?
        .ok_or_else(|| FileSyncError::ExtensionNotFound {
            public_key: public_key.clone(),
            name: name.clone(),
        })?;

    PermissionManager::check_filesync_permission(
        &state,
        &extension.id,
        FileSyncAction::ReadWrite,
        FileSyncTarget::All,
    )
    .await
    ?;

    Err(FileSyncError::NotInitialized)
}

/// Pause sync operations
#[tauri::command]
pub async fn filesync_pause_sync(
    state: State<'_, AppState>,
    public_key: String,
    name: String,
) -> Result<(), FileSyncError> {
    let extension = state
        .extension_manager
        .get_extension_by_public_key_and_name(&public_key, &name)
        .map_err(|e| FileSyncError::Internal { reason: e.to_string() })?
        .ok_or_else(|| FileSyncError::ExtensionNotFound {
            public_key: public_key.clone(),
            name: name.clone(),
        })?;

    PermissionManager::check_filesync_permission(
        &state,
        &extension.id,
        FileSyncAction::ReadWrite,
        FileSyncTarget::All,
    )
    .await
    ?;

    Err(FileSyncError::NotInitialized)
}

/// Resume sync operations
#[tauri::command]
pub async fn filesync_resume_sync(
    state: State<'_, AppState>,
    public_key: String,
    name: String,
) -> Result<(), FileSyncError> {
    let extension = state
        .extension_manager
        .get_extension_by_public_key_and_name(&public_key, &name)
        .map_err(|e| FileSyncError::Internal { reason: e.to_string() })?
        .ok_or_else(|| FileSyncError::ExtensionNotFound {
            public_key: public_key.clone(),
            name: name.clone(),
        })?;

    PermissionManager::check_filesync_permission(
        &state,
        &extension.id,
        FileSyncAction::ReadWrite,
        FileSyncTarget::All,
    )
    .await
    ?;

    Err(FileSyncError::NotInitialized)
}

// ============================================================================
// Conflict Resolution Commands
// ============================================================================

/// Resolve a file conflict
#[tauri::command]
pub async fn filesync_resolve_conflict(
    state: State<'_, AppState>,
    public_key: String,
    name: String,
    request: ResolveConflictRequest,
) -> Result<(), FileSyncError> {
    let extension = state
        .extension_manager
        .get_extension_by_public_key_and_name(&public_key, &name)
        .map_err(|e| FileSyncError::Internal { reason: e.to_string() })?
        .ok_or_else(|| FileSyncError::ExtensionNotFound {
            public_key: public_key.clone(),
            name: name.clone(),
        })?;

    PermissionManager::check_filesync_permission(
        &state,
        &extension.id,
        FileSyncAction::ReadWrite,
        FileSyncTarget::Spaces,
    )
    .await
    ?;

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
    #[cfg(desktop)]
    {
        let folder = app_handle.dialog().file().blocking_pick_folder();
        Ok(folder.and_then(|p| p.as_path().map(|path| path.to_string_lossy().to_string())))
    }

    #[cfg(target_os = "android")]
    {
        use tauri_plugin_android_fs::AndroidFsExt;

        let api = app_handle.android_fs();
        let selected = api
            .file_picker()
            .pick_dir(None, false)
            .map_err(|e| FileSyncError::FilesystemError {
                reason: e.to_string(),
            })?;

        if let Some(dir_uri) = selected {
            let _ = api.take_persistable_uri_permission(&dir_uri);
            Ok(Some(format!("{:?}", dir_uri)))
        } else {
            Ok(None)
        }
    }
}

// ============================================================================
// Local Directory Scanning Commands
// ============================================================================

/// Scan local files in a sync rule folder
#[tauri::command]
pub async fn filesync_scan_local(
    #[allow(unused_variables)] app_handle: AppHandle,
    state: State<'_, AppState>,
    public_key: String,
    name: String,
    request: ScanLocalRequest,
) -> Result<Vec<LocalFileInfo>, FileSyncError> {
    let extension = state
        .extension_manager
        .get_extension_by_public_key_and_name(&public_key, &name)
        .map_err(|e| FileSyncError::Internal { reason: e.to_string() })?
        .ok_or_else(|| FileSyncError::ExtensionNotFound {
            public_key: public_key.clone(),
            name: name.clone(),
        })?;

    PermissionManager::check_filesync_permission(
        &state,
        &extension.id,
        FileSyncAction::Read,
        FileSyncTarget::Rules,
    )
    .await
    ?;

    let rows = core::select_with_crdt(
        SQL_GET_SYNC_RULE.clone(),
        vec![JsonValue::String(request.rule_id.clone())],
        &state.db,
    )?;

    let row = rows.first().ok_or(FileSyncError::SyncRuleNotFound {
        id: request.rule_id.clone(),
    })?;

    let base_path = get_string(row, 2);

    let scan_path = if let Some(subpath) = &request.subpath {
        format!("{}/{}", base_path, subpath)
    } else {
        base_path.clone()
    };

    #[cfg(desktop)]
    {
        scan_local_directory_desktop(&request.rule_id, &scan_path, &base_path)
    }

    #[cfg(target_os = "android")]
    {
        scan_local_directory_android(&app_handle, &request.rule_id, &scan_path, &base_path)
    }
}
