//! WebSocket server for external bridge
//!
//! Handles incoming connections from external clients (browser extensions,
//! CLI tools, servers, etc.) and routes requests to haex-vault extensions.

use crate::AppState;
use crate::database::core::{execute_with_crdt, select_with_crdt};
use crate::event_names::EVENT_EXTENSION_AUTO_START_REQUEST;
use futures_util::{SinkExt, StreamExt};
use serde_json::Value as JsonValue;
use std::collections::HashMap;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager};
use tokio::net::{TcpListener, TcpStream};
use std::time::Duration;
use tokio::sync::{mpsc, oneshot, RwLock};
use tokio_tungstenite::{accept_async, tungstenite::Message};

use super::authorization::{
    PendingAuthorization, SQL_GET_CLIENT_EXTENSION, SQL_GET_EXTENSION_ID_BY_PUBLIC_KEY_AND_NAME,
    SQL_IS_CLIENT_AUTHORIZED_FOR_EXTENSION, SQL_IS_CLIENT_KNOWN, SQL_UPDATE_LAST_SEEN,
};
use super::crypto::{ServerKeyPair, create_encrypted_response};
use super::error::BridgeError;
use super::protocol::{HandshakeResponse, ProtocolMessage};

/// Default port for the external bridge WebSocket server
pub const DEFAULT_BRIDGE_PORT: u16 = 19455;
const PROTOCOL_VERSION: u32 = 1;
/// Default timeout for extension responses (can be overridden per extension)
const DEFAULT_REQUEST_TIMEOUT_SECS: u64 = 30;

/// Type alias for pending response senders
type ResponseSender = oneshot::Sender<serde_json::Value>;

/// Connected client state
struct ConnectedClient {
    client_id: String,
    client_name: String,
    public_key: String,
    authorized: bool,
    extension_id: Option<String>,
    tx: mpsc::UnboundedSender<Message>,
}

/// Session authorization entry (for "allow once" authorizations)
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, rename_all = "camelCase")]
#[serde(rename_all = "camelCase")]
pub struct SessionAuthorization {
    /// Unique client identifier (public key fingerprint)
    pub client_id: String,
    /// Extension ID this client can access
    pub extension_id: String,
}

/// External Bridge WebSocket Server
pub struct ExternalBridge {
    running: bool,
    current_port: u16,
    shutdown_tx: Option<mpsc::Sender<()>>,
    clients: Arc<RwLock<HashMap<String, ConnectedClient>>>,
    pending_authorizations: Arc<RwLock<HashMap<String, PendingAuthorization>>>,
    server_keypair: Arc<RwLock<Option<ServerKeyPair>>>,
    /// Pending responses waiting for extension callbacks (requestId → sender)
    pending_responses: Arc<RwLock<HashMap<String, ResponseSender>>>,
    /// Session-based authorizations (for "allow once" - cleared when server stops)
    /// Key: client_id, Value: SessionAuthorization
    session_authorizations: Arc<RwLock<HashMap<String, SessionAuthorization>>>,
}

impl Default for ExternalBridge {
    fn default() -> Self {
        Self::new()
    }
}

