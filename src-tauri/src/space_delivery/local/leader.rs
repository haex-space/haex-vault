//! Leader-side logic: buffering MLS messages, key packages, welcomes, and pending commits.
//!
//! These operations use `with_connection` directly (rather than `core::select`/`core::execute`)
//! because the buffer tables contain BLOB columns. The JSON-based core functions cannot
//! round-trip blob data faithfully (blobs become Null on read).

use std::collections::HashMap;
use std::sync::Arc;

use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use time::OffsetDateTime;
use tokio::sync::RwLock;

use crate::database::core::with_connection;
use crate::database::DbConnection;
use crate::peer_storage::endpoint::DeliveryConnectionHandler;
use rusqlite::OptionalExtension;
use uuid::Uuid;

use super::error::DeliveryError;
use super::protocol::{self, MlsMessageEntry, Notification, Request, Response};
use super::types::{ConnectedPeer, PeerClaim};

/// Map a `DatabaseError` into `DeliveryError::Database`.
fn map_db(e: crate::database::error::DatabaseError) -> DeliveryError {
    DeliveryError::Database {
        reason: e.to_string(),
    }
}

/// Store an MLS message in the leader buffer. Returns the auto-incremented ID.
pub fn store_message(
    db: &DbConnection,
    space_id: &str,
    sender_did: &str,
    message_type: &str,
    message_blob: &[u8],
) -> Result<i64, DeliveryError> {
    with_connection(db, |conn| {
        conn.execute(
            "INSERT INTO haex_local_delivery_messages_no_sync (space_id, sender_did, message_type, message_blob) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![space_id, sender_did, message_type, message_blob],
        ).map_err(|e| crate::database::error::DatabaseError::DatabaseError {
            reason: e.to_string(),
        })?;
        let id = conn.last_insert_rowid();
        Ok(id)
    })
    .map_err(map_db)
}

/// Fetch MLS messages after a given ID.
/// Returns tuples of (id, sender_did, message_type, message_blob, created_at).
pub fn fetch_messages(
    db: &DbConnection,
    space_id: &str,
    after_id: Option<i64>,
) -> Result<Vec<(i64, String, String, Vec<u8>, String)>, DeliveryError> {
    with_connection(db, |conn| {
        let after = after_id.unwrap_or(0);
        let mut stmt = conn
            .prepare(
                "SELECT id, sender_did, message_type, message_blob, created_at \
                 FROM haex_local_delivery_messages_no_sync \
                 WHERE space_id = ?1 AND id > ?2 \
                 ORDER BY id ASC",
            )
            .map_err(|e| crate::database::error::DatabaseError::DatabaseError {
                reason: e.to_string(),
            })?;

        let rows = stmt
            .query_map(rusqlite::params![space_id, after], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, Vec<u8>>(3)?,
                    row.get::<_, String>(4)?,
                ))
            })
            .map_err(|e| crate::database::error::DatabaseError::DatabaseError {
                reason: e.to_string(),
            })?;

        let mut result = Vec::new();
        for row in rows {
            result.push(row.map_err(|e| crate::database::error::DatabaseError::DatabaseError {
                reason: e.to_string(),
            })?);
        }
        Ok(result)
    })
    .map_err(map_db)
}

/// Store a key package for a target DID. Returns the generated UUID.
pub fn store_key_package(
    db: &DbConnection,
    space_id: &str,
    target_did: &str,
    package_blob: &[u8],
) -> Result<String, DeliveryError> {
    let id = Uuid::new_v4().to_string();
    with_connection(db, |conn| {
        conn.execute(
            "INSERT INTO haex_local_delivery_key_packages_no_sync (id, space_id, target_did, package_blob) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![id, space_id, target_did, package_blob],
        ).map_err(|e| crate::database::error::DatabaseError::DatabaseError {
            reason: e.to_string(),
        })?;
        Ok(id)
    })
    .map_err(map_db)
}

