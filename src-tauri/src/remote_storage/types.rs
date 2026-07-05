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
    /// Row provenance: `"owned"` for locally-created backends, `"shared_from_space"`
    /// for entries that were replicated to this device as a member of a space.
    /// `None` on rows written before the origin_type migration (treat as `"owned"`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub origin_type: Option<String>,
    /// LIST | GET | PUT | DELETE bitmap. Only meaningful for `shared_from_space`
    /// rows — captures the access level the owner granted us.
    ///
    /// `#[ts(type = "number")]` overrides the default `bigint` ts-rs emits for
    /// `i64` — the flags only occupy 4 low bits, and the frontend helpers in
    /// `src/lib/storage/shareAccessFlags.ts` operate on plain numbers.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(type = "number | null")]
    pub share_access_flags: Option<i64>,
    /// Space id from `haex_shared_space_sync` — only populated for
    /// `shared_from_space` rows. Members render this in the list to answer
    /// "which space did this show up from?".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub space_id: Option<String>,
    /// Human-readable space name (joined from `haex_spaces.name`) so the
    /// frontend doesn't need a second lookup to build the "aus <space>" chip.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub space_name: Option<String>,
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
    /// Use path-style URLs instead of virtual-hosted-style
    #[serde(default)]
    pub path_style: Option<bool>,
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

/// Request a resumable streaming download into a local file.
///
/// `transfer_id` is a frontend-generated UUID so the WebView can correlate
/// `storage:transfer:*` events to a specific UI row and (optionally) cancel
/// the transfer via [`remote_storage_cancel_transfer`].
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct DownloadToPathRequest {
    /// Backend ID to download from
    pub backend_id: String,
    /// Object key (path in the bucket)
    pub key: String,
    /// Absolute path on disk where the object should land. If the file
    /// already exists, the download resumes from its current size.
    pub output_path: String,
    /// Caller-generated transfer id, used to route progress events and
    /// cancellation requests.
    pub transfer_id: String,
}

/// Request a streaming upload from a local file.
///
/// Mirrors [`DownloadToPathRequest`]. The backend reads `source_path` in
/// chunks and pushes to the configured remote; progress + cancellation
/// flow through `AppState.transfer_tokens` keyed by `transfer_id`, same
/// as the download path.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct UploadFromPathRequest {
    /// Backend ID to upload to
    pub backend_id: String,
    /// Object key (path in the bucket)
    pub key: String,
    /// Absolute path on disk of the file to upload.
    pub source_path: String,
    /// Caller-generated transfer id, used to route progress events and
    /// cancellation requests.
    pub transfer_id: String,
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

/// Directory-style listing result: a single hierarchy level under a prefix,
/// split into sub-prefixes ("folders") and objects ("files"). Built on top of
/// S3's `delimiter` parameter so very large buckets aren't enumerated to
/// browse a single folder.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct StorageListDirResponse {
    /// Sub-prefixes under the requested prefix (always end with "/").
    pub folders: Vec<String>,
    /// Objects whose key starts with the prefix and contains no further "/".
    pub objects: Vec<StorageObjectInfo>,
}