impl ExternalBridge {
    pub fn new() -> Self {
        Self {
            running: false,
            current_port: DEFAULT_BRIDGE_PORT,
            shutdown_tx: None,
            clients: Arc::new(RwLock::new(HashMap::new())),
            pending_authorizations: Arc::new(RwLock::new(HashMap::new())),
            server_keypair: Arc::new(RwLock::new(None)),
            pending_responses: Arc::new(RwLock::new(HashMap::new())),
            session_authorizations: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Get a clone of the pending_responses map for use in Tauri commands
    pub fn get_pending_responses(&self) -> Arc<RwLock<HashMap<String, ResponseSender>>> {
        self.pending_responses.clone()
    }

    /// Get a clone of the session_authorizations map for use in Tauri commands
    pub fn get_session_authorizations(&self) -> Arc<RwLock<HashMap<String, SessionAuthorization>>> {
        self.session_authorizations.clone()
    }

    /// Add a session authorization (for "allow once")
    pub async fn add_session_authorization(&self, client_id: &str, extension_id: &str) {
        let mut authorizations = self.session_authorizations.write().await;
        authorizations.insert(
            client_id.to_string(),
            SessionAuthorization {
                client_id: client_id.to_string(),
                extension_id: extension_id.to_string(),
            },
        );
        println!(
            "[ExternalBridge] Added session authorization for client {} -> extension {}",
            client_id, extension_id
        );
    }

    /// Check if a client has a session authorization
    pub async fn get_session_authorization(&self, client_id: &str) -> Option<SessionAuthorization> {
        let authorizations = self.session_authorizations.read().await;
        authorizations.get(client_id).cloned()
    }

    pub fn is_running(&self) -> bool {
        self.running
    }

    /// Get the current port the server is running on (or will run on)
    pub fn get_port(&self) -> u16 {
        self.current_port
    }

    /// Start the WebSocket server on the specified port
    pub async fn start(&mut self, app_handle: AppHandle, port: Option<u16>) -> Result<(), BridgeError> {
        if self.running {
            return Err(BridgeError::AlreadyRunning);
        }

        let port = port.unwrap_or(DEFAULT_BRIDGE_PORT);
        self.current_port = port;

        // Generate server keypair
        {
            let mut keypair = self.server_keypair.write().await;
            *keypair = Some(ServerKeyPair::generate());
        }

        let (shutdown_tx, mut shutdown_rx) = mpsc::channel::<()>(1);
        self.shutdown_tx = Some(shutdown_tx);

        let addr = format!("127.0.0.1:{}", port);
        let listener = TcpListener::bind(&addr).await?;

        println!("[ExternalBridge] WebSocket server listening on {}", addr);

        let clients = self.clients.clone();
        let pending = self.pending_authorizations.clone();
        let server_keypair = self.server_keypair.clone();
        let pending_responses = self.pending_responses.clone();
        let session_authorizations = self.session_authorizations.clone();

        // Spawn the server task
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    result = listener.accept() => {
                        match result {
                            Ok((stream, addr)) => {
                                println!("[ExternalBridge] New connection from {}", addr);
                                let app = app_handle.clone();
                                let clients = clients.clone();
                                let pending = pending.clone();
                                let keypair = server_keypair.clone();
                                let pending_resp = pending_responses.clone();
                                let session_auths = session_authorizations.clone();

                                tokio::spawn(async move {
                                    if let Err(e) = handle_connection(stream, app, clients, pending, keypair, pending_resp, session_auths).await {
                                        eprintln!("[ExternalBridge] Connection error: {}", e);
                                    }
                                });
                            }
                            Err(e) => {
                                eprintln!("[ExternalBridge] Accept error: {}", e);
                            }
                        }
                    }
                    _ = shutdown_rx.recv() => {
                        println!("[ExternalBridge] Shutdown signal received");
                        break;
                    }
                }
            }
        });

        self.running = true;
        Ok(())
    }

    /// Stop the WebSocket server
    pub async fn stop(&mut self) -> Result<(), BridgeError> {
        if !self.running {
            return Err(BridgeError::NotRunning);
        }

        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(()).await;
        }

        // Close all client connections
        let mut clients = self.clients.write().await;
        clients.clear();

        self.running = false;
        println!("[ExternalBridge] Server stopped");
        Ok(())
    }

    /// Deny a pending authorization request
    pub async fn deny_pending_request(&self, client_id: &str) -> Result<(), BridgeError> {
        // Remove from pending
        let mut pending = self.pending_authorizations.write().await;
        pending.remove(client_id);

        // Send denial to client if connected
        let clients = self.clients.read().await;
        if let Some(client) = clients.get(client_id) {
            let msg = ProtocolMessage::AuthorizationUpdate { authorized: false };
            let json = serde_json::to_string(&msg)?;
            let _ = client.tx.send(Message::Text(json.into()));
        }

        Ok(())
    }

    /// Notify a client that authorization was granted
    pub async fn notify_authorization_granted(
        &self,
        client_id: &str,
        extension_id: &str,
    ) -> Result<(), BridgeError> {
        let mut clients = self.clients.write().await;
        if let Some(client) = clients.get_mut(client_id) {
            client.authorized = true;
            client.extension_id = Some(extension_id.to_string());

            let msg = ProtocolMessage::AuthorizationUpdate { authorized: true };
            let json = serde_json::to_string(&msg)?;
            let _ = client.tx.send(Message::Text(json.into()));
        }

        // Remove from pending
        let mut pending = self.pending_authorizations.write().await;
        pending.remove(client_id);

        Ok(())
    }

    /// Get all pending authorization requests
    pub async fn get_pending_authorizations(&self) -> Vec<PendingAuthorization> {
        let pending = self.pending_authorizations.read().await;
        pending.values().cloned().collect()
    }
}