/// Fetch and consume (delete) one key package for a target DID.
/// Single-use per MLS spec: SELECT one, then DELETE it.
pub fn consume_key_package(
    db: &DbConnection,
    space_id: &str,
    target_did: &str,
) -> Result<Option<Vec<u8>>, DeliveryError> {
    with_connection(db, |conn| {
        // Find the oldest key package for this target
        let result: Option<(String, Vec<u8>)> = conn
            .query_row(
                "SELECT id, package_blob FROM haex_local_delivery_key_packages_no_sync \
                 WHERE space_id = ?1 AND target_did = ?2 \
                 ORDER BY created_at ASC LIMIT 1",
                rusqlite::params![space_id, target_did],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, Vec<u8>>(1)?)),
            )
            .optional()
            .map_err(|e: rusqlite::Error| crate::database::error::DatabaseError::DatabaseError {
                reason: e.to_string(),
            })?;

        match result {
            Some((id, blob)) => {
                conn.execute(
                    "DELETE FROM haex_local_delivery_key_packages_no_sync WHERE id = ?1",
                    rusqlite::params![id],
                )
                .map_err(|e| crate::database::error::DatabaseError::DatabaseError {
                    reason: e.to_string(),
                })?;
                Ok(Some(blob))
            }
            None => Ok(None),
        }
    })
    .map_err(map_db)
}

/// Store a welcome message for a recipient. Returns the generated UUID.
pub fn store_welcome(
    db: &DbConnection,
    space_id: &str,
    recipient_did: &str,
    welcome_blob: &[u8],
) -> Result<String, DeliveryError> {
    let id = Uuid::new_v4().to_string();
    with_connection(db, |conn| {
        conn.execute(
            "INSERT INTO haex_local_delivery_welcomes_no_sync (id, space_id, recipient_did, welcome_blob) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![id, space_id, recipient_did, welcome_blob],
        ).map_err(|e| crate::database::error::DatabaseError::DatabaseError {
            reason: e.to_string(),
        })?;
        Ok(id)
    })
    .map_err(map_db)
}

/// Fetch and mark consumed all welcomes for a recipient DID.
/// Returns the welcome blobs. Marks them as consumed so they are not returned again.
pub fn consume_welcomes(
    db: &DbConnection,
    space_id: &str,
    recipient_did: &str,
) -> Result<Vec<Vec<u8>>, DeliveryError> {
    with_connection(db, |conn| {
        // Fetch all unconsumed welcomes
        let mut stmt = conn
            .prepare(
                "SELECT id, welcome_blob FROM haex_local_delivery_welcomes_no_sync \
                 WHERE space_id = ?1 AND recipient_did = ?2 AND consumed = 0 \
                 ORDER BY created_at ASC",
            )
            .map_err(|e| crate::database::error::DatabaseError::DatabaseError {
                reason: e.to_string(),
            })?;

        let rows = stmt
            .query_map(rusqlite::params![space_id, recipient_did], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, Vec<u8>>(1)?))
            })
            .map_err(|e| crate::database::error::DatabaseError::DatabaseError {
                reason: e.to_string(),
            })?;

        let mut ids = Vec::new();
        let mut blobs = Vec::new();
        for row in rows {
            let (id, blob) =
                row.map_err(|e| crate::database::error::DatabaseError::DatabaseError {
                    reason: e.to_string(),
                })?;
            ids.push(id);
            blobs.push(blob);
        }

        // Mark all fetched welcomes as consumed
        for id in &ids {
            conn.execute(
                "UPDATE haex_local_delivery_welcomes_no_sync SET consumed = 1 WHERE id = ?1",
                rusqlite::params![id],
            )
            .map_err(|e| crate::database::error::DatabaseError::DatabaseError {
                reason: e.to_string(),
            })?;
        }

        Ok(blobs)
    })
    .map_err(map_db)
}

/// Store a pending commit (for crash recovery). Returns the generated UUID.
pub fn store_pending_commit(
    db: &DbConnection,
    space_id: &str,
    commit_blob: &[u8],
) -> Result<String, DeliveryError> {
    let id = Uuid::new_v4().to_string();
    with_connection(db, |conn| {
        conn.execute(
            "INSERT INTO haex_local_delivery_pending_commits_no_sync (id, space_id, commit_blob) VALUES (?1, ?2, ?3)",
            rusqlite::params![id, space_id, commit_blob],
        ).map_err(|e| crate::database::error::DatabaseError::DatabaseError {
            reason: e.to_string(),
        })?;
        Ok(id)
    })
    .map_err(map_db)
}

