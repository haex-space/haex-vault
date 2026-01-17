// src-tauri/src/storage/types.rs
//!
//! Storage Types - Generische Storage-Typen für alle Extensions
//!

use serde::{Deserialize, Serialize};
use ts_rs::TS;

// ============================================================================
// Storage Backend Types
// ============================================================================

/// Storage backend info (public, without credentials)
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct StorageBackendInfo {
    pub id: String,
    /// Backend type (e.g., "s3")
    pub r#type: String,
    pub name: String,
    pub enabled: bool,
    pub created_at: String,
    /// Public config (without secrets like access keys)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config: Option<S3PublicConfig>,
}

/// S3 config without secrets (for display purposes)
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct S3PublicConfig {
    /// Custom endpoint URL (for non-AWS S3-compatible services)
    pub endpoint: Option<String>,
    /// AWS region or custom region name
    pub region: String,
    /// Bucket name
    pub bucket: String,
}

/// S3-compatible backend configuration
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct S3Config {
    /// Custom endpoint URL (for non-AWS S3-compatible services)
    pub endpoint: Option<String>,
    /// AWS region or custom region name
    pub region: String,
    /// Bucket name
    pub bucket: String,
    /// Access key ID
    pub access_key_id: String,
    /// Secret access key
    pub secret_access_key: String,
    /// Session token for temporary credentials (e.g., Supabase S3 with user JWT for RLS)
    #[serde(default)]
    pub session_token: Option<String>,
    /// Use path-style URLs instead of virtual-hosted-style
    #[serde(default)]
    pub path_style: Option<bool>,
}

/// Request to add a new storage backend
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AddStorageBackendRequest {
    /// Display name for the backend
    pub name: String,
    /// Backend type (currently only "s3")
    pub r#type: String,
    /// Configuration (structure depends on type) - JSON string that will be parsed
    #[ts(type = "Record<string, unknown>")]
    pub config: serde_json::Value,
}

/// Request to update a storage backend
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct UpdateStorageBackendRequest {
    /// Backend ID to update
    pub backend_id: String,
    /// New display name (optional)
    pub name: Option<String>,
    /// New configuration (optional) - only provided fields are updated
    /// If credentials are omitted, existing credentials are preserved
    #[ts(type = "Record<string, unknown> | undefined")]
    pub config: Option<serde_json::Value>,
}

/// Request to upload data
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct StorageUploadRequest {
    /// Backend ID to upload to
    pub backend_id: String,
    /// Object key (path in the bucket)
    pub key: String,
    /// Data as base64-encoded string
    pub data: String,
}

/// Request to download data
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct StorageDownloadRequest {
    /// Backend ID to download from
    pub backend_id: String,
    /// Object key (path in the bucket)
    pub key: String,
}

/// Request to delete an object
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct StorageDeleteRequest {
    /// Backend ID
    pub backend_id: String,
    /// Object key (path in the bucket)
    pub key: String,
}

/// Request to list objects
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct StorageListRequest {
    /// Backend ID
    pub backend_id: String,
    /// Optional prefix to filter objects
    pub prefix: Option<String>,
}

/// Object info from list operation
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct StorageObjectInfo {
    /// Object key
    pub key: String,
    /// Size in bytes
    pub size: u64,
    /// Last modified timestamp (ISO 8601)
    pub last_modified: Option<String>,
}