/// Handle a single WebSocket connection
async fn handle_connection(
    stream: TcpStream,
    app_handle: AppHandle,
    clients: Arc<RwLock<HashMap<String, ConnectedClient>>>,
    pending: Arc<RwLock<HashMap<String, PendingAuthorization>>>,
    server_keypair: Arc<RwLock<Option<ServerKeyPair>>>,
    pending_responses: Arc<RwLock<HashMap<String, ResponseSender>>>,
    session_authorizations: Arc<RwLock<HashMap<String, SessionAuthorization>>>,
) -> Result<(), BridgeError> {
    let ws_stream = accept_async(stream).await?;
    let (mut write, mut read) = ws_stream.split();

    // Create channel for sending messages to this client
    let (tx, mut rx) = mpsc::unbounded_channel::<Message>();

    // Spawn task to forward messages from channel to WebSocket
    let write_task = tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            if write.send(msg).await.is_err() {
                break;
            }
        }
    });

    let mut client_id: Option<String> = None;
    let mut client_public_key_spki: Option<String> = None;
    let mut authorized = false;

    // Get server public key for handshake responses
    let server_public_key_spki = {
        let keypair_guard = server_keypair.read().await;
        match keypair_guard.as_ref() {
            Some(kp) => kp.public_key_spki_base64().unwrap_or_default(),
            None => String::new(),
        }
    };

    // Main message loop
    while let Some(msg_result) = read.next().await {
        let msg = match msg_result {
            Ok(m) => m,
            Err(e) => {
                eprintln!("[ExternalBridge] Read error: {}", e);
                break;
            }
        };

        match msg {
            Message::Text(text) => {
                // Debug: Log raw message (truncated for readability)
                let preview = if text.len() > 200 {
                    format!("{}...", &text[..200])
                } else {
                    text.to_string()
                };
                eprintln!("[ExternalBridge] Received raw message: {}", preview);

                let protocol_msg: ProtocolMessage = match serde_json::from_str(&text) {
                    Ok(m) => m,
                    Err(e) => {
                        eprintln!("[ExternalBridge] Parse error: {} - raw: {}", e, preview);
                        let error_msg = ProtocolMessage::Error {
                            code: "PARSE_ERROR".to_string(),
                            message: e.to_string(),
                        };
                        let json = serde_json::to_string(&error_msg)?;
                        tx.send(Message::Text(json.into()))?;
                        continue;
                    }
                };

                match protocol_msg {
                    ProtocolMessage::Handshake(handshake) => {
                        let cid = handshake.client.client_id.clone();
                        client_id = Some(cid.clone());

                        // Check if client is already authorized in database
                        let db_authorized = check_client_authorized(&app_handle, &cid).await;

                        // Check if client has session-based authorization (from "allow once")
                        let session_auth = {
                            let auths = session_authorizations.read().await;
                            auths.get(&cid).cloned()
                        };

                        let is_authorized = db_authorized || session_auth.is_some();
                        let ext_id = if db_authorized {
                            get_client_extension(&app_handle, &cid).await
                        } else {
                            session_auth.as_ref().map(|sa| sa.extension_id.clone())
                        };

                        if is_authorized {
                            // Client is authorized (either permanently or for this session)
                            authorized = true;

                            if session_auth.is_some() {
                                println!(
                                    "[ExternalBridge] Client {} authorized via session (allow once)",
                                    cid
                                );
                            }

                            // Add to connected clients
                            let mut clients_guard = clients.write().await;
                            clients_guard.insert(
                                cid.clone(),
                                ConnectedClient {
                                    client_id: cid.clone(),
                                    client_name: handshake.client.client_name.clone(),
                                    public_key: handshake.client.public_key.clone(),
                                    authorized: true,
                                    extension_id: ext_id.clone(),
                                    tx: tx.clone(),
                                },
                            );

                            // Update last_seen (only for database-authorized clients)
                            if db_authorized {
                                let _ = update_client_last_seen(&app_handle, &cid).await;
                            }

                            // Store client's public key for encrypted responses
                            client_public_key_spki = Some(handshake.client.public_key.clone());

                            // Send authorized response
                            let response = ProtocolMessage::HandshakeResponse(HandshakeResponse {
                                version: PROTOCOL_VERSION,
                                server_public_key: server_public_key_spki.clone(),
                                authorized: true,
                                pending_approval: false,
                            });
                            let json = serde_json::to_string(&response)?;
                            tx.send(Message::Text(json.into()))?;
                        } else {
                            // Client needs authorization
                            // Add to connected clients (unauthorized)
                            let mut clients_guard = clients.write().await;
                            clients_guard.insert(
                                cid.clone(),
                                ConnectedClient {
                                    client_id: cid.clone(),
                                    client_name: handshake.client.client_name.clone(),
                                    public_key: handshake.client.public_key.clone(),
                                    authorized: false,
                                    extension_id: None,
                                    tx: tx.clone(),
                                },
                            );

                            // Add to pending authorizations
                            let mut pending_guard = pending.write().await;
                            let pending_auth = PendingAuthorization {
                                client_id: cid.clone(),
                                client_name: handshake.client.client_name.clone(),
                                public_key: handshake.client.public_key.clone(),
                                requested_extensions: handshake.client.requested_extensions.clone(),
                            };
                            pending_guard.insert(cid.clone(), pending_auth.clone());

                            // Emit event to frontend to show authorization dialog
                            let _ = app_handle.emit("external:authorization-request", &pending_auth);

                            // Store client's public key for encrypted responses later
                            client_public_key_spki = Some(handshake.client.public_key.clone());

                            // Send pending response (include server public key for future encrypted communication)
                            let response = ProtocolMessage::HandshakeResponse(HandshakeResponse {
                                version: PROTOCOL_VERSION,
                                server_public_key: server_public_key_spki.clone(),
                                authorized: false,
                                pending_approval: true,
                            });
                            let json = serde_json::to_string(&response)?;
                            tx.send(Message::Text(json.into()))?;
                        }
                    }

                    ProtocolMessage::Request(envelope) => {
                        eprintln!(
                            "[ExternalBridge] Received request: action={}, client_id={:?}, ext_pk={:?}, ext_name={:?}",
                            envelope.action,
                            client_id,
                            envelope.extension_public_key,
                            envelope.extension_name
                        );

                        // Check authorization - either from handshake or session (allow once)
                        // Session authorization may have been granted AFTER the handshake
                        let is_authorized = if authorized {
                            true
                        } else if let Some(cid) = &client_id {
                            let session_auth = session_authorizations.read().await;
                            session_auth.contains_key(cid)
                        } else {
                            false
                        };

                        if !is_authorized {
                            eprintln!("[ExternalBridge] Request rejected: client not authorized");
                            let error_msg = ProtocolMessage::Error {
                                code: "UNAUTHORIZED".to_string(),
                                message: "Client not authorized".to_string(),
                            };
                            let json = serde_json::to_string(&error_msg)?;
                            tx.send(Message::Text(json.into()))?;
                            continue;
                        }

                        // Decrypt the envelope using server's keypair
                        let keypair_guard = server_keypair.read().await;
                        let decrypted = match keypair_guard.as_ref() {
                            Some(kp) => envelope.decrypt(kp),
                            None => {
                                let error_msg = ProtocolMessage::Error {
                                    code: "SERVER_ERROR".to_string(),
                                    message: "Server keypair not available".to_string(),
                                };
                                let json = serde_json::to_string(&error_msg)?;
                                tx.send(Message::Text(json.into()))?;
                                continue;
                            }
                        };
                        drop(keypair_guard);

                        match decrypted {
                            Ok(payload) => {
                                // Process the decrypted request
                                // Use client's public key as identifier (consistent with rest of haex-vault)
                                let public_key = client_public_key_spki.as_deref().unwrap_or("");
                                let cid = client_id.as_deref().unwrap_or("");
                                let response_payload = process_request(
                                    &envelope.action,
                                    &payload,
                                    public_key,
                                    envelope.extension_public_key.as_deref(),
                                    envelope.extension_name.as_deref(),
                                    cid,
                                    &app_handle,
                                    pending_responses.clone(),
                                    session_authorizations.clone(),
                                ).await;

                                // Send encrypted response back
                                if let Some(client_pk) = &client_public_key_spki {
                                    match create_encrypted_response(
                                        &envelope.action,
                                        &response_payload,
                                        client_pk,
                                    ) {
                                        Ok(response_envelope) => {
                                            let response = ProtocolMessage::Response(response_envelope);
                                            let json = serde_json::to_string(&response)?;
                                            tx.send(Message::Text(json.into()))?;
                                        }
                                        Err(e) => {
                                            eprintln!("[ExternalBridge] Failed to encrypt response: {}", e);
                                            let error_msg = ProtocolMessage::Error {
                                                code: "ENCRYPTION_ERROR".to_string(),
                                                message: "Failed to encrypt response".to_string(),
                                            };
                                            let json = serde_json::to_string(&error_msg)?;
                                            tx.send(Message::Text(json.into()))?;
                                        }
                                    }
                                }
                            }
                            Err(e) => {
                                eprintln!("[ExternalBridge] Failed to decrypt request: {}", e);
                                let error_msg = ProtocolMessage::Error {
                                    code: "DECRYPTION_ERROR".to_string(),
                                    message: "Failed to decrypt request".to_string(),
                                };
                                let json = serde_json::to_string(&error_msg)?;
                                tx.send(Message::Text(json.into()))?;
                            }
                        }
                    }

                    ProtocolMessage::Ping => {
                        let pong = ProtocolMessage::Pong;
                        let json = serde_json::to_string(&pong)?;
                        tx.send(Message::Text(json.into()))?;
                    }

                    _ => {
                        // Ignore other message types
                    }
                }
            }

            Message::Binary(_) => {
                // Binary messages not supported
            }

            Message::Ping(data) => {
                tx.send(Message::Pong(data))?;
            }

            Message::Pong(_) => {
                // Ignore pong responses
            }

            Message::Close(_) => {
                break;
            }

            Message::Frame(_) => {
                // Raw frames not expected
            }
        }
    }

    // Cleanup on disconnect
    if let Some(cid) = client_id {
        let mut clients_guard = clients.write().await;
        clients_guard.remove(&cid);
        println!("[ExternalBridge] Client {} disconnected", cid);
    }

    // Cancel write task
    write_task.abort();

    Ok(())
}