/// Clear all buffer tables for a space (called when leadership ends).
pub fn clear_buffers(db: &DbConnection, space_id: &str) -> Result<(), DeliveryError> {
    with_connection(db, |conn| {
        for table in &[
            "haex_local_delivery_messages_no_sync",
            "haex_local_delivery_key_packages_no_sync",
            "haex_local_delivery_welcomes_no_sync",
            "haex_local_delivery_pending_commits_no_sync",
        ] {
            conn.execute(
                &format!("DELETE FROM {table} WHERE space_id = ?1"),
                rusqlite::params![space_id],
            )
            .map_err(|e| crate::database::error::DatabaseError::DatabaseError {
                reason: e.to_string(),
            })?;
        }
        Ok(())
    })
    .map_err(map_db)
}

// ============================================================================
// Invite token management
// ============================================================================

/// A local invite token created by the admin.
#[derive(Debug, Clone)]
pub struct LocalInviteToken {
    pub id: String,
    pub space_id: String,
    /// If Some, only this DID can claim (contact invite). If None, anyone can (conference).
    pub target_did: Option<String>,
    /// Pre-created UCAN for contact invites (target_did is known).
    pub pre_created_ucan: Option<String>,
    pub capability: String,
    pub max_uses: u32,
    pub current_uses: u32,
    pub expires_at: OffsetDateTime,
    pub created_at: OffsetDateTime,
}

impl LocalInviteToken {
    pub fn is_valid(&self) -> bool {
        self.current_uses < self.max_uses && OffsetDateTime::now_utc() < self.expires_at
    }

    pub fn can_claim(&self, did: &str) -> bool {
        self.is_valid() && self.target_did.as_ref().map_or(true, |t| t == did)
    }
}

/// Create a contact invite token with a pre-created UCAN.
///
/// The target DID is known upfront, so the UCAN is created immediately.
pub fn create_contact_invite_token(
    state: &LeaderState,
    target_did: &str,
    capability: &str,
    expires_in_seconds: u64,
) -> Result<String, DeliveryError> {
    let admin = super::ucan::load_admin_identity(&state.db, &state.space_id)?;
    let ucan_token = super::ucan::create_delegated_ucan(
        &admin.did,
        &admin.private_key_base64,
        target_did,
        &state.space_id,
        capability,
        Some(&admin.root_ucan),
        86400 * 365, // 1 year expiry (admin can revoke via MLS)
    )?;

    let now = OffsetDateTime::now_utc();
    let expires_at = now + time::Duration::seconds(expires_in_seconds as i64);
    let token_id = Uuid::new_v4().to_string();

    let token = LocalInviteToken {
        id: token_id.clone(),
        space_id: state.space_id.clone(),
        target_did: Some(target_did.to_string()),
        pre_created_ucan: Some(ucan_token),
        capability: capability.to_string(),
        max_uses: 1,
        current_uses: 0,
        expires_at,
        created_at: now,
    };

    // Block on writing since invite_tokens uses tokio RwLock
    // but this is fine — called from async context via commands
    let tokens = state.invite_tokens.clone();
    tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current().block_on(async {
            tokens.write().await.push(token);
        })
    });

    Ok(token_id)
}

/// Create a conference invite token (no target DID, no pre-created UCAN).
///
/// The UCAN will be created at claim time when the claimer's DID is known.
pub async fn create_conference_invite_token(
    state: &LeaderState,
    capability: &str,
    max_uses: u32,
    expires_in_seconds: u64,
) -> Result<String, DeliveryError> {
    let now = OffsetDateTime::now_utc();
    let expires_at = now + time::Duration::seconds(expires_in_seconds as i64);
    let token_id = Uuid::new_v4().to_string();

    let token = LocalInviteToken {
        id: token_id.clone(),
        space_id: state.space_id.clone(),
        target_did: None,
        pre_created_ucan: None,
        capability: capability.to_string(),
        max_uses,
        current_uses: 0,
        expires_at,
        created_at: now,
    };

    state.invite_tokens.write().await.push(token);
    Ok(token_id)
}

