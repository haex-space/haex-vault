//! LocalSend Protocol v2.1 message types
//!
//! This module defines the wire format for LocalSend API requests and responses.
//! See: https://github.com/localsend/protocol

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::types::{DeviceType, FileInfo};

// ============================================================================
// Discovery Messages (Multicast UDP + HTTP)
// ============================================================================

/// Device announcement message (sent via multicast or HTTP POST /register)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceAnnouncement {
    /// Device alias (human-readable name)
    pub alias: String,
    /// Protocol version (e.g., "2.1")
    pub version: String,
    /// Device model (e.g., "Linux", "MacBook Pro")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device_model: Option<String>,
    /// Device type for UI icons
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device_type: Option<DeviceType>,
    /// SHA-256 fingerprint of TLS certificate (or random string if HTTP)
    pub fingerprint: String,
    /// Port the device is listening on
    pub port: u16,
    /// Protocol: "http" or "https"
    pub protocol: String,
    /// Whether this device supports download mode (browser mode)
    #[serde(default)]
    pub download: bool,
    /// True if this is an announcement, false if it's a response
    #[serde(default)]
    pub announce: bool,
}

// ============================================================================
// File Transfer Messages
// ============================================================================

/// File metadata in prepare-upload request
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrepareUploadFile {
    /// Unique file ID
    pub id: String,
    /// File name
    pub file_name: String,
    /// File size in bytes
    pub size: u64,
    /// MIME type
    pub file_type: String,
    /// SHA-256 hash (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
    /// Base64 preview thumbnail (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preview: Option<String>,
    /// Metadata (e.g., for folders: modified timestamp)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<FileMetadata>,
}

/// File metadata (optional)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileMetadata {
    /// Last modified timestamp (ISO 8601)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modified: Option<String>,
    /// File permissions (Unix mode)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub permissions: Option<String>,
}

/// Request body for POST /api/localsend/v2/prepare-upload
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrepareUploadRequest {
    /// Sender device info
    pub info: DeviceAnnouncement,
    /// Files to upload (file_id -> file info)
    pub files: HashMap<String, PrepareUploadFile>,
}

/// Response body for POST /api/localsend/v2/prepare-upload
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrepareUploadResponse {
    /// Session ID for this transfer
    pub session_id: String,
    /// Tokens for each file (file_id -> token)
    /// If a file is skipped (already exists), it won't have a token
    pub files: HashMap<String, String>,
}

/// Error response for prepare-upload
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrepareUploadError {
    /// Error code
    pub code: PrepareUploadErrorCode,
    /// Optional message
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// Error codes for prepare-upload
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PrepareUploadErrorCode {
    /// Transfer was blocked/rejected
    Blocked,
    /// Invalid PIN
    InvalidPin,
    /// Too many requests
    TooManyRequests,
}

/// Request body for POST /api/localsend/v2/prepare-download (browser mode)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrepareDownloadRequest {
    /// Receiver device info
    pub info: DeviceAnnouncement,
}

/// Response body for POST /api/localsend/v2/prepare-download
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrepareDownloadResponse {
    /// Sender device info
    pub info: DeviceAnnouncement,
    /// Session ID for this transfer
    pub session_id: String,
    /// Files available for download (file_id -> file info)
    pub files: HashMap<String, PrepareUploadFile>,
}

// ============================================================================
// Upload Query Parameters
// ============================================================================

/// Query parameters for POST /api/localsend/v2/upload
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UploadQuery {
    /// Session ID from prepare-upload
    pub session_id: String,
    /// File ID
    pub file_id: String,
    /// Token for this file
    pub token: String,
}

/// Query parameters for POST /api/localsend/v2/cancel
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CancelQuery {
    /// Session ID to cancel
    pub session_id: String,
}

/// Query parameters for GET /api/localsend/v2/download
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadQuery {
    /// Session ID from prepare-download
    pub session_id: String,
    /// File ID to download
    pub file_id: String,
}

// ============================================================================
// Conversion helpers
// ============================================================================

impl From<FileInfo> for PrepareUploadFile {
    fn from(f: FileInfo) -> Self {
        Self {
            id: f.id,
            file_name: f.file_name,
            size: f.size,
            file_type: f.file_type,
            sha256: f.sha256,
            preview: f.preview,
            metadata: None,
        }
    }
}

impl From<PrepareUploadFile> for FileInfo {
    fn from(f: PrepareUploadFile) -> Self {
        Self {
            id: f.id,
            file_name: f.file_name,
            size: f.size,
            file_type: f.file_type,
            sha256: f.sha256,
            preview: f.preview,
            relative_path: None,
            local_path: None,
        }
    }
}