/// Check if a client is authorized (via CRDT database query)
async fn check_client_authorized(app_handle: &AppHandle, client_id: &str) -> bool {
    let state = app_handle.state::<AppState>();
    let params = vec![JsonValue::String(client_id.to_string())];

    match select_with_crdt(SQL_IS_CLIENT_KNOWN.to_string(), params, &state.db) {
        Ok(rows) => {
            if let Some(row) = rows.first() {
                if let Some(count) = row.first() {
                    return count.as_i64().unwrap_or(0) > 0;
                }
            }
            false
        }
        Err(_) => false,
    }
}

/// Get the extension_id for an authorized client
async fn get_client_extension(app_handle: &AppHandle, client_id: &str) -> Option<String> {
    let state = app_handle.state::<AppState>();
    let params = vec![JsonValue::String(client_id.to_string())];

    match select_with_crdt(SQL_GET_CLIENT_EXTENSION.to_string(), params, &state.db) {
        Ok(rows) => {
            if let Some(row) = rows.first() {
                if let Some(ext_id) = row.first() {
                    return ext_id.as_str().map(|s| s.to_string());
                }
            }
            None
        }
        Err(_) => None,
    }
}

/// Update last_seen timestamp for a client
async fn update_client_last_seen(
    app_handle: &AppHandle,
    client_id: &str,
) -> Result<(), BridgeError> {
    let state = app_handle.state::<AppState>();
    let hlc_guard = state
        .hlc
        .lock()
        .map_err(|e| BridgeError::Database(e.to_string()))?;

    let params = vec![JsonValue::String(client_id.to_string())];

    execute_with_crdt(
        SQL_UPDATE_LAST_SEEN.to_string(),
        params,
        &state.db,
        &hlc_guard,
    )
    .map_err(|e| BridgeError::Database(e.to_string()))?;

    Ok(())
}