/// Validate and consume an invite token. Returns (capability, Option<pre-created UCAN>).
pub async fn validate_and_consume_invite(
    state: &LeaderState,
    token_id: &str,
    claimer_did: &str,
) -> Result<(String, Option<String>), DeliveryError> {
    let mut tokens = state.invite_tokens.write().await;
    let token = tokens
        .iter_mut()
        .find(|t| t.id == token_id)
        .ok_or_else(|| DeliveryError::AccessDenied {
            reason: "Invalid invite token".to_string(),
        })?;

    if !token.can_claim(claimer_did) {
        return Err(DeliveryError::AccessDenied {
            reason: if !token.is_valid() {
                "Invite token expired or exhausted".to_string()
            } else {
                "This invite is not for your DID".to_string()
            },
        });
    }

    token.current_uses += 1;
    let capability = token.capability.clone();
    let pre_ucan = token.pre_created_ucan.clone();
    Ok((capability, pre_ucan))
}

// ============================================================================
// Leader connection handler
// ============================================================================

/// Base64-encode bytes to a string.
fn base64_encode(data: &[u8]) -> String {
    BASE64.encode(data)
}

/// Base64-decode a string to bytes.
fn base64_decode(s: &str) -> Result<Vec<u8>, String> {
    BASE64.decode(s).map_err(|e| format!("base64 decode error: {e}"))
}

/// State held by the leader for active delivery sessions.
pub struct LeaderState {
    /// Database connection
    pub db: DbConnection,
    /// Space ID this leader serves
    pub space_id: String,
    /// Currently connected peers (endpoint_id → peer info) — IN-MEMORY ONLY, never persisted
    pub connected_peers: Arc<RwLock<HashMap<String, ConnectedPeer>>>,
    /// Notification senders for connected peers (endpoint_id → sender)
    pub notification_senders:
        Arc<RwLock<HashMap<String, tokio::sync::mpsc::Sender<Notification>>>>,
    /// In-memory invite tokens (contact + conference)
    pub invite_tokens: Arc<RwLock<Vec<LocalInviteToken>>>,
}

/// Connection handler for the leader side of space delivery.
pub struct LeaderConnectionHandler {
    pub state: Arc<LeaderState>,
}

impl DeliveryConnectionHandler for LeaderConnectionHandler {
    fn handle_connection(
        &self,
        conn: iroh::endpoint::Connection,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + '_>> {
        Box::pin(self.handle_connection_inner(conn))
    }
}

impl LeaderConnectionHandler {
    async fn handle_connection_inner(&self, conn: iroh::endpoint::Connection) {
        let remote = conn.remote_id();
        let remote_str = remote.to_string();

        loop {
            match conn.accept_bi().await {
                Ok((send, mut recv)) => {
                    let state = self.state.clone();
                    let peer_endpoint_id = remote_str.clone();
                    tokio::spawn(async move {
                        if let Err(e) =
                            handle_delivery_stream(send, &mut recv, &state, &peer_endpoint_id).await
                        {
                            eprintln!(
                                "[SpaceDelivery] Stream error from {peer_endpoint_id}: {e}"
                            );
                        }
                    });
                }
                Err(_) => {
                    eprintln!("[SpaceDelivery] Connection from {remote_str} closed");
                    break;
                }
            }
        }

        // Clean up peer state on disconnect
        self.state
            .connected_peers
            .write()
            .await
            .remove(&remote_str);
        self.state
            .notification_senders
            .write()
            .await
            .remove(&remote_str);
    }
}

/// Look up the DID for a connected peer by endpoint_id.
async fn lookup_peer_did(
    state: &LeaderState,
    endpoint_id: &str,
) -> Result<String, DeliveryError> {
    let peers = state.connected_peers.read().await;
    peers
        .get(endpoint_id)
        .map(|p| p.did.clone())
        .ok_or_else(|| DeliveryError::ProtocolError {
            reason: "peer has not announced yet".to_string(),
        })
}

