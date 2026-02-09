//! LocalSend protocol implementation for haex-vault
//!
//! This module implements the LocalSend v2.1 protocol for local file sharing.
//! See: https://github.com/localsend/protocol
//!
//! # Platform Support
//!
//! - **Desktop (Linux, macOS, Windows)**: Full support (auto-discovery via multicast, always-on server)
//! - **Mobile (Android, iOS)**: Full support (HTTP discovery, server only when app is in foreground)
//!
//! # Architecture
//!
//! - `protocol.rs` - LocalSend message types and API structures
//! - `types.rs` - Internal types (Device, Transfer, Session)
//! - `crypto.rs` - TLS certificate generation and fingerprinting
//! - `discovery.rs` - Multicast UDP device discovery (desktop only)
//! - `server.rs` - HTTPS server for receiving files (all platforms)
//! - `client.rs` - HTTP client for sending files (all platforms)
//! - `error.rs` - Error types

mod client;
mod crypto;
mod error;
mod protocol;
mod server;
mod types;

// Discovery: Multicast only on desktop (mobile uses HTTP scan)
#[cfg(not(any(target_os = "android", target_os = "ios")))]
mod discovery;

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

pub use error::LocalSendError;
pub use types::*;

#[cfg(any(target_os = "android", target_os = "ios"))]
use protocol::DeviceAnnouncement;
use tauri::{AppHandle, Manager, State};
use crate::AppState;

/// Default port for LocalSend (both HTTP and multicast)
pub const DEFAULT_PORT: u16 = 53317;

/// Multicast address for device discovery
pub const MULTICAST_ADDR: &str = "224.0.0.167";

/// Protocol version we implement
pub const PROTOCOL_VERSION: &str = "2.1";

/// Response from user for pending transfer (accept with save_dir, or reject)
pub type TransferResponse = Option<String>; // Some(save_dir) = accept, None = reject

/// LocalSend service state
pub struct LocalSendState {
    /// Our device info
    pub device_info: RwLock<DeviceInfo>,
    /// Discovered devices (fingerprint -> Device) - Arc for sharing with discovery task
    pub devices: Arc<RwLock<HashMap<String, Device>>>,
    /// Active transfer sessions (session_id -> TransferSession)
    pub sessions: RwLock<HashMap<String, TransferSession>>,
    /// TLS certificate and private key
    pub tls_identity: RwLock<Option<crypto::TlsIdentity>>,
    /// Server running state
    pub server_running: RwLock<bool>,
    /// Discovery running state
    pub discovery_running: RwLock<bool>,
    /// Shutdown signal for server
    pub server_shutdown: RwLock<Option<tokio::sync::oneshot::Sender<()>>>,
    /// Shutdown signal for discovery
    pub discovery_shutdown: RwLock<Option<tokio::sync::oneshot::Sender<()>>>,
    /// Settings
    pub settings: RwLock<LocalSendSettings>,
    /// Pending responses for transfer requests (session_id -> response channel)
    pub pending_responses: RwLock<HashMap<String, tokio::sync::oneshot::Sender<TransferResponse>>>,
    /// Registered extension ID for event routing (only one extension can handle LocalSend)
    pub registered_extension_id: RwLock<Option<String>>,
    /// Cache of prepared files (file_id -> local_path) for sending
    pub prepared_files: RwLock<HashMap<String, String>>,
}

impl LocalSendState {
    pub fn new() -> Self {
        Self {
            device_info: RwLock::new(DeviceInfo::default()),
            devices: Arc::new(RwLock::new(HashMap::new())),
            sessions: RwLock::new(HashMap::new()),
            tls_identity: RwLock::new(None),
            server_running: RwLock::new(false),
            discovery_running: RwLock::new(false),
            server_shutdown: RwLock::new(None),
            discovery_shutdown: RwLock::new(None),
            settings: RwLock::new(LocalSendSettings::default()),
            pending_responses: RwLock::new(HashMap::new()),
            registered_extension_id: RwLock::new(None),
            prepared_files: RwLock::new(HashMap::new()),
        }
    }

    /// Initialize TLS identity (generates certificate and fingerprint)
    pub async fn init_identity(&self) -> Result<(), LocalSendError> {
        let mut tls_guard = self.tls_identity.write().await;
        if tls_guard.is_none() {
            let identity = crypto::TlsIdentity::generate()?;
            let fingerprint = identity.fingerprint.clone();
            *tls_guard = Some(identity);

            // Update device info with fingerprint
            let mut device_info = self.device_info.write().await;
            device_info.fingerprint = fingerprint;
        }
        Ok(())
    }
}