/// Check if a client is authorized for a specific extension (by extension public_key + name)
/// For dev extensions (public_key starts with "dev_"), this always returns false
/// because dev extensions are not stored in the database. Authorization for dev
/// extensions is handled via session authorization instead.
async fn check_client_authorized_for_extension(
    app_handle: &AppHandle,
    client_id: &str,
    extension_public_key: &str,
    extension_name: &str,
) -> bool {
    // Dev extensions are not in the database, so we can't check DB authorization
    // Session authorization is checked separately
    if extension_public_key.starts_with("dev_") {
        return false;
    }

    let state = app_handle.state::<AppState>();
    let params = vec![
        JsonValue::String(client_id.to_string()),
        JsonValue::String(extension_public_key.to_string()),
        JsonValue::String(extension_name.to_string()),
    ];

    match select_with_crdt(
        SQL_IS_CLIENT_AUTHORIZED_FOR_EXTENSION.to_string(),
        params,
        &state.db,
    ) {
        Ok(rows) => {
            if let Some(row) = rows.first() {
                if let Some(count) = row.first() {
                    return count.as_i64().unwrap_or(0) > 0;
                }
            }
            false
        }
        Err(e) => {
            eprintln!(
                "[ExternalBridge] Failed to check client authorization: {}",
                e
            );
            false
        }
    }
}

