// src-tauri/src/extension/filesync/helpers.rs
//!
//! General helper functions for FileSync operations
//!

use crate::database::core;
use crate::database::error::DatabaseError;
use crate::extension::filesync::error::FileSyncError;
use crate::extension::filesync::queries::*;
use crate::extension::filesync::storage::s3::S3Backend;
use crate::extension::filesync::types::*;
use crate::AppState;
use serde_json::Value as JsonValue;

// ============================================================================
// Error Conversion
// ============================================================================

impl From<DatabaseError> for FileSyncError {
    fn from(e: DatabaseError) -> Self {
        FileSyncError::DatabaseError {
            reason: e.to_string(),
        }
    }
}

// ============================================================================
// Row Parsing Helpers
// ============================================================================

pub fn get_string(row: &[JsonValue], idx: usize) -> String {
    row.get(idx)
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string()
}

pub fn get_bool(row: &[JsonValue], idx: usize) -> bool {
    row.get(idx)
        .and_then(|v| v.as_i64())
        .map(|v| v != 0)
        .unwrap_or(false)
}

pub fn get_u64(row: &[JsonValue], idx: usize) -> u64 {
    row.get(idx).and_then(|v| v.as_i64()).unwrap_or(0) as u64
}

// ============================================================================
// Row Mapping
// ============================================================================

/// Convert row from SQL_LIST_FILES query to FileInfo
pub fn row_to_file_info(row: &[JsonValue], state: &AppState) -> Result<FileInfo, FileSyncError> {
    let file_id = get_string(row, 0);

    // Get backend IDs for this file
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
        id: file_id,
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
        sync_state: parse_sync_state(&get_string(row, 8)),
        backends,
        created_at: get_string(row, 9),
        updated_at: get_string(row, 10),
    })
}

// ============================================================================
// Backend Factory
// ============================================================================

/// Create S3 backend from database config
pub async fn create_backend_from_db(
    state: &AppState,
    backend_id: &str,
) -> Result<(S3Backend, StorageBackendType), FileSyncError> {
    let rows = core::select_with_crdt(
        SQL_GET_BACKEND.clone(),
        vec![JsonValue::String(backend_id.to_string())],
        &state.db,
    )?;

    let row = rows.first().ok_or_else(|| FileSyncError::BackendNotFound {
        id: backend_id.to_string(),
    })?;

    let backend_type_str = get_string(row, 0);
    let config_json = get_string(row, 1);

    let backend_type = parse_backend_type(&backend_type_str).ok_or_else(|| {
        FileSyncError::Internal {
            reason: format!("Unknown backend type: {}", backend_type_str),
        }
    })?;

    let config: BackendConfig = serde_json::from_str(&config_json).map_err(|e| {
        FileSyncError::Internal {
            reason: format!("Failed to parse backend config: {}", e),
        }
    })?;

    let s3_config = match &config {
        BackendConfig::S3(c) | BackendConfig::R2(c) | BackendConfig::Minio(c) => c,
    };

    let backend = S3Backend::new(s3_config, backend_type.clone()).await?;
    Ok((backend, backend_type))
}

// ============================================================================
// Type Conversion
// ============================================================================

pub fn sync_state_to_string(state: &FileSyncState) -> &'static str {
    match state {
        FileSyncState::Synced => "synced",
        FileSyncState::Syncing => "syncing",
        FileSyncState::LocalOnly => "local_only",
        FileSyncState::RemoteOnly => "remote_only",
        FileSyncState::Conflict => "conflict",
        FileSyncState::Error => "error",
    }
}

pub fn parse_sync_state(s: &str) -> FileSyncState {
    match s {
        "synced" => FileSyncState::Synced,
        "syncing" => FileSyncState::Syncing,
        "local_only" => FileSyncState::LocalOnly,
        "remote_only" => FileSyncState::RemoteOnly,
        "conflict" => FileSyncState::Conflict,
        _ => FileSyncState::Error,
    }
}

pub fn parse_backend_type(s: &str) -> Option<StorageBackendType> {
    match s {
        "s3" => Some(StorageBackendType::S3),
        "r2" => Some(StorageBackendType::R2),
        "minio" => Some(StorageBackendType::Minio),
        "gdrive" => Some(StorageBackendType::GDrive),
        "dropbox" => Some(StorageBackendType::Dropbox),
        _ => None,
    }
}

pub fn parse_sync_direction(s: &str) -> SyncDirection {
    match s {
        "up" => SyncDirection::Up,
        "down" => SyncDirection::Down,
        _ => SyncDirection::Both,
    }
}

pub fn sync_direction_to_string(d: &SyncDirection) -> String {
    match d {
        SyncDirection::Up => "up".to_string(),
        SyncDirection::Down => "down".to_string(),
        SyncDirection::Both => "both".to_string(),
    }
}
