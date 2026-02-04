//! HTTP client for sending files via LocalSend
//!
//! Implements the LocalSend v2.1 sender API:
//! - POST /api/localsend/v2/prepare-upload - Prepare to send files
//! - POST /api/localsend/v2/upload - Send file data
//! - POST /api/localsend/v2/cancel - Cancel a transfer

use reqwest::Client;
use std::collections::HashMap;
use std::path::Path;
use tokio::fs::File;
use tokio::io::AsyncReadExt;
use tauri::{AppHandle, Emitter, State};
use uuid::Uuid;

use super::error::LocalSendError;
use super::protocol::*;
use super::types::*;
use super::PROTOCOL_VERSION;
use crate::AppState;

// Use the same event names as server.rs for consistency
use super::server::{EVENT_TRANSFER_PROGRESS, EVENT_TRANSFER_COMPLETE, EVENT_TRANSFER_FAILED};

/// Send files to a device
pub async fn send_files(
    app_handle: AppHandle,
    state: State<'_, AppState>,
    device: Device,
    mut files: Vec<FileInfo>,
) -> Result<String, LocalSendError> {
    let device_info = state.localsend.device_info.read().await.clone();

    // Restore local_path from cache (since it's not serialized from frontend)
    {
        let prepared = state.localsend.prepared_files.read().await;
        for file in &mut files {
            if file.local_path.is_none() {
                if let Some(path) = prepared.get(&file.id) {
                    file.local_path = Some(path.clone());
                }
            }
        }
    }

    // Create HTTP client that accepts self-signed certificates
    let client = Client::builder()
        .danger_accept_invalid_certs(true)
        .build()
        .map_err(|e| LocalSendError::NetworkError(e.to_string()))?;

    // Build base URL
    let base_url = format!("{}://{}:{}", device.protocol, device.address, device.port);

    // Create our announcement
    let our_info = DeviceAnnouncement {
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

    // Convert files to protocol format
    let mut protocol_files: HashMap<String, PrepareUploadFile> = HashMap::new();
    for file in &files {
        protocol_files.insert(file.id.clone(), file.clone().into());
    }

    // Prepare upload request
    let prepare_request = PrepareUploadRequest {
        info: our_info,
        files: protocol_files,
    };

    // Send prepare-upload request
    let prepare_url = format!("{}/api/localsend/v2/prepare-upload", base_url);
    let response = client
        .post(&prepare_url)
        .json(&prepare_request)
        .send()
        .await?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();

        // Check for specific errors
        if status.as_u16() == 403 {
            if body.contains("InvalidPin") {
                return Err(LocalSendError::InvalidPin);
            }
            return Err(LocalSendError::TransferRejected);
        }

        return Err(LocalSendError::ProtocolError(format!(
            "prepare-upload failed: {} - {}",
            status, body
        )));
    }

    let prepare_response: PrepareUploadResponse = response.json().await?;
    let session_id = prepare_response.session_id.clone();

    // Create session for tracking
    let session = TransferSession {
        session_id: session_id.clone(),
        direction: TransferDirection::Outgoing,
        state: TransferState::InProgress,
        device: device.clone(),
        files: files.clone(),
        file_tokens: prepare_response.files.clone(),
        save_dir: None,
        pin: None,
        created_at: now_millis(),
        progress: HashMap::new(),
    };

    // Store session
    {
        let mut sessions = state.localsend.sessions.write().await;
        sessions.insert(session_id.clone(), session);
    }

    // Clone for async task
    let app_handle_clone = app_handle.clone();
    let session_id_clone = session_id.clone();
    let files_clone = files.clone();
    let file_tokens = prepare_response.files.clone();

    // Spawn upload task
    tokio::spawn(async move {
        let result = upload_files(
            &app_handle_clone,
            &client,
            &base_url,
            &session_id_clone,
            &files_clone,
            &file_tokens,
        )
        .await;

        match result {
            Ok(()) => {
                let _ = app_handle_clone.emit(EVENT_TRANSFER_COMPLETE, &session_id_clone);
            }
            Err(e) => {
                let _ = app_handle_clone.emit(
                    EVENT_TRANSFER_FAILED,
                    serde_json::json!({
                        "sessionId": session_id_clone,
                        "error": e.to_string()
                    }),
                );
            }
        }
    });

    Ok(session_id)
}