/// Get extension ID by public_key and name
/// First checks dev_extensions in memory, then falls back to database lookup
async fn get_extension_id_by_public_key_and_name(
    app_handle: &AppHandle,
    extension_public_key: &str,
    extension_name: &str,
) -> Option<String> {
    let state = app_handle.state::<AppState>();

    // First, check if this is a dev extension request (public_key starts with "dev_")
    // Dev extensions are stored in memory, not in the database
    if extension_public_key.starts_with("dev_") {
        // Extract the actual public key (remove "dev_" prefix)
        let actual_public_key = &extension_public_key[4..];
        // Dev extension ID format: dev_{public_key}_{name}
        let expected_dev_id = format!("dev_{}_{}", actual_public_key, extension_name);

        let dev_extensions = state.extension_manager.dev_extensions.lock().unwrap();
        if dev_extensions.contains_key(&expected_dev_id) {
            eprintln!(
                "[ExternalBridge] Found dev extension: {}",
                expected_dev_id
            );
            return Some(expected_dev_id);
        }
        eprintln!(
            "[ExternalBridge] Dev extension not found: {} (available: {:?})",
            expected_dev_id,
            dev_extensions.keys().collect::<Vec<_>>()
        );
        return None;
    }

    // For production extensions, look up in database
    let params = vec![
        JsonValue::String(extension_public_key.to_string()),
        JsonValue::String(extension_name.to_string()),
    ];

    match select_with_crdt(
        SQL_GET_EXTENSION_ID_BY_PUBLIC_KEY_AND_NAME.to_string(),
        params,
        &state.db,
    ) {
        Ok(rows) => {
            if let Some(row) = rows.first() {
                if let Some(id) = row.first() {
                    return id.as_str().map(|s| s.to_string());
                }
            }
            None
        }
        Err(e) => {
            eprintln!("[ExternalBridge] Failed to get extension ID: {}", e);
            None
        }
    }
}

