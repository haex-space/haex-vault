//! Stored-backend resolution with per-rule overrides.

use serde_json::Value as JsonValue;

use crate::database::core;
use crate::database::row::get_string;
use crate::remote_storage::backend::{create_backend, StorageBackend};
use crate::remote_storage::error::StorageError;
use crate::remote_storage::queries::SQL_GET_BACKEND_CONFIG;

/// Short debug label for a JSON value's top-level shape.
fn json_value_shape(value: &JsonValue) -> &'static str {
    match value {
        JsonValue::Null => "null",
        JsonValue::Bool(_) => "bool",
        JsonValue::Number(_) => "number",
        JsonValue::String(_) => "string",
        JsonValue::Array(_) => "array",
        JsonValue::Object(_) => "object",
    }
}

/// Resolve a backend by ID and apply optional, non-persisted rule overrides.
pub async fn get_backend_instance_from_db_with_overrides(
    db: &crate::database::DbConnection,
    backend_id: &str,
    bucket_override: Option<&str>,
    scoped_cred_override: Option<&crate::remote_storage::iam_adapter::ScopedCred>,
) -> Result<Box<dyn StorageBackend>, StorageError> {
    let rows = core::select_with_crdt(
        SQL_GET_BACKEND_CONFIG.clone(),
        vec![JsonValue::String(backend_id.to_string())],
        db,
    )
    .map_err(|e| StorageError::DatabaseError {
        reason: e.to_string(),
    })?;

    let row = rows.first().ok_or_else(|| StorageError::BackendNotFound {
        id: backend_id.to_string(),
    })?;
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

    let mut config: JsonValue =
        serde_json::from_str(&config_str).map_err(|e| StorageError::InvalidConfig {
            reason: format!("Failed to parse config: {e}"),
        })?;

    if let Some(bucket) = bucket_override.filter(|bucket| !bucket.is_empty()) {
        if let Some(obj) = config.as_object_mut() {
            obj.insert("bucket".to_string(), JsonValue::String(bucket.to_string()));
        }
    }

    if let Some(cred) = scoped_cred_override {
        let config_shape = json_value_shape(&config);
        let obj = config
            .as_object_mut()
            .ok_or_else(|| StorageError::InvalidConfig {
                reason: format!(
                    "backend {backend_id} config must be a JSON object to accept a ScopedCred \
                     override, got {config_shape}"
                ),
            })?;
        obj.insert(
            "accessKeyId".to_string(),
            JsonValue::String(cred.access_key_id.clone()),
        );
        obj.insert(
            "secretAccessKey".to_string(),
            JsonValue::String(cred.secret_access_key.clone()),
        );
    }

    create_backend(&backend_type, &config).await
}
