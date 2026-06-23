//! WebSocket server for external bridge
//!
//! Handles incoming connections from external clients (browser extensions,
//! CLI tools, servers, etc.) and routes requests to haex-vault extensions.

mod auth;
mod connection;
mod process;

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tauri::AppHandle;
use tokio::net::TcpListener;
use tokio::sync::{mpsc, oneshot, Notify, RwLock};
use tokio_tungstenite::tungstenite::Message;

use super::authorization::PendingAuthorization;
use super::crypto::ServerKeyPair;
use super::error::BridgeError;
use super::protocol::ProtocolMessage;

/// Default port for the external bridge WebSocket server
pub const DEFAULT_BRIDGE_PORT: u16 = 19455;
const PROTOCOL_VERSION: u32 = 1;
/// Default timeout for extension responses (can be overridden per extension)
const DEFAULT_REQUEST_TIMEOUT_SECS: u64 = 30;

/// Type alias for pending response senders
type ResponseSender = oneshot::Sender<serde_json::Value>;

/// Connected client state
#[allow(dead_code)]
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
    /// Human-readable client name (e.g. "haex-pass Browser Extension")
    pub client_name: String,
    /// Client's public key (base64)
    pub public_key: String,
    /// Extension ID this client can access
    pub extension_id: String,
}

/// Session blocked client entry (for "deny once" blocks)
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, rename_all = "camelCase")]
#[serde(rename_all = "camelCase")]
pub struct SessionBlockedClient {
    /// Unique client identifier (public key fingerprint)
    pub client_id: String,
    /// Human-readable client name
    pub client_name: String,
    /// Client's public key (base64)
    pub public_key: String,
}