/// Ensure an extension is loaded (auto-start if needed)
/// Returns Ok(()) if extension is loaded or was successfully started
///
/// This function:
/// 1. Checks if extension already has an open window (Desktop only)
/// 2. If not, emits an event to the frontend to request extension loading
/// 3. Waits for the extension to be ready
async fn ensure_extension_loaded(
    app_handle: &AppHandle,
    extension_id: &str,
) -> Result<(), String> {
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        let state = app_handle.state::<AppState>();

        // Check if extension already has an open window
        if state
            .extension_webview_manager
            .has_window_for_extension(extension_id)
        {
            eprintln!(
                "[ExternalBridge] Extension {} already has an open window",
                extension_id
            );
            return Ok(());
        }
    }

    // Extension not loaded - request frontend to start it
    eprintln!(
        "[ExternalBridge] Extension {} not loaded, requesting frontend to start it...",
        extension_id
    );

    // Emit event to frontend to start the extension
    // The frontend will handle this based on the extension's display_mode
    let payload = serde_json::json!({
        "extensionId": extension_id,
    });

    if let Err(e) = app_handle.emit(EVENT_EXTENSION_AUTO_START_REQUEST, &payload) {
        return Err(format!("Failed to emit auto-start request: {}", e));
    }

    // Wait for extension to initialize
    // TODO: Implement proper signaling mechanism where extension confirms it's ready
    // For now, we wait a fixed time which should be enough for most extensions
    tokio::time::sleep(Duration::from_millis(1000)).await;

    // Verify extension is now loaded (Desktop only)
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        let state = app_handle.state::<AppState>();
        if !state
            .extension_webview_manager
            .has_window_for_extension(extension_id)
        {
            // Extension might be running in iframe mode, which we can't detect from backend
            // We'll proceed and let the request timeout if the extension doesn't respond
            eprintln!(
                "[ExternalBridge] Extension {} may be running in iframe mode or failed to start",
                extension_id
            );
        }
    }

    Ok(())
}

