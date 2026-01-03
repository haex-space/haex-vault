//! Internal types for LocalSend module

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use ts_rs::TS;

/// Device type classification (for UI icons)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[ts(export)]
#[serde(rename_all = "lowercase")]
pub enum DeviceType {
    Mobile,
    Desktop,
    Web,
    Headless,
    Server,
}

impl Default for DeviceType {
    fn default() -> Self {
        DeviceType::Desktop
    }
}

/// Our device information
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct DeviceInfo {
    /// Human-readable device name
    pub alias: String,
    /// Protocol version
    pub version: String,
    /// Device model (e.g., "Linux", "MacBook Pro")
    pub device_model: Option<String>,
    /// Device type for UI
    pub device_type: DeviceType,
    /// SHA-256 fingerprint of our TLS certificate
    pub fingerprint: String,
    /// Port we're listening on
    pub port: u16,
    /// Protocol (http or https)
    pub protocol: String,
    /// Whether we support download mode (browser mode)
    pub download: bool,
}

impl Default for DeviceInfo {
    fn default() -> Self {
        Self {
            alias: whoami::devicename().unwrap_or_else(|_| "Unknown".to_string()),
            version: crate::localsend::PROTOCOL_VERSION.to_string(),
            device_model: Some(std::env::consts::OS.to_string()),
            device_type: DeviceType::Desktop,
            fingerprint: String::new(),
            port: crate::localsend::DEFAULT_PORT,
            protocol: "https".to_string(),
            download: false,
        }
    }
}

/// A discovered remote device
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct Device {
    /// Human-readable device name
    pub alias: String,
    /// Protocol version
    pub version: String,
    /// Device model
    pub device_model: Option<String>,
    /// Device type for UI
    pub device_type: DeviceType,
    /// SHA-256 fingerprint of device's TLS certificate
    pub fingerprint: String,
    /// IP address
    pub address: String,
    /// Port
    pub port: u16,
    /// Protocol (http or https)
    pub protocol: String,
    /// Whether device supports download mode
    pub download: bool,
    /// Last seen timestamp (Unix millis)
    pub last_seen: u64,
}

/// File metadata for transfer
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct FileInfo {
    /// Unique file ID within the transfer
    pub id: String,
    /// File name
    pub file_name: String,
    /// File size in bytes
    pub size: u64,
    /// MIME type
    pub file_type: String,
    /// SHA-256 hash (optional, for verification)
    pub sha256: Option<String>,
    /// Base64 preview thumbnail (optional)
    pub preview: Option<String>,
    /// Relative path for folders (e.g., "folder/subfolder/file.txt")
    pub relative_path: Option<String>,
    /// Absolute path on local filesystem (for sending)
    #[serde(skip_serializing)]
    #[ts(skip)]
    pub local_path: Option<String>,
}

/// Transfer session state
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[ts(export)]
#[serde(rename_all = "lowercase")]
pub enum TransferState {
    /// Waiting for user to accept/reject
    Pending,
    /// Transfer accepted, in progress
    InProgress,
    /// Transfer completed successfully
    Completed,
    /// Transfer was rejected
    Rejected,
    /// Transfer was cancelled
    Cancelled,
    /// Transfer failed
    Failed,
}

/// Transfer direction
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[ts(export)]
#[serde(rename_all = "lowercase")]
pub enum TransferDirection {
    /// We are receiving files
    Incoming,
    /// We are sending files
    Outgoing,
}

/// A transfer session
#[derive(Debug, Clone)]
pub struct TransferSession {
    /// Unique session ID
    pub session_id: String,
    /// Transfer direction
    pub direction: TransferDirection,
    /// Current state
    pub state: TransferState,
    /// Remote device
    pub device: Device,
    /// Files in this transfer
    pub files: Vec<FileInfo>,
    /// Per-file tokens (for incoming transfers)
    pub file_tokens: HashMap<String, String>,
    /// Directory to save files (for incoming transfers)
    pub save_dir: Option<String>,
    /// PIN required for this transfer
    pub pin: Option<String>,
    /// Created timestamp (Unix millis)
    pub created_at: u64,
    /// Progress per file (file_id -> bytes transferred)
    pub progress: HashMap<String, u64>,
}

/// Pending transfer request (for UI)
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct PendingTransfer {
    /// Session ID
    pub session_id: String,
    /// Sender device info
    pub sender: Device,
    /// Files to be received
    pub files: Vec<FileInfo>,
    /// Total size in bytes
    pub total_size: u64,
    /// Whether PIN is required
    pub pin_required: bool,
    /// Created timestamp (Unix millis)
    pub created_at: u64,
}

/// Transfer progress update (for events)
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct TransferProgress {
    /// Session ID
    pub session_id: String,
    /// File ID
    pub file_id: String,
    /// File name
    pub file_name: String,
    /// Bytes transferred
    pub bytes_transferred: u64,
    /// Total bytes
    pub total_bytes: u64,
    /// Transfer speed in bytes/sec
    pub speed: u64,
}

/// Server status information
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct ServerStatus {
    /// Whether server is running
    pub running: bool,
    /// Port server is listening on
    pub port: Option<u16>,
    /// Our fingerprint
    pub fingerprint: Option<String>,
    /// Local IP addresses
    pub addresses: Vec<String>,
}

/// Server info returned when starting
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct ServerInfo {
    /// Port server is listening on
    pub port: u16,
    /// Our fingerprint
    pub fingerprint: String,
    /// Local IP addresses
    pub addresses: Vec<String>,
}

/// LocalSend settings
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct LocalSendSettings {
    /// Device alias
    pub alias: String,
    /// Port to use
    pub port: u16,
    /// Auto-accept transfers from known devices
    pub auto_accept: bool,
    /// Default save directory
    pub save_directory: Option<String>,
    /// Require PIN for incoming transfers
    pub require_pin: bool,
    /// PIN (if require_pin is true)
    pub pin: Option<String>,
    /// Show notification on incoming transfer
    pub show_notifications: bool,
}

impl Default for LocalSendSettings {
    fn default() -> Self {
        Self {
            alias: whoami::devicename().unwrap_or_else(|_| "Unknown".to_string()),
            port: crate::localsend::DEFAULT_PORT,
            auto_accept: false,
            save_directory: None,
            require_pin: false,
            pin: None,
            show_notifications: true,
        }
    }
}

/// Helper to get current timestamp in milliseconds
pub fn now_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
