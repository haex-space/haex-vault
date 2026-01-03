//! HTTPS server for receiving LocalSend file transfers
//!
//! Implements the LocalSend v2.1 receiver API:
//! - POST /api/localsend/v2/register - Device registration (HTTP discovery fallback)
//! - GET /api/localsend/v2/info - Device info
//! - POST /api/localsend/v2/prepare-upload - Prepare to receive files
//! - POST /api/localsend/v2/upload - Receive file data
//! - POST /api/localsend/v2/cancel - Cancel a transfer

use axum::{
    body::Body,
    extract::{ConnectInfo, Query, State as AxumState},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use axum_server::tls_rustls::RustlsConfig;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::fs::File;
use tokio::io::AsyncWriteExt;
use tokio::sync::oneshot;
use tauri::{AppHandle, Emitter, Manager, State};
use uuid::Uuid;

use super::crypto::{get_local_ip_addresses, TlsIdentity};
use super::discovery::register_device;
use super::error::LocalSendError;
use super::protocol::*;
use super::types::*;
use super::{LocalSendState, DEFAULT_PORT, PROTOCOL_VERSION};
use crate::AppState;

/// Event names for transfers
pub const EVENT_TRANSFER_REQUEST: &str = "localsend:transfer-request";
pub const EVENT_TRANSFER_PROGRESS: &str = "localsend:transfer-progress";
pub const EVENT_TRANSFER_COMPLETE: &str = "localsend:transfer-complete";
pub const EVENT_TRANSFER_FAILED: &str = "localsend:transfer-failed";

/// Shared state for the Axum server
struct ServerState {
    app_handle: AppHandle,
    localsend: Arc<LocalSendState>,
}

/// Start the HTTPS server
pub async fn start_server(
    app_handle: AppHandle,
    state: State<'_, AppState>,
    port: Option<u16>,
) -> Result<ServerInfo, LocalSendError> {
    // Install the ring crypto provider for rustls (must be done before TLS config)
    // This is idempotent - calling it multiple times is safe
    let _ = rustls::crypto::ring::default_provider().install_default();

    // Check if already running
    if *state.localsend.server_running.read().await {
        return Err(LocalSendError::ServerAlreadyRunning);
    }

    let port = port.unwrap_or(DEFAULT_PORT);

    // Generate TLS identity if not exists
    let identity = {
        let mut tls_guard = state.localsend.tls_identity.write().await;
        if tls_guard.is_none() {
            let new_identity = TlsIdentity::generate()?;
            *tls_guard = Some(new_identity);
        }
        tls_guard.clone().unwrap()
    };

    // Update device info with fingerprint
    {
        let mut device_info = state.localsend.device_info.write().await;
        device_info.fingerprint = identity.fingerprint.clone();
        device_info.port = port;
    }

    // Create shutdown channel
    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
    *state.localsend.server_shutdown.write().await = Some(shutdown_tx);
    *state.localsend.server_running.write().await = true;

    // Get local addresses
    let addresses = get_local_ip_addresses().unwrap_or_default();

    let server_info = ServerInfo {
        port,
        fingerprint: identity.fingerprint.clone(),
        addresses: addresses.clone(),
    };

    // Share the same state with the Axum server
    let server_state = Arc::new(ServerState {
        app_handle: app_handle.clone(),
        localsend: state.localsend.clone(),
    });

    // Create router
    let app = Router::new()
        .route("/api/localsend/v2/info", get(handle_info))
        .route("/api/localsend/v2/register", post(handle_register))
        .route("/api/localsend/v2/prepare-upload", post(handle_prepare_upload))
        .route("/api/localsend/v2/upload", post(handle_upload))
        .route("/api/localsend/v2/cancel", post(handle_cancel))
        .with_state(server_state);

    // Configure TLS
    let tls_config = RustlsConfig::from_pem(
        identity.cert_pem.as_bytes().to_vec(),
        identity.key_pem.as_bytes().to_vec(),
    )
    .await
    .map_err(|e| LocalSendError::TlsError(format!("Failed to configure TLS: {e}")))?;

    // Spawn server task
    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    tokio::spawn(async move {
        println!("[LocalSend Server] Starting on port {}", port);

        let server = axum_server::bind_rustls(addr, tls_config)
            .serve(app.into_make_service_with_connect_info::<SocketAddr>());

        tokio::select! {
            result = server => {
                if let Err(e) = result {
                    eprintln!("[LocalSend Server] Error: {}", e);
                }
            }
            _ = shutdown_rx => {
                println!("[LocalSend Server] Shutting down");
            }
        }
    });

    Ok(server_info)
}

/// Stop the HTTPS server
pub async fn stop_server(state: State<'_, AppState>) -> Result<(), LocalSendError> {
    if !*state.localsend.server_running.read().await {
        return Err(LocalSendError::ServerNotRunning);
    }

    // Send shutdown signal
    if let Some(tx) = state.localsend.server_shutdown.write().await.take() {
        let _ = tx.send(());
    }

    *state.localsend.server_running.write().await = false;

    Ok(())
}

/// Get server status
pub async fn get_server_status(state: State<'_, AppState>) -> Result<ServerStatus, LocalSendError> {
    let running = *state.localsend.server_running.read().await;

    if running {
        let device_info = state.localsend.device_info.read().await;
        let addresses = get_local_ip_addresses().unwrap_or_default();

        Ok(ServerStatus {
            running: true,
            port: Some(device_info.port),
            fingerprint: Some(device_info.fingerprint.clone()),
            addresses,
        })
    } else {
        Ok(ServerStatus {
            running: false,
            port: None,
            fingerprint: None,
            addresses: vec![],
        })
    }
}

/// Get pending incoming transfer requests
pub async fn get_pending_transfers(
    state: State<'_, AppState>,
) -> Result<Vec<PendingTransfer>, LocalSendError> {
    let sessions = state.localsend.sessions.read().await;

    let pending: Vec<PendingTransfer> = sessions
        .values()
        .filter(|s| s.state == TransferState::Pending && s.direction == TransferDirection::Incoming)
        .map(|s| PendingTransfer {
            session_id: s.session_id.clone(),
            sender: s.device.clone(),
            files: s.files.clone(),
            total_size: s.files.iter().map(|f| f.size).sum(),
            pin_required: s.pin.is_some(),
            created_at: s.created_at,
        })
        .collect();

    Ok(pending)
}

/// Accept an incoming transfer
pub async fn accept_transfer(
    _app_handle: AppHandle,
    state: State<'_, AppState>,
    session_id: String,
    save_dir: String,
) -> Result<(), LocalSendError> {
    let mut sessions = state.localsend.sessions.write().await;

    let session = sessions
        .get_mut(&session_id)
        .ok_or_else(|| LocalSendError::SessionNotFound(session_id.clone()))?;

    if session.state != TransferState::Pending {
        return Err(LocalSendError::ProtocolError(format!(
            "Session {} is not pending",
            session_id
        )));
    }

    session.state = TransferState::InProgress;
    session.save_dir = Some(save_dir);

    Ok(())
}

/// Reject an incoming transfer
pub async fn reject_transfer(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<(), LocalSendError> {
    let mut sessions = state.localsend.sessions.write().await;

    let session = sessions
        .get_mut(&session_id)
        .ok_or_else(|| LocalSendError::SessionNotFound(session_id.clone()))?;

    session.state = TransferState::Rejected;

    Ok(())
}

// ============================================================================
// Axum Handlers
// ============================================================================

/// GET /api/localsend/v2/info
async fn handle_info(
    AxumState(state): AxumState<Arc<ServerState>>,
) -> impl IntoResponse {
    let device_info = state.localsend.device_info.read().await;

    let response = DeviceAnnouncement {
        alias: device_info.alias.clone(),
        version: PROTOCOL_VERSION.to_string(),
        device_model: device_info.device_model.clone(),
        device_type: Some(device_info.device_type.clone()),
        fingerprint: device_info.fingerprint.clone(),
        port: device_info.port,
        protocol: device_info.protocol.clone(),
        download: device_info.download,
        announce: false,
    };

    Json(response)
}

/// POST /api/localsend/v2/register
async fn handle_register(
    AxumState(state): AxumState<Arc<ServerState>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Json(announcement): Json<DeviceAnnouncement>,
) -> impl IntoResponse {
    // Register the device
    let _ = register_device(
        &state.app_handle,
        &state.localsend.devices,
        announcement.clone(),
        addr.ip().to_string(),
    )
    .await;

    // Return our device info
    let device_info = state.localsend.device_info.read().await;

    let response = DeviceAnnouncement {
        alias: device_info.alias.clone(),
        version: PROTOCOL_VERSION.to_string(),
        device_model: device_info.device_model.clone(),
        device_type: Some(device_info.device_type.clone()),
        fingerprint: device_info.fingerprint.clone(),
        port: device_info.port,
        protocol: device_info.protocol.clone(),
        download: device_info.download,
        announce: false,
    };

    Json(response)
}

/// POST /api/localsend/v2/prepare-upload
async fn handle_prepare_upload(
    AxumState(state): AxumState<Arc<ServerState>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Query(params): Query<HashMap<String, String>>,
    Json(request): Json<PrepareUploadRequest>,
) -> Response {
    let settings = state.localsend.settings.read().await;

    // Check PIN if required
    if settings.require_pin {
        let provided_pin = params.get("pin").map(|s| s.as_str()).unwrap_or("");
        let expected_pin = settings.pin.as_deref().unwrap_or("");

        if provided_pin != expected_pin {
            return (
                StatusCode::FORBIDDEN,
                Json(PrepareUploadError {
                    code: PrepareUploadErrorCode::InvalidPin,
                    message: Some("Invalid PIN".to_string()),
                }),
            )
                .into_response();
        }
    }

    // Create session
    let session_id = Uuid::new_v4().to_string();
    let mut file_tokens = HashMap::new();

    // Generate tokens for each file
    for file_id in request.files.keys() {
        let token = Uuid::new_v4().to_string();
        file_tokens.insert(file_id.clone(), token);
    }

    // Create sender device from announcement
    let sender = Device {
        alias: request.info.alias.clone(),
        version: request.info.version.clone(),
        device_model: request.info.device_model.clone(),
        device_type: request.info.device_type.clone().unwrap_or(DeviceType::Desktop),
        fingerprint: request.info.fingerprint.clone(),
        address: addr.ip().to_string(),
        port: request.info.port,
        protocol: request.info.protocol.clone(),
        download: request.info.download,
        last_seen: now_millis(),
    };

    // Convert files
    let files: Vec<FileInfo> = request
        .files
        .into_values()
        .map(|f| f.into())
        .collect();

    // Create session
    let session = TransferSession {
        session_id: session_id.clone(),
        direction: TransferDirection::Incoming,
        state: TransferState::Pending,
        device: sender.clone(),
        files: files.clone(),
        file_tokens: file_tokens.clone(),
        save_dir: None,
        pin: if settings.require_pin {
            settings.pin.clone()
        } else {
            None
        },
        created_at: now_millis(),
        progress: HashMap::new(),
    };

    // Store session
    {
        let mut sessions = state.localsend.sessions.write().await;
        sessions.insert(session_id.clone(), session);
    }

    // Emit event for pending transfer
    let pending = PendingTransfer {
        session_id: session_id.clone(),
        sender,
        files,
        total_size: 0, // TODO: calculate
        pin_required: settings.require_pin,
        created_at: now_millis(),
    };

    let _ = state.app_handle.emit(EVENT_TRANSFER_REQUEST, &pending);

    // Return response
    let response = PrepareUploadResponse {
        session_id,
        files: file_tokens,
    };

    Json(response).into_response()
}

/// POST /api/localsend/v2/upload
async fn handle_upload(
    AxumState(state): AxumState<Arc<ServerState>>,
    Query(params): Query<UploadQuery>,
    body: Body,
) -> Response {
    // Find session
    let session = {
        let sessions = state.localsend.sessions.read().await;
        sessions.get(&params.session_id).cloned()
    };

    let session = match session {
        Some(s) => s,
        None => {
            return (StatusCode::NOT_FOUND, "Session not found").into_response();
        }
    };

    // Check session state
    if session.state == TransferState::Rejected {
        return (StatusCode::FORBIDDEN, "Transfer rejected").into_response();
    }

    if session.state == TransferState::Pending {
        // Wait for acceptance or reject
        // For now, reject pending transfers
        return (StatusCode::ACCEPTED, "Waiting for acceptance").into_response();
    }

    // Verify token
    let expected_token = session.file_tokens.get(&params.file_id);
    if expected_token != Some(&params.token) {
        return (StatusCode::FORBIDDEN, "Invalid token").into_response();
    }

    // Find file info
    let file_info = session.files.iter().find(|f| f.id == params.file_id);
    let file_info = match file_info {
        Some(f) => f,
        None => {
            return (StatusCode::NOT_FOUND, "File not found").into_response();
        }
    };

    // Determine save path
    let default_dir = ".".to_string();
    let save_dir = session.save_dir.as_ref().unwrap_or(&default_dir);
    let mut save_path = PathBuf::from(save_dir);

    // Handle relative path for folders
    if let Some(ref rel_path) = file_info.relative_path {
        save_path.push(rel_path);
    } else {
        save_path.push(&file_info.file_name);
    }

    // Create parent directories
    if let Some(parent) = save_path.parent() {
        if let Err(e) = tokio::fs::create_dir_all(parent).await {
            eprintln!("[LocalSend Server] Failed to create directory: {}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, "Failed to create directory").into_response();
        }
    }

    // Write file
    let file = match File::create(&save_path).await {
        Ok(f) => f,
        Err(e) => {
            eprintln!("[LocalSend Server] Failed to create file: {}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, "Failed to create file").into_response();
        }
    };

    let mut file = tokio::io::BufWriter::new(file);
    let mut bytes_written: u64 = 0;

    // Stream body to file
    use futures_util::StreamExt;
    let mut stream = body.into_data_stream();

    while let Some(chunk) = stream.next().await {
        match chunk {
            Ok(data) => {
                if let Err(e) = file.write_all(&data).await {
                    eprintln!("[LocalSend Server] Failed to write file: {}", e);
                    return (StatusCode::INTERNAL_SERVER_ERROR, "Failed to write file").into_response();
                }
                bytes_written += data.len() as u64;

                // Emit progress
                let progress = TransferProgress {
                    session_id: params.session_id.clone(),
                    file_id: params.file_id.clone(),
                    file_name: file_info.file_name.clone(),
                    bytes_transferred: bytes_written,
                    total_bytes: file_info.size,
                    speed: 0, // TODO: calculate
                };
                let _ = state.app_handle.emit(EVENT_TRANSFER_PROGRESS, &progress);
            }
            Err(e) => {
                eprintln!("[LocalSend Server] Failed to read body: {}", e);
                return (StatusCode::INTERNAL_SERVER_ERROR, "Failed to read body").into_response();
            }
        }
    }

    // Flush file
    if let Err(e) = file.flush().await {
        eprintln!("[LocalSend Server] Failed to flush file: {}", e);
        return (StatusCode::INTERNAL_SERVER_ERROR, "Failed to flush file").into_response();
    }

    println!(
        "[LocalSend Server] Received file: {} ({} bytes)",
        file_info.file_name, bytes_written
    );

    StatusCode::OK.into_response()
}

/// POST /api/localsend/v2/cancel
async fn handle_cancel(
    AxumState(state): AxumState<Arc<ServerState>>,
    Query(params): Query<CancelQuery>,
) -> Response {
    let mut sessions = state.localsend.sessions.write().await;

    if let Some(session) = sessions.get_mut(&params.session_id) {
        session.state = TransferState::Cancelled;
        let _ = state.app_handle.emit(
            EVENT_TRANSFER_FAILED,
            serde_json::json!({
                "sessionId": params.session_id,
                "error": "Cancelled by sender"
            }),
        );
    }

    StatusCode::OK.into_response()
}
