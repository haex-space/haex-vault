//! WebSocket server for external bridge
//!
//! Handles incoming connections from external clients (browser extensions,
//! CLI tools, servers, etc.) and routes requests to haex-vault extensions.

use crate::AppState;
use crate::database::core::{execute_with_crdt, select_with_crdt};
use futures_util::{SinkExt, StreamExt};
use serde_json::Value as JsonValue;
use std::collections::HashMap;
use std::sync::Arc;
use tauri::{AppHandle, Emitter};
use tokio::net::{TcpListener, TcpStream};
use std::time::Duration;
use tokio::sync::{mpsc, oneshot, RwLock};
use tokio_tungstenite::{accept_async, tungstenite::Message};

use super::authorization::{
    PendingAuthorization, SQL_GET_CLIENT_EXTENSION, SQL_IS_CLIENT_KNOWN,
    SQL_UPDATE_LAST_SEEN,
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
                let protocol_msg: ProtocolMessage = match serde_json::from_str(&text) {
                    Ok(m) => m,
                    Err(e) => {
                        eprintln!("[ExternalBridge] Parse error: {}", e);
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
                        if !authorized {
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
                                let response_payload = process_request(
                                    &envelope.action,
                                    &payload,
                                    public_key,
                                    &app_handle,
                                    pending_responses.clone(),
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

/// Process a decrypted request and route it to the appropriate extension
///
/// # Arguments
/// * `action` - The action/method name to perform
/// * `payload` - The decrypted request payload (must contain requestId)
/// * `public_key` - Client's public key (Base64 SPKI format, used as identifier)
/// * `app_handle` - Tauri app handle for emitting events
/// * `pending_responses` - Map to store response channel for correlation
async fn process_request(
    action: &str,
    payload: &serde_json::Value,
    public_key: &str,
    app_handle: &AppHandle,
    pending_responses: Arc<RwLock<HashMap<String, ResponseSender>>>,
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
        "publicKey": public_key,
        "action": action,
        "payload": payload
    });

    // Emit the request to the extension via Tauri event
    // The extension's SDK will receive this and call the appropriate handler
    if let Err(e) = app_handle.emit("haextension:external:request", &external_request) {
        eprintln!("[ExternalBridge] Failed to emit external request: {}", e);
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