/// Process a decrypted request and route it to the appropriate extension
///
/// # Arguments
/// * `action` - The action/method name to perform
/// * `payload` - The decrypted request payload (must contain requestId)
/// * `client_public_key` - Client's public key (Base64 SPKI format, used as identifier)
/// * `extension_public_key` - Target extension's public key (from manifest)
/// * `extension_name` - Target extension's name (from manifest)
/// * `client_id` - Client's unique identifier
/// * `app_handle` - Tauri app handle for emitting events
/// * `pending_responses` - Map to store response channel for correlation
async fn process_request(
    action: &str,
    payload: &serde_json::Value,
    client_public_key: &str,
    extension_public_key: Option<&str>,
    extension_name: Option<&str>,
    client_id: &str,
    app_handle: &AppHandle,
    pending_responses: Arc<RwLock<HashMap<String, ResponseSender>>>,
    session_authorizations: Arc<RwLock<HashMap<String, SessionAuthorization>>>,
) -> serde_json::Value {
    // Extract requestId - required for response correlation
    let request_id = match payload.get("requestId").and_then(|v| v.as_str()) {
        Some(id) if !id.is_empty() => id.to_string(),
        _ => {
            return serde_json::json!({
                "success": false,
                "error": "Missing required field: requestId"
            });
        }
    };

    // Validate that extension target is specified
    let (ext_public_key, ext_name) = match (extension_public_key, extension_name) {
        (Some(pk), Some(name)) if !pk.is_empty() && !name.is_empty() => (pk, name),
        _ => {
            return serde_json::json!({
                "requestId": request_id,
                "success": false,
                "error": "Missing required fields: extensionPublicKey and extensionName"
            });
        }
    };

    // Lookup the extension's internal ID first (needed for session auth check)
    let extension_id = match get_extension_id_by_public_key_and_name(app_handle, ext_public_key, ext_name).await {
        Some(id) => id,
        None => {
            return serde_json::json!({
                "requestId": request_id,
                "success": false,
                "error": "Extension not found"
            });
        }
    };

    // Verify client is authorized for this extension
    // Check both database authorization AND session authorization ("allow once")
    let db_authorized = check_client_authorized_for_extension(app_handle, client_id, ext_public_key, ext_name).await;
    let session_authorized = {
        let auths = session_authorizations.read().await;
        auths.get(client_id).map(|sa| sa.extension_id == extension_id).unwrap_or(false)
    };

    if !db_authorized && !session_authorized {
        return serde_json::json!({
            "requestId": request_id,
            "success": false,
            "error": "Client not authorized for this extension"
        });
    }

    // Ensure the extension is loaded (auto-start if needed)
    if let Err(e) = ensure_extension_loaded(app_handle, &extension_id).await {
        eprintln!("[ExternalBridge] Failed to ensure extension is loaded: {}", e);
        return serde_json::json!({
            "requestId": request_id,
            "success": false,
            "error": format!("Failed to load extension: {}", e)
        });
    }

    // Create oneshot channel for response
    let (tx, rx) = oneshot::channel::<serde_json::Value>();

    // Store the sender in pending_responses
    {
        let mut pending = pending_responses.write().await;
        pending.insert(request_id.clone(), tx);
    }

    // Build the external request payload to send to the extension
    let external_request = serde_json::json!({
        "requestId": request_id,
        "publicKey": client_public_key,
        "action": action,
        "payload": payload,
        "extensionPublicKey": ext_public_key,
        "extensionName": ext_name
    });

    // Emit the request to the extension via Tauri event
    // For WebView extensions: emit directly to the extension's webview window
    // For iframe extensions: emit to main window (frontend will forward via postMessage)
    let emit_result = {
        let state = app_handle.state::<AppState>();
        let manager = &state.extension_webview_manager;

        // Try to emit to extension webviews first
        let webview_result = manager.emit_to_all_extensions(
            app_handle,
            "haextension:external:request",
            external_request.clone(),
        );

        // Also emit to main window for iframe-based extensions
        let main_result = app_handle.emit("haextension:external:request", &external_request);

        // Consider success if either worked
        webview_result.is_ok() || main_result.is_ok()
    };

    if !emit_result {
        eprintln!("[ExternalBridge] Failed to emit external request to any window");
        // Clean up pending response
        let mut pending = pending_responses.write().await;
        pending.remove(&request_id);
        return serde_json::json!({
            "requestId": request_id,
            "success": false,
            "error": "Failed to route request to extension"
        });
    }

    // Wait for response with timeout
    // TODO: Make timeout configurable per extension
    match tokio::time::timeout(Duration::from_secs(DEFAULT_REQUEST_TIMEOUT_SECS), rx).await {
        Ok(Ok(response)) => response,
        Ok(Err(_)) => {
            // Channel was dropped (sender was dropped without sending)
            serde_json::json!({
                "requestId": request_id,
                "success": false,
                "error": "Extension did not respond"
            })
        }
        Err(_) => {
            // Timeout
            // Clean up pending response
            let mut pending = pending_responses.write().await;
            pending.remove(&request_id);
            serde_json::json!({
                "requestId": request_id,
                "success": false,
                "error": "Request timeout"
            })
        }
    }
}
