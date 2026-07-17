//! WebSocket connection handler — runs the per-connection message loop:
//! handshake, request routing, ping/pong, and cleanup on disconnect.

use futures_util::{SinkExt, StreamExt};
use std::collections::HashMap;
use std::sync::Arc;
use tauri::{AppHandle, Emitter};
use tokio::net::TcpStream;
use tokio::sync::{mpsc, RwLock};
use tokio_tungstenite::{accept_async, tungstenite::Message};

use super::auth::{
    check_client_authorized, check_client_blocked, get_client_extension,
    get_stored_requested_permissions, update_client_last_seen,
};
use super::process::process_request;
use super::{
    ConnectedClient, ResponseSender, SessionAuthorization, SessionBlockedClient, PROTOCOL_VERSION,
};
use crate::external_bridge::authorization::PendingAuthorization;
use crate::external_bridge::crypto::{create_encrypted_response, ServerKeyPair};
use crate::external_bridge::error::BridgeError;
use crate::external_bridge::protocol::{
    canonical_requested_permissions, has_permissions_declaration, HandshakeResponse,
    ProtocolMessage,
};

/// Handle a single WebSocket connection
pub(super) async fn handle_connection(
    stream: TcpStream,
    app_handle: AppHandle,
    clients: Arc<RwLock<HashMap<String, ConnectedClient>>>,
    pending: Arc<RwLock<HashMap<String, PendingAuthorization>>>,
    server_keypair: Arc<RwLock<Option<ServerKeyPair>>>,
    pending_responses: Arc<RwLock<HashMap<String, ResponseSender>>>,
    session_authorizations: Arc<RwLock<HashMap<String, SessionAuthorization>>>,
    session_blocked: Arc<RwLock<HashMap<String, SessionBlockedClient>>>,
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

    // Get server public key for handshake responses
    let server_public_key_base64 = {
        let keypair_guard = server_keypair.read().await;
        match keypair_guard.as_ref() {
            Some(kp) => kp.public_key_base64(),
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

                        // Protocol v2 requires a permissions declaration. Reject
                        // early (same shape as the blocked-client branch below):
                        // send an Error frame explaining why, then an
                        // unauthorized/non-pending HandshakeResponse, then close.
                        if handshake.version < 2 || !has_permissions_declaration(&handshake.client)
                        {
                            eprintln!(
                                "[ExternalBridge] Client {} rejected: protocol v{} or missing permissions declaration",
                                cid, handshake.version
                            );
                            let error_msg = ProtocolMessage::Error {
                                code: "PERMISSIONS_DECLARATION_REQUIRED".to_string(),
                                message:
                                    "Handshake must declare requested permissions (protocol v2+)"
                                        .to_string(),
                            };
                            let json = serde_json::to_string(&error_msg)?;
                            tx.send(Message::Text(json.into()))?;

                            let response = ProtocolMessage::HandshakeResponse(HandshakeResponse {
                                version: PROTOCOL_VERSION,
                                server_public_key: server_public_key_base64.clone(),
                                authorized: false,
                                pending_approval: false,
                            });
                            let json = serde_json::to_string(&response)?;
                            tx.send(Message::Text(json.into()))?;
                            break;
                        }

                        // Check if client is blocked (permanent or session)
                        let is_db_blocked = check_client_blocked(&app_handle, &cid).await;
                        let is_session_blocked = {
                            let blocked = session_blocked.read().await;
                            blocked.contains_key(&cid)
                        };

                        if is_db_blocked || is_session_blocked {
                            println!(
                                "[ExternalBridge] Client {} is blocked (db={}, session={}), rejecting connection",
                                cid, is_db_blocked, is_session_blocked
                            );
                            let response = ProtocolMessage::HandshakeResponse(HandshakeResponse {
                                version: PROTOCOL_VERSION,
                                server_public_key: server_public_key_base64.clone(),
                                authorized: false,
                                pending_approval: false,
                            });
                            let json = serde_json::to_string(&response)?;
                            tx.send(Message::Text(json.into()))?;
                            // Close connection for blocked clients
                            break;
                        }

                        // Check if client is already authorized in database
                        let db_authorized = check_client_authorized(&app_handle, &cid).await;

                        // Check if client has session-based authorization (from "allow once").
                        // A client may hold several session entries (one per granted
                        // target); they share the same manifest, so any one answers
                        // the manifest-match check below.
                        let session_auth = {
                            let auths = session_authorizations.read().await;
                            auths.values().find(|sa| sa.client_id == cid).cloned()
                        };

                        // A stored authorization only counts if the client's live
                        // declaration still matches what was granted. A changed
                        // manifest (different declared actions/permissions) must
                        // force re-authorization rather than silently keep the
                        // old grant (Entscheidung 3 in the permission-parity plan).
                        let canonical_manifest = canonical_requested_permissions(
                            &handshake.client.permissions,
                            &handshake.client.requested_extensions,
                        );
                        let db_manifest_matches = if db_authorized {
                            get_stored_requested_permissions(&app_handle, &cid)
                                .await
                                .as_deref()
                                == Some(canonical_manifest.as_str())
                        } else {
                            false
                        };
                        let session_manifest_matches = session_auth
                            .as_ref()
                            .map(|sa| sa.requested_permissions == canonical_manifest)
                            .unwrap_or(false);

                        let is_authorized = (db_authorized && db_manifest_matches)
                            || (session_auth.is_some() && session_manifest_matches);
                        let ext_id = if db_authorized && db_manifest_matches {
                            get_client_extension(&app_handle, &cid).await
                        } else if session_manifest_matches {
                            session_auth.as_ref().map(|sa| sa.extension_id.clone())
                        } else {
                            None
                        };

                        if is_authorized {
                            // Client is authorized (either permanently or for this session)
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
                                server_public_key: server_public_key_base64.clone(),
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

                            // Add to pending authorizations. If the same client_id is
                            // already pending (e.g. a flaky extension that disconnects
                            // and re-handshakes in a loop), suppress the duplicate UI
                            // event — otherwise every reconnect re-fires GTK present()
                            // and stacks a new auth modal, which jams the window.
                            let mut pending_guard = pending.write().await;
                            let pending_auth = PendingAuthorization {
                                client_id: cid.clone(),
                                client_name: handshake.client.client_name.clone(),
                                public_key: handshake.client.public_key.clone(),
                                requested_extensions: handshake.client.requested_extensions.clone(),
                                permissions: handshake.client.permissions.clone(),
                            };
                            let already_pending = pending_guard
                                .insert(cid.clone(), pending_auth.clone())
                                .is_some();

                            if !already_pending {
                                // Emit event to frontend to show authorization dialog.
                                // Nur Main-Window — der Authorization-Dialog wird dort
                                // gerendert; Extensions dürfen Authorization-Requests
                                // anderer Clients nicht beobachten.
                                let _ = app_handle.emit_to(
                                    "main",
                                    "external:authorization-request",
                                    &pending_auth,
                                );
                            }

                            // Store client's public key for encrypted responses later
                            client_public_key_spki = Some(handshake.client.public_key.clone());

                            // Send pending response (include server public key for future encrypted communication)
                            let response = ProtocolMessage::HandshakeResponse(HandshakeResponse {
                                version: PROTOCOL_VERSION,
                                server_public_key: server_public_key_base64.clone(),
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

                        // Check authorization - from clients map or session (allow once)
                        // Authorization may have been granted AFTER the handshake via notify_authorization_granted()
                        let is_authorized = if let Some(cid) = &client_id {
                            // First check if client is authorized in the shared clients map
                            // This catches authorization granted after handshake
                            let clients_guard = clients.read().await;
                            let client_authorized = clients_guard
                                .get(cid)
                                .map(|c| c.authorized)
                                .unwrap_or(false);
                            drop(clients_guard);

                            if client_authorized {
                                true
                            } else {
                                // Fall back to session authorization (allow once)
                                let session_auth = session_authorizations.read().await;
                                session_auth.values().any(|sa| &sa.client_id == cid)
                            }
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
                                )
                                .await;

                                // Send encrypted response back
                                if let Some(client_pk) = &client_public_key_spki {
                                    match create_encrypted_response(
                                        &envelope.action,
                                        &response_payload,
                                        client_pk,
                                    ) {
                                        Ok(response_envelope) => {
                                            let response =
                                                ProtocolMessage::Response(response_envelope);
                                            let json = serde_json::to_string(&response)?;
                                            tx.send(Message::Text(json.into()))?;
                                        }
                                        Err(e) => {
                                            eprintln!(
                                                "[ExternalBridge] Failed to encrypt response: {}",
                                                e
                                            );
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