/// Process a single request/response exchange on a QUIC bidirectional stream.
async fn handle_delivery_stream(
    mut send: iroh::endpoint::SendStream,
    recv: &mut iroh::endpoint::RecvStream,
    state: &LeaderState,
    peer_endpoint_id: &str,
) -> Result<(), DeliveryError> {
    let request =
        protocol::read_request(recv)
            .await
            .map_err(|e| DeliveryError::ProtocolError {
                reason: e.to_string(),
            })?;

    let response = match request {
        Request::Announce {
            did,
            endpoint_id,
            label,
            claims,
        } => {
            let peer = ConnectedPeer {
                endpoint_id: endpoint_id.clone(),
                did,
                label,
                claims: claims
                    .unwrap_or_default()
                    .into_iter()
                    .map(|c| PeerClaim {
                        claim_type: c.claim_type,
                        value: c.value,
                    })
                    .collect(),
                connected_at: time::OffsetDateTime::now_utc()
                    .format(&time::format_description::well_known::Rfc3339)
                    .unwrap_or_default(),
            };
            state
                .connected_peers
                .write()
                .await
                .insert(peer_endpoint_id.to_string(), peer);
            Response::Ok
        }

        Request::MlsUploadKeyPackages {
            space_id: _,
            packages,
        } => match lookup_peer_did(state, peer_endpoint_id).await {
            Ok(sender_did) => {
                for pkg_b64 in &packages {
                    match base64_decode(pkg_b64) {
                        Ok(blob) => {
                            if let Err(e) =
                                store_key_package(&state.db, &state.space_id, &sender_did, &blob)
                            {
                                return send_response(
                                    &mut send,
                                    &Response::Error {
                                        message: e.to_string(),
                                    },
                                )
                                .await;
                            }
                        }
                        Err(msg) => {
                            return send_response(
                                &mut send,
                                &Response::Error { message: msg },
                            )
                            .await;
                        }
                    }
                }
                Response::Ok
            }
            Err(e) => Response::Error {
                message: e.to_string(),
            },
        },

        Request::MlsFetchKeyPackage {
            space_id: _,
            target_did,
        } => match consume_key_package(&state.db, &state.space_id, &target_did) {
            Ok(Some(blob)) => Response::KeyPackage {
                package: base64_encode(&blob),
            },
            Ok(None) => Response::Error {
                message: format!("no key package available for {target_did}"),
            },
            Err(e) => Response::Error {
                message: e.to_string(),
            },
        },

        Request::MlsSendMessage {
            space_id: _,
            message,
            message_type,
        } => match lookup_peer_did(state, peer_endpoint_id).await {
            Ok(sender_did) => match base64_decode(&message) {
                Ok(blob) => {
                    match store_message(
                        &state.db,
                        &state.space_id,
                        &sender_did,
                        &message_type,
                        &blob,
                    ) {
                        Ok(msg_id) => {
                            // Broadcast notification to all connected peers
                            let notification = Notification::Mls {
                                space_id: state.space_id.clone(),
                                message_type: message_type.clone(),
                            };
                            let senders = state.notification_senders.read().await;
                            for (_eid, tx) in senders.iter() {
                                let _ = tx.try_send(notification.clone());
                            }
                            Response::MessageStored { message_id: msg_id }
                        }
                        Err(e) => Response::Error {
                            message: e.to_string(),
                        },
                    }
                }
                Err(msg) => Response::Error { message: msg },
            },
            Err(e) => Response::Error {
                message: e.to_string(),
            },
        },

        Request::MlsFetchMessages {
            space_id: _,
            after_id,
        } => match fetch_messages(&state.db, &state.space_id, after_id) {
            Ok(rows) => Response::Messages {
                messages: rows
                    .into_iter()
                    .map(|(id, sender_did, msg_type, blob, created_at)| MlsMessageEntry {
                        id,
                        sender_did,
                        message_type: msg_type,
                        message: base64_encode(&blob),
                        created_at,
                    })
                    .collect(),
            },
            Err(e) => Response::Error {
                message: e.to_string(),
            },
        },

        Request::MlsSendWelcome {
            space_id: _,
            recipient_did,
            welcome,
        } => match base64_decode(&welcome) {
            Ok(blob) => match store_welcome(&state.db, &state.space_id, &recipient_did, &blob) {
                Ok(_id) => Response::Ok,
                Err(e) => Response::Error {
                    message: e.to_string(),
                },
            },
            Err(msg) => Response::Error { message: msg },
        },

        Request::MlsFetchWelcomes { space_id: _ } => {
            match lookup_peer_did(state, peer_endpoint_id).await {
                Ok(peer_did) => {
                    match consume_welcomes(&state.db, &state.space_id, &peer_did) {
                        Ok(blobs) => Response::Welcomes {
                            welcomes: blobs.iter().map(|b| base64_encode(b)).collect(),
                        },
                        Err(e) => Response::Error {
                            message: e.to_string(),
                        },
                    }
                }
                Err(e) => Response::Error {
                    message: e.to_string(),
                },
            }
        }

        Request::SyncPush { space_id, changes } => {
            if space_id != state.space_id {
                Response::Error {
                    message: format!("Wrong space: expected {}", state.space_id),
                }
            } else {
                match serde_json::from_value::<Vec<crate::crdt::scanner::LocalColumnChange>>(
                    changes,
                ) {
                    Ok(local_changes) if !local_changes.is_empty() => {
                        let batch_id = Uuid::new_v4().to_string();
                        let total = local_changes.len();

                        // Convert LocalColumnChange → RemoteColumnChange
                        let remote_changes: Vec<crate::crdt::commands::RemoteColumnChange> =
                            local_changes
                                .iter()
                                .enumerate()
                                .map(|(i, lc)| crate::crdt::commands::RemoteColumnChange {
                                    table_name: lc.table_name.clone(),
                                    row_pks: lc.row_pks.clone(),
                                    column_name: lc.column_name.clone(),
                                    hlc_timestamp: lc.hlc_timestamp.clone(),
                                    batch_id: batch_id.clone(),
                                    batch_seq: i + 1,
                                    batch_total: total,
                                    decrypted_value: lc.value.clone(),
                                })
                                .collect();

                        // Apply changes to leader's DB (no backend_info for local delivery)
                        match crate::crdt::commands::apply_remote_changes_to_db(
                            &state.db,
                            remote_changes,
                            None,
                        ) {
                            Ok(()) => {
                                // Collect affected table names for notification
                                let mut tables: Vec<String> =
                                    local_changes.iter().map(|c| c.table_name.clone()).collect();
                                tables.sort();
                                tables.dedup();

                                // Broadcast NOTIFY_SYNC to other connected peers
                                let notification = Notification::Sync {
                                    space_id: space_id.clone(),
                                    tables,
                                };
                                let senders = state.notification_senders.read().await;
                                for (endpoint_id, sender) in senders.iter() {
                                    if endpoint_id != peer_endpoint_id {
                                        let _ = sender.try_send(notification.clone());
                                    }
                                }

                                Response::Ok
                            }
                            Err(e) => Response::Error {
                                message: format!("Apply error: {e}"),
                            },
                        }
                    }
                    Ok(_) => Response::Ok, // Empty changes
                    Err(e) => Response::Error {
                        message: format!("Invalid changes format: {e}"),
                    },
                }
            }
        }

        Request::SyncPull {
            space_id,
            after_timestamp,
        } => {
            if space_id != state.space_id {
                Response::Error {
                    message: format!("Wrong space: expected {}", state.space_id),
                }
            } else {
                match crate::crdt::scanner::scan_all_crdt_tables_for_local_changes(
                    &state.db,
                    after_timestamp.as_deref(),
                    "leader",
                ) {
                    Ok(changes) => match serde_json::to_value(&changes) {
                        Ok(json) => Response::SyncChanges { changes: json },
                        Err(e) => Response::Error {
                            message: format!("Serialization error: {e}"),
                        },
                    },
                    Err(e) => Response::Error {
                        message: format!("Scan error: {e}"),
                    },
                }
            }
        }

        Request::ClaimInvite { .. } => {
            // TODO: implement invite claim handling in a later phase
            Response::Error {
                message: "ClaimInvite not yet implemented".to_string(),
            }
        }
    };

    send_response(&mut send, &response).await
}

/// Encode and send a response on the QUIC send stream, then finish.
async fn send_response(
    send: &mut iroh::endpoint::SendStream,
    response: &Response,
) -> Result<(), DeliveryError> {
    let bytes = protocol::encode(response).map_err(|e| DeliveryError::ProtocolError {
        reason: e.to_string(),
    })?;
    send.write_all(&bytes)
        .await
        .map_err(|e| DeliveryError::ProtocolError {
            reason: e.to_string(),
        })?;
    send.finish().map_err(|e| DeliveryError::ProtocolError {
        reason: e.to_string(),
    })?;
    Ok(())
}