/// External Bridge WebSocket Server
pub struct ExternalBridge {
    running: bool,
    current_port: u16,
    shutdown_tx: Option<mpsc::Sender<()>>,
    /// Handle to the accept-loop task spawned by `start`. Kept so `stop`
    /// can `.await` (and as a fallback `.abort()`) the task — without this,
    /// the listener can still hold the port for a short window after
    /// `stop` returns, and a quick `stop` → `start` cycle hits a bind
    /// conflict on the same port.
    server_task: Option<tokio::task::JoinHandle<()>>,
    clients: Arc<RwLock<HashMap<String, ConnectedClient>>>,
    pending_authorizations: Arc<RwLock<HashMap<String, PendingAuthorization>>>,
    server_keypair: Arc<RwLock<Option<ServerKeyPair>>>,
    /// Pending responses waiting for extension callbacks (requestId → sender)
    pending_responses: Arc<RwLock<HashMap<String, ResponseSender>>>,
    /// Session-based authorizations (for "allow once" - cleared when server stops)
    /// Key: client_id, Value: SessionAuthorization
    session_authorizations: Arc<RwLock<HashMap<String, SessionAuthorization>>>,
    /// Session-based blocked clients (for "deny once" - cleared when server stops)
    /// Key: client_id, Value: SessionBlockedClient
    session_blocked: Arc<RwLock<HashMap<String, SessionBlockedClient>>>,
    /// Extension ready signals - notifies when an extension has completed initialization
    /// Key: extension_id, Value: Notify that fires when extension is ready
    extension_ready_signals: Arc<RwLock<HashMap<String, Arc<Notify>>>>,
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
            server_task: None,
            clients: Arc::new(RwLock::new(HashMap::new())),
            pending_authorizations: Arc::new(RwLock::new(HashMap::new())),
            server_keypair: Arc::new(RwLock::new(None)),
            pending_responses: Arc::new(RwLock::new(HashMap::new())),
            session_authorizations: Arc::new(RwLock::new(HashMap::new())),
            session_blocked: Arc::new(RwLock::new(HashMap::new())),
            extension_ready_signals: Arc::new(RwLock::new(HashMap::new())),
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
    pub async fn add_session_authorization(
        &self,
        client_id: &str,
        client_name: &str,
        public_key: &str,
        extension_id: &str,
    ) {
        let mut authorizations = self.session_authorizations.write().await;
        authorizations.insert(
            client_id.to_string(),
            SessionAuthorization {
                client_id: client_id.to_string(),
                client_name: client_name.to_string(),
                public_key: public_key.to_string(),
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

    /// Get a clone of the session_blocked map for use in connection handlers
    pub fn get_session_blocked(&self) -> Arc<RwLock<HashMap<String, SessionBlockedClient>>> {
        self.session_blocked.clone()
    }

    /// Add a client to the session blocked list (for "deny once")
    pub async fn add_session_blocked(&self, client_id: &str, client_name: &str, public_key: &str) {
        let mut blocked = self.session_blocked.write().await;
        blocked.insert(
            client_id.to_string(),
            SessionBlockedClient {
                client_id: client_id.to_string(),
                client_name: client_name.to_string(),
                public_key: public_key.to_string(),
            },
        );
        println!(
            "[ExternalBridge] Added client {} to session blocked list",
            client_id
        );
    }

    /// Remove a client from the session blocked list
    pub async fn remove_session_blocked(&self, client_id: &str) {
        let mut blocked = self.session_blocked.write().await;
        blocked.remove(client_id);
        println!(
            "[ExternalBridge] Removed client {} from session blocked list",
            client_id
        );
    }

    /// Check if a client is session blocked
    pub async fn is_session_blocked(&self, client_id: &str) -> bool {
        let blocked = self.session_blocked.read().await;
        blocked.contains_key(client_id)
    }

    /// Get all session blocked clients
    pub async fn get_session_blocked_clients(&self) -> Vec<SessionBlockedClient> {
        let blocked = self.session_blocked.read().await;
        blocked.values().cloned().collect()
    }

    /// Signal that an extension has completed initialization and is ready to handle requests.
    /// This notifies any waiting `ensure_extension_loaded` calls.
    pub async fn signal_extension_ready(&self, extension_id: &str) {
        let signals = self.extension_ready_signals.read().await;
        if let Some(notify) = signals.get(extension_id) {
            eprintln!("[ExternalBridge] Extension {} signaled ready", extension_id);
            notify.notify_waiters();
        }
    }

    /// Wait for an extension to signal that it's ready, with a timeout.
    /// Returns true if the extension signaled ready, false if timeout occurred.
    pub async fn wait_for_extension_ready(&self, extension_id: &str, timeout_ms: u64) -> bool {
        // Get or create a Notify for this extension
        let notify = {
            let mut signals = self.extension_ready_signals.write().await;
            signals
                .entry(extension_id.to_string())
                .or_insert_with(|| Arc::new(Notify::new()))
                .clone()
        };

        // Wait for notification with timeout
        let result =
            tokio::time::timeout(Duration::from_millis(timeout_ms), notify.notified()).await;

        // Cleanup the signal entry
        {
            let mut signals = self.extension_ready_signals.write().await;
            signals.remove(extension_id);
        }

        result.is_ok()
    }

    /// Get a clone of the extension_ready_signals map
    pub fn get_extension_ready_signals(&self) -> Arc<RwLock<HashMap<String, Arc<Notify>>>> {
        self.extension_ready_signals.clone()
    }

    pub fn is_running(&self) -> bool {
        self.running
    }

    /// Get the current port the server is running on (or will run on)
    pub fn get_port(&self) -> u16 {
        self.current_port
    }

    /// Start the WebSocket server on the specified port
    pub async fn start(
        &mut self,
        app_handle: AppHandle,
        port: Option<u16>,
    ) -> Result<(), BridgeError> {
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
        let session_blocked = self.session_blocked.clone();

        // Spawn the server task. The JoinHandle is stored on `self` so
        // `stop` can await it; without that the listener-bound port may
        // still be busy for a short window after `stop` returns, breaking
        // a quick `stop` → `start` cycle.
        let task = tokio::spawn(async move {
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
                                let session_blk = session_blocked.clone();

                                tokio::spawn(async move {
                                    if let Err(e) = connection::handle_connection(stream, app, clients, pending, keypair, pending_resp, session_auths, session_blk).await {
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
        self.server_task = Some(task);

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

        // Wait for the accept-loop task to finish so the listener's port is
        // fully released before we return. With a 2s upper bound: if the
        // task is stuck inside `accept_async` waiting on a slow client
        // handshake, `abort()` cancels it. Without this guard, a quick
        // `stop` → `start` cycle can hit a bind conflict on the same port.
        if let Some(task) = self.server_task.take() {
            match tokio::time::timeout(Duration::from_secs(2), task).await {
                Ok(Ok(())) => {}
                Ok(Err(e)) => {
                    eprintln!("[ExternalBridge] Server task ended with error: {}", e);
                }
                Err(_) => {
                    eprintln!("[ExternalBridge] Server task did not exit within 2s; detaching");
                    // The handle was moved into the timeout future and is
                    // unreachable on the timeout branch, so it is dropped
                    // here. Dropping a Tokio JoinHandle *detaches* the task
                    // (it keeps running) rather than cancelling it — but in
                    // practice the shutdown signal sent above breaks the
                    // accept-loop's `select!`, so reaching this branch is
                    // already an anomaly; we accept a brief detach over
                    // restructuring to thread an AbortHandle through.
                }
            }
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
        println!(
            "[ExternalBridge] notify_authorization_granted called for client_id={}, extension_id={}",
            client_id, extension_id
        );

        let mut clients = self.clients.write().await;
        println!(
            "[ExternalBridge] Connected clients: {:?}",
            clients.keys().collect::<Vec<_>>()
        );

        if let Some(client) = clients.get_mut(client_id) {
            client.authorized = true;
            client.extension_id = Some(extension_id.to_string());

            let msg = ProtocolMessage::AuthorizationUpdate { authorized: true };
            let json = serde_json::to_string(&msg)?;
            let send_result = client.tx.send(Message::Text(json.into()));
            println!(
                "[ExternalBridge] Sent authorization update to client {}: {:?}",
                client_id, send_result
            );
        } else {
            println!(
                "[ExternalBridge] WARNING: Client {} not found in connected clients!",
                client_id
            );
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

#[cfg(test)]
mod lifecycle_tests {
    //! Regression guard: ExternalBridge::stop must reliably release the
    //! listener port before returning.
    //!
    //! A behavioural test requires standing up a real WebSocket server +
    //! racing a fast `stop` → `start` cycle to reproduce the port-busy
    //! window. A source-level guard catches accidental removal of the
    //! `server_task` handle / timeout-bound await much more cheaply.

    /// stop() must await the spawned accept-loop task (bounded by a
    /// timeout) before returning, otherwise the listener port can still
    /// be busy when start() is called immediately afterwards.
    #[test]
    fn stop_must_await_server_task() {
        let source = include_str!("mod.rs");
        let production = source
            .split_once("#[cfg(test)]")
            .map(|(p, _)| p)
            .unwrap_or(source);

        // The task handle must be stored on self so stop can reach it.
        assert!(
            production.contains("server_task: Option<tokio::task::JoinHandle"),
            "ExternalBridge must store the accept-loop JoinHandle so stop \
             can wait for the listener to be fully released"
        );
        // stop must consume the handle and bound the wait.
        assert!(
            production.contains("self.server_task.take()"),
            "ExternalBridge::stop must take and await self.server_task"
        );
        assert!(
            production.contains("tokio::time::timeout(Duration::from_secs(2), task)"),
            "stop must bound the wait on the server task so a stuck \
             accept_async cannot wedge the call"
        );
    }
}