impl Default for LocalSendState {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Tauri Commands - Desktop Only (Multicast Discovery)
// ============================================================================

/// Start device discovery via multicast UDP (desktop only)
#[cfg(not(any(target_os = "android", target_os = "ios")))]
#[tauri::command]
pub async fn localsend_start_discovery(
    app_handle: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), LocalSendError> {
    discovery::start_discovery(app_handle, state).await
}

/// Stop device discovery (desktop only)
#[cfg(not(any(target_os = "android", target_os = "ios")))]
#[tauri::command]
pub async fn localsend_stop_discovery(
    state: State<'_, AppState>,
) -> Result<(), LocalSendError> {
    discovery::stop_discovery(state).await
}

/// Get list of discovered devices (desktop - from multicast)
#[cfg(not(any(target_os = "android", target_os = "ios")))]
#[tauri::command]
pub async fn localsend_get_devices(
    state: State<'_, AppState>,
) -> Result<Vec<Device>, LocalSendError> {
    discovery::get_devices(state).await
}

// ============================================================================
// Tauri Commands - All Platforms (Server)
// ============================================================================

/// Start the HTTPS server for receiving files
#[tauri::command]
pub async fn localsend_start_server(
    app_handle: AppHandle,
    state: State<'_, AppState>,
    port: Option<u16>,
) -> Result<ServerInfo, LocalSendError> {
    server::start_server(app_handle, state, port).await
}

/// Stop the HTTPS server
#[tauri::command]
pub async fn localsend_stop_server(
    state: State<'_, AppState>,
) -> Result<(), LocalSendError> {
    server::stop_server(state).await
}

/// Get server status
#[tauri::command]
pub async fn localsend_get_server_status(
    state: State<'_, AppState>,
) -> Result<ServerStatus, LocalSendError> {
    server::get_server_status(state).await
}

/// Get pending incoming transfer requests
#[tauri::command]
pub async fn localsend_get_pending_transfers(
    state: State<'_, AppState>,
) -> Result<Vec<PendingTransfer>, LocalSendError> {
    server::get_pending_transfers(state).await
}

/// Accept an incoming transfer
#[tauri::command]
pub async fn localsend_accept_transfer(
    app_handle: AppHandle,
    state: State<'_, AppState>,
    session_id: String,
    save_dir: String,
) -> Result<(), LocalSendError> {
    server::accept_transfer(app_handle, state, session_id, save_dir).await
}

/// Reject an incoming transfer
#[tauri::command]
pub async fn localsend_reject_transfer(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<(), LocalSendError> {
    server::reject_transfer(state, session_id).await
}

// ============================================================================
// Tauri Commands - All Platforms (Client + Basic Discovery)
// ============================================================================

/// Send files to a device (all platforms)
#[tauri::command]
pub async fn localsend_send_files(
    app_handle: AppHandle,
    state: State<'_, AppState>,
    device: Device,
    files: Vec<FileInfo>,
) -> Result<String, LocalSendError> {
    client::send_files(app_handle, state, device, files).await
}

/// Cancel an outgoing transfer (all platforms)
#[tauri::command]
pub async fn localsend_cancel_send(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<(), LocalSendError> {
    client::cancel_send(state, session_id).await
}

/// Prepare files for sending - collect metadata (all platforms)
#[tauri::command]
pub async fn localsend_prepare_files(
    state: State<'_, AppState>,
    paths: Vec<String>,
) -> Result<Vec<FileInfo>, LocalSendError> {
    let files = client::prepare_files_for_send(paths).await?;

    // Cache local paths for later use in send_files
    {
        let mut prepared = state.localsend.prepared_files.write().await;
        for file in &files {
            if let Some(ref local_path) = file.local_path {
                prepared.insert(file.id.clone(), local_path.clone());
            }
        }
    }

    Ok(files)
}

/// Get current settings (all platforms)
#[tauri::command]
pub async fn localsend_get_settings(
    state: State<'_, AppState>,
) -> Result<LocalSendSettings, LocalSendError> {
    let settings = state.localsend.settings.read().await.clone();
    Ok(settings)
}

/// Update settings (all platforms)
#[tauri::command]
pub async fn localsend_set_settings(
    state: State<'_, AppState>,
    settings: LocalSendSettings,
) -> Result<(), LocalSendError> {
    *state.localsend.settings.write().await = settings;
    Ok(())
}

/// Get our device info (all platforms)
#[tauri::command]
pub async fn localsend_get_device_info(
    state: State<'_, AppState>,
) -> Result<DeviceInfo, LocalSendError> {
    let device_info = state.localsend.device_info.read().await.clone();
    Ok(device_info)
}

/// Set our device alias (all platforms)
#[tauri::command]
pub async fn localsend_set_alias(
    state: State<'_, AppState>,
    alias: String,
) -> Result<(), LocalSendError> {
    state.localsend.device_info.write().await.alias = alias;
    Ok(())
}

/// Get the default save directory for the current platform
pub fn get_default_save_directory(app_handle: &AppHandle) -> Option<String> {
    // Try download directory first (works on Desktop, may work on Android with permissions)
    if let Ok(download_dir) = app_handle.path().download_dir() {
        return Some(download_dir.to_string_lossy().to_string());
    }

    // Fallback to document directory
    if let Ok(doc_dir) = app_handle.path().document_dir() {
        return Some(doc_dir.to_string_lossy().to_string());
    }

    // Last resort: app data directory (always available, including Android)
    if let Ok(app_data_dir) = app_handle.path().app_data_dir() {
        let localsend_dir = app_data_dir.join("LocalSend");
        // Create the directory if it doesn't exist
        let _ = std::fs::create_dir_all(&localsend_dir);
        return Some(localsend_dir.to_string_lossy().to_string());
    }

    None
}

/// Initialize LocalSend (generate identity, etc.) - call on app start
#[tauri::command]
pub async fn localsend_init(
    app_handle: AppHandle,
    state: State<'_, AppState>,
) -> Result<DeviceInfo, LocalSendError> {
    state.localsend.init_identity().await?;

    // Set default save directory if not already set
    {
        let mut settings = state.localsend.settings.write().await;
        if settings.save_directory.is_none() {
            settings.save_directory = get_default_save_directory(&app_handle);
        }
    }

    let device_info = state.localsend.device_info.read().await.clone();
    Ok(device_info)
}

// ============================================================================
// Mobile-specific: HTTP-based discovery (scan network)
// ============================================================================

/// Scan for devices via HTTP (mobile fallback when multicast doesn't work)
#[cfg(any(target_os = "android", target_os = "ios"))]
#[tauri::command]
pub async fn localsend_scan_network(
    _app_handle: AppHandle,
    state: State<'_, AppState>,
) -> Result<Vec<Device>, LocalSendError> {
    // Get local IP to determine network range
    let local_ips = crypto::get_local_ip_addresses()?;

    if local_ips.is_empty() {
        return Ok(vec![]);
    }

    let device_info = state.localsend.device_info.read().await.clone();

    // Create our announcement for registration
    let our_info = DeviceAnnouncement {
        alias: device_info.alias.clone(),
        version: PROTOCOL_VERSION.to_string(),
        device_model: device_info.device_model.clone(),
        device_type: Some(device_info.device_type.clone()),
        fingerprint: device_info.fingerprint.clone(),
        port: device_info.port,
        protocol: device_info.protocol.clone(),
        download: device_info.download,
        announce: true,
    };

    let client = reqwest::Client::builder()
        .danger_accept_invalid_certs(true)
        .timeout(std::time::Duration::from_secs(2))
        .build()
        .map_err(|e| LocalSendError::NetworkError(e.to_string()))?;

    let mut devices = Vec::new();

    // Scan common local network range (e.g., 192.168.1.1-254)
    for local_ip in &local_ips {
        if let Some(base) = local_ip.rsplit_once('.') {
            let base_ip = base.0;

            // Scan in parallel
            let mut handles = Vec::new();

            for i in 1..=254 {
                let target_ip = format!("{}.{}", base_ip, i);

                // Skip our own IP
                if target_ip == *local_ip {
                    continue;
                }

                let client = client.clone();
                let our_info = our_info.clone();

                let handle = tokio::spawn(async move {
                    let url = format!("https://{}:{}/api/localsend/v2/register", target_ip, DEFAULT_PORT);

                    match client.post(&url).json(&our_info).send().await {
                        Ok(response) if response.status().is_success() => {
                            if let Ok(announcement) = response.json::<DeviceAnnouncement>().await {
                                Some(Device {
                                    alias: announcement.alias,
                                    version: announcement.version,
                                    device_model: announcement.device_model,
                                    device_type: announcement.device_type.unwrap_or(DeviceType::Desktop),
                                    fingerprint: announcement.fingerprint,
                                    address: target_ip,
                                    port: announcement.port,
                                    protocol: announcement.protocol,
                                    download: announcement.download,
                                    last_seen: now_millis(),
                                })
                            } else {
                                None
                            }
                        }
                        _ => None,
                    }
                });

                handles.push(handle);
            }

            // Collect results
            for handle in handles {
                if let Ok(Some(device)) = handle.await {
                    devices.push(device);
                }
            }
        }
    }

    // Store discovered devices
    {
        let mut devices_guard = state.localsend.devices.write().await;
        for device in &devices {
            devices_guard.insert(device.fingerprint.clone(), device.clone());
        }
    }

    Ok(devices)
}

/// Get cached devices (mobile - from last scan)
#[cfg(any(target_os = "android", target_os = "ios"))]
#[tauri::command]
pub async fn localsend_get_devices(
    state: State<'_, AppState>,
) -> Result<Vec<Device>, LocalSendError> {
    let devices = state.localsend.devices.read().await;
    Ok(devices.values().cloned().collect())
}