/// Upload files to the receiver
async fn upload_files(
    app_handle: &AppHandle,
    client: &Client,
    base_url: &str,
    session_id: &str,
    files: &[FileInfo],
    file_tokens: &HashMap<String, String>,
) -> Result<(), LocalSendError> {
    let upload_url = format!("{}/api/localsend/v2/upload", base_url);

    for file in files {
        // Get token for this file
        let token = file_tokens
            .get(&file.id)
            .ok_or_else(|| LocalSendError::ProtocolError(format!("No token for file {}", file.id)))?;

        // Get local path
        let local_path = file
            .local_path
            .as_ref()
            .ok_or_else(|| LocalSendError::InvalidFilePath("No local path".to_string()))?;

        // Read file
        let path = Path::new(local_path);
        if !path.exists() {
            return Err(LocalSendError::FileNotFound(local_path.clone()));
        }

        let mut file_handle = File::open(path).await?;
        let metadata = file_handle.metadata().await?;
        let total_size = metadata.len();

        // For now, read entire file (TODO: streaming upload with chunks)
        let mut file_data = Vec::new();
        file_handle.read_to_end(&mut file_data).await?;
        let mut bytes_sent: u64 = 0;

        // Build upload URL with query params
        let url = format!(
            "{}?sessionId={}&fileId={}&token={}",
            upload_url, session_id, file.id, token
        );

        // Send file
        let response = client
            .post(&url)
            .header("Content-Type", "application/octet-stream")
            .body(file_data.clone())
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(LocalSendError::TransferFailed(format!(
                "Upload failed: {} - {}",
                status, body
            )));
        }

        bytes_sent = total_size;

        // Emit progress
        let progress = TransferProgress {
            session_id: session_id.to_string(),
            file_id: file.id.clone(),
            file_name: file.file_name.clone(),
            bytes_transferred: bytes_sent,
            total_bytes: total_size,
            speed: 0, // TODO: calculate
        };
        let _ = app_handle.emit(EVENT_TRANSFER_PROGRESS, &progress);

        println!(
            "[LocalSend Client] Sent file: {} ({} bytes)",
            file.file_name, bytes_sent
        );
    }

    Ok(())
}

/// Cancel an outgoing transfer
pub async fn cancel_send(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<(), LocalSendError> {
    // Get session
    let session = {
        let sessions = state.localsend.sessions.read().await;
        sessions.get(&session_id).cloned()
    };

    let session = session.ok_or_else(|| LocalSendError::SessionNotFound(session_id.clone()))?;

    // Create client
    let client = Client::builder()
        .danger_accept_invalid_certs(true)
        .build()
        .map_err(|e| LocalSendError::NetworkError(e.to_string()))?;

    // Build cancel URL
    let base_url = format!(
        "{}://{}:{}",
        session.device.protocol, session.device.address, session.device.port
    );
    let cancel_url = format!(
        "{}/api/localsend/v2/cancel?sessionId={}",
        base_url, session_id
    );

    // Send cancel request
    let _ = client.post(&cancel_url).send().await;

    // Update session state
    {
        let mut sessions = state.localsend.sessions.write().await;
        if let Some(s) = sessions.get_mut(&session_id) {
            s.state = TransferState::Cancelled;
        }
    }

    Ok(())
}

/// Prepare files for sending (collect metadata)
pub async fn prepare_files_for_send(paths: Vec<String>) -> Result<Vec<FileInfo>, LocalSendError> {
    let mut files = Vec::new();

    for path_str in paths {
        let path = Path::new(&path_str);

        if !path.exists() {
            return Err(LocalSendError::FileNotFound(path_str));
        }

        if path.is_file() {
            let file_info = create_file_info(path, None).await?;
            files.push(file_info);
        } else if path.is_dir() {
            // Recursively collect files from directory
            collect_directory_files(path, path, &mut files).await?;
        }
    }

    Ok(files)
}

/// Create FileInfo from a path
async fn create_file_info(path: &Path, relative_to: Option<&Path>) -> Result<FileInfo, LocalSendError> {
    let metadata = tokio::fs::metadata(path).await?;

    let file_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown")
        .to_string();

    let file_type = mime_guess::from_path(path)
        .first_or_octet_stream()
        .to_string();

    let relative_path = relative_to.and_then(|base| {
        path.strip_prefix(base)
            .ok()
            .and_then(|p| p.to_str())
            .map(|s| s.to_string())
    });

    Ok(FileInfo {
        id: Uuid::new_v4().to_string(),
        file_name,
        size: metadata.len(),
        file_type,
        sha256: None, // TODO: calculate hash
        preview: None,
        relative_path,
        local_path: Some(path.to_string_lossy().to_string()),
    })
}

/// Recursively collect files from a directory
async fn collect_directory_files(
    base_path: &Path,
    current_path: &Path,
    files: &mut Vec<FileInfo>,
) -> Result<(), LocalSendError> {
    let mut entries = tokio::fs::read_dir(current_path).await?;

    while let Some(entry) = entries.next_entry().await? {
        let path = entry.path();

        if path.is_file() {
            let file_info = create_file_info(&path, Some(base_path)).await?;
            files.push(file_info);
        } else if path.is_dir() {
            // Recursively process subdirectory
            Box::pin(collect_directory_files(base_path, &path, files)).await?;
        }
    }

    Ok(())
}
