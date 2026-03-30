# Phase 6.2: Rust `space_delivery` Module

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Build the Rust `space_delivery/local/` module — protocol types, leader logic (buffering, housekeeping), peer logic (connect, send/receive), and Tauri commands. Refactor the shared iroh endpoint to support dual-ALPN dispatching.

**Architecture:** The iroh Endpoint gets a second ALPN (`haex-delivery/1`) alongside the existing `haex-peer/1`. The accept loop dispatches connections by ALPN to either `peer_storage` or `space_delivery`. The `space_delivery` module uses the same length-prefixed JSON wire format but with its own request/response types for MLS delivery and CRDT sync.

**Tech Stack:** Rust, iroh 0.96, tokio, serde, Tauri 2, rusqlite (via DbConnection)

**Corrections (applied after initial plan write):**

1. **Naming:** All `local_ds_*` references are now `local_delivery_*` (tables, settings, commands). No abbreviations in naming — spell out fully.
2. **Database access for `_no_sync` tables:** Use `core::select()` and `core::execute()` (not `with_connection` directly, and not `select_with_crdt`/`execute_with_crdt`). These are the existing functions for non-CRDT tables that handle parameter conversion and error types properly.
3. **Announce / peer identity:** Connected peer identities are stored **in-memory only** (HashMap in LeaderState). Never persisted to DB. When leader stops or peer disconnects, the data is gone. Rationale: transient connections (e.g. conference sharing) should leave no trace.
4. **Table names in SQL:** Use `haex_local_delivery_messages_no_sync`, `haex_local_delivery_key_packages_no_sync`, `haex_local_delivery_welcomes_no_sync`, `haex_local_delivery_pending_commits_no_sync`.

---

### Task 1: Create `space_delivery` module skeleton

Create the module structure without any logic yet — just empty files with module declarations.

**Files:**
- Create: `src-tauri/src/space_delivery/mod.rs`
- Create: `src-tauri/src/space_delivery/local/mod.rs`
- Create: `src-tauri/src/space_delivery/local/protocol.rs`
- Create: `src-tauri/src/space_delivery/local/types.rs`
- Create: `src-tauri/src/space_delivery/local/error.rs`
- Create: `src-tauri/src/space_delivery/local/leader.rs`
- Create: `src-tauri/src/space_delivery/local/peer.rs`
- Create: `src-tauri/src/space_delivery/local/election.rs`
- Create: `src-tauri/src/space_delivery/local/discovery.rs`
- Create: `src-tauri/src/space_delivery/local/housekeeping.rs`
- Create: `src-tauri/src/space_delivery/local/commands.rs`
- Modify: `src-tauri/src/lib.rs` — add `mod space_delivery;`

**Step 1: Create directory structure**

```bash
mkdir -p src-tauri/src/space_delivery/local
```

**Step 2: Create `src-tauri/src/space_delivery/mod.rs`**

```rust
pub mod local;
```

**Step 3: Create `src-tauri/src/space_delivery/local/mod.rs`**

```rust
pub mod commands;
pub mod discovery;
pub mod election;
pub mod error;
pub mod housekeeping;
pub mod leader;
pub mod peer;
pub mod protocol;
pub mod types;

pub use commands::*;
```

**Step 4: Create empty submodule files**

Each file starts with a module doc comment and minimal content:

`protocol.rs`:
```rust
//! Space delivery protocol types over QUIC streams.
//!
//! Request/response protocol for MLS delivery and CRDT sync in local spaces.

/// ALPN protocol identifier for space delivery
pub const ALPN: &[u8] = b"haex-delivery/1";
```

`types.rs`:
```rust
//! Shared types for local space delivery.
```

`error.rs`:
```rust
//! Error types for local space delivery.

use serde::Serialize;

#[derive(Debug, thiserror::Error, Serialize)]
pub enum DeliveryError {
    #[error("Not a leader for this space")]
    NotLeader,
    #[error("No leader found for space {space_id}")]
    NoLeader { space_id: String },
    #[error("Space not found: {space_id}")]
    SpaceNotFound { space_id: String },
    #[error("Access denied: {reason}")]
    AccessDenied { reason: String },
    #[error("Connection failed: {reason}")]
    ConnectionFailed { reason: String },
    #[error("Protocol error: {reason}")]
    ProtocolError { reason: String },
    #[error("Database error: {reason}")]
    Database { reason: String },
    #[error("MLS error: {reason}")]
    Mls { reason: String },
}
```

`leader.rs`:
```rust
//! Leader-side logic: buffering MLS messages, key packages, welcomes, and pending commits.
```

`peer.rs`:
```rust
//! Peer-side logic: connecting to leader, sending/receiving messages.
```

`election.rs`:
```rust
//! Priority-based leader election and graceful handoff.
```

`discovery.rs`:
```rust
//! mDNS discovery combined with CRDT-based leader priorities.
```

`housekeeping.rs`:
```rust
//! Cleanup routines for expired buffer data (messages, key packages, welcomes, pending commits).
```

`commands.rs`:
```rust
//! Tauri commands for the local delivery service.

use tauri::State;
use crate::AppState;
use super::error::DeliveryError;
```

**Step 5: Register module in lib.rs**

Add `mod space_delivery;` at the top of `src-tauri/src/lib.rs` alongside the other module declarations (after `mod peer_storage;`).

**Step 6: Verify it compiles**

Run: `cd /home/haex/Projekte/haex-vault/src-tauri && cargo check 2>&1 | tail -5`

Expected: Compiles with warnings about unused imports/dead code (that's fine).

**Step 7: Commit**

```
feat: add space_delivery module skeleton
```

---

### Task 2: Define the protocol types

Define all request/response types for the space delivery protocol. This uses the same wire format (length-prefixed JSON over QUIC) as peer_storage but with different message types.

**Files:**
- Modify: `src-tauri/src/space_delivery/local/protocol.rs`
- Modify: `src-tauri/src/space_delivery/local/types.rs`

**Step 1: Define protocol.rs**

```rust
//! Space delivery protocol types over QUIC streams.
//!
//! Request/response protocol for MLS delivery and CRDT sync in local spaces.

use serde::{Deserialize, Serialize};

/// ALPN protocol identifier for space delivery
pub const ALPN: &[u8] = b"haex-delivery/1";

/// Maximum request size (10 MB — CRDT changes can be large)
const MAX_REQUEST_SIZE: usize = 10 * 1024 * 1024;

/// Maximum response size (10 MB)
const MAX_RESPONSE_SIZE: usize = 10 * 1024 * 1024;

// ============================================================================
// Request types
// ============================================================================

/// All request types for the space delivery protocol.
/// Tagged by `op` field for JSON serialization.
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Request {
    // -- MLS Delivery --
    /// Upload key packages for a DID in a space
    MlsUploadKeyPackages {
        space_id: String,
        /// Base64-encoded key packages
        packages: Vec<String>,
    },
    /// Fetch a key package for a target DID
    MlsFetchKeyPackage {
        space_id: String,
        target_did: String,
    },
    /// Send an MLS message (commit, proposal, application)
    MlsSendMessage {
        space_id: String,
        /// Base64-encoded MLS message
        message: String,
        message_type: String,
    },
    /// Fetch MLS messages after a given ID
    MlsFetchMessages {
        space_id: String,
        after_id: Option<i64>,
    },
    /// Send a welcome message to a specific recipient
    MlsSendWelcome {
        space_id: String,
        recipient_did: String,
        /// Base64-encoded welcome message
        welcome: String,
    },
    /// Fetch welcome messages for the caller
    MlsFetchWelcomes {
        space_id: String,
    },

    // -- CRDT Sync --
    /// Push CRDT changes to the leader
    SyncPush {
        space_id: String,
        /// JSON-serialized CRDT changes (same format as server push)
        changes: serde_json::Value,
    },
    /// Pull CRDT changes from the leader
    SyncPull {
        space_id: String,
        after_timestamp: Option<String>,
    },

    // -- Identity --
    /// Announce identity to the leader (sent on connect)
    Announce {
        did: String,
        endpoint_id: String,
        /// Optional claims the peer chooses to share
        label: Option<String>,
        claims: Option<Vec<IdentityClaim>>,
    },
}

// ============================================================================
// Response types
// ============================================================================

/// All response types for the space delivery protocol.
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Response {
    /// Success with no data
    Ok,
    /// MLS message stored, returns ID
    MessageStored { message_id: i64 },
    /// Single key package
    KeyPackage {
        /// Base64-encoded
        package: String,
    },
    /// List of MLS messages
    Messages { messages: Vec<MlsMessageEntry> },
    /// List of welcome messages
    Welcomes {
        /// Base64-encoded welcomes
        welcomes: Vec<String>,
    },
    /// CRDT sync changes
    SyncChanges { changes: serde_json::Value },
    /// Error response
    Error { message: String },
}

// ============================================================================
// Notification types (pushed from leader to peers over long-lived stream)
// ============================================================================

/// Notifications pushed from leader to connected peers.
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum Notification {
    /// New sync data available
    Sync { space_id: String, tables: Vec<String> },
    /// New MLS message available
    Mls { space_id: String, message_type: String },
    /// New invite available
    Invite { space_id: String, invite_id: String },
    /// Leader is shutting down (handoff or stop)
    LeaderStopping,
}

// ============================================================================
// Supporting types
// ============================================================================

/// An MLS message stored in the leader's buffer
#[derive(Debug, Serialize, Deserialize)]
pub struct MlsMessageEntry {
    pub id: i64,
    pub sender_did: String,
    pub message_type: String,
    /// Base64-encoded
    pub message: String,
    pub created_at: String,
}

/// An identity claim shared by a peer
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdentityClaim {
    pub claim_type: String,
    pub value: String,
}

// ============================================================================
// Wire format helpers (reuse pattern from peer_storage)
// ============================================================================

use crate::peer_storage::protocol::PeerProtocolError;

/// Encode a message to bytes (length-prefixed JSON)
pub fn encode<T: Serialize>(msg: &T) -> Result<Vec<u8>, serde_json::Error> {
    let json = serde_json::to_vec(msg)?;
    let len = (json.len() as u32).to_be_bytes();
    let mut buf = Vec::with_capacity(4 + json.len());
    buf.extend_from_slice(&len);
    buf.extend_from_slice(&json);
    Ok(buf)
}

/// Read a request from a QUIC receive stream
pub async fn read_request(
    recv: &mut iroh::endpoint::RecvStream,
) -> Result<Request, PeerProtocolError> {
    crate::peer_storage::protocol::read_message(recv, MAX_REQUEST_SIZE).await
}

/// Read a response from a QUIC receive stream
pub async fn read_response(
    recv: &mut iroh::endpoint::RecvStream,
) -> Result<Response, PeerProtocolError> {
    crate::peer_storage::protocol::read_message(recv, MAX_RESPONSE_SIZE).await
}

/// Read a notification from a QUIC receive stream
pub async fn read_notification(
    recv: &mut iroh::endpoint::RecvStream,
) -> Result<Notification, PeerProtocolError> {
    crate::peer_storage::protocol::read_message(recv, MAX_RESPONSE_SIZE).await
}
```

**Step 2: Define types.rs**

```rust
//! Shared types for local space delivery.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Information about a connected peer (visible to admin)
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ConnectedPeer {
    pub endpoint_id: String,
    pub did: String,
    pub label: Option<String>,
    pub claims: Vec<PeerClaim>,
    pub connected_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PeerClaim {
    pub claim_type: String,
    pub value: String,
}

/// Status of the local delivery service
#[derive(Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct DeliveryStatus {
    pub is_leader: bool,
    pub space_id: Option<String>,
    pub connected_peers: Vec<ConnectedPeer>,
    pub buffered_messages: u32,
    pub buffered_welcomes: u32,
    pub buffered_key_packages: u32,
}

/// Information about the current leader for a space
#[derive(Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct LeaderInfo {
    pub endpoint_id: String,
    pub priority: i32,
    pub space_id: String,
}
```

**Step 3: Verify compilation**

Run: `cd /home/haex/Projekte/haex-vault/src-tauri && cargo check 2>&1 | tail -5`

**Step 4: Commit**

```
feat: define space delivery protocol and types
```

---

### Task 3: Refactor iroh endpoint for dual-ALPN dispatching

The iroh endpoint currently only registers `haex-peer/1`. We need to register both ALPNs and dispatch connections based on the negotiated ALPN.

**Files:**
- Modify: `src-tauri/src/peer_storage/endpoint.rs`
- Modify: `src-tauri/src/space_delivery/local/leader.rs`

**Step 1: Register both ALPNs in endpoint start**

In `src-tauri/src/peer_storage/endpoint.rs`, in the `start()` method, change line 158:

```rust
// Before:
.alpns(vec![ALPN.to_vec()])

// After:
.alpns(vec![
    ALPN.to_vec(),
    crate::space_delivery::local::protocol::ALPN.to_vec(),
])
```

**Step 2: Add delivery state to PeerEndpoint**

Add a new field to `PeerState` (or a separate shared state) for delivery connections. In `PeerEndpoint`, add a callback/handler reference that the space_delivery module can register.

In `endpoint.rs`, add to `PeerState`:

```rust
/// Handler for space delivery connections (set by space_delivery module)
pub delivery_handler: Option<Arc<dyn DeliveryConnectionHandler>>,
```

Define the trait above `PeerState`:

```rust
/// Trait for handling space delivery connections. Implemented by space_delivery module.
#[async_trait::async_trait]
pub trait DeliveryConnectionHandler: Send + Sync {
    async fn handle_connection(&self, conn: iroh::endpoint::Connection);
}
```

Add `async-trait` — check if already in Cargo.toml, if not note that we need it.

**Step 3: Dispatch in accept_loop by ALPN**

In `accept_loop`, after accepting a connection, check the ALPN:

```rust
async fn accept_loop(endpoint: Endpoint, state: Arc<RwLock<PeerState>>) {
    while let Some(incoming) = endpoint.accept().await {
        let state = state.clone();
        tokio::spawn(async move {
            match incoming.await {
                Ok(conn) => {
                    let alpn = conn.alpn();
                    let alpn_bytes = alpn.as_ref();

                    if alpn_bytes == crate::peer_storage::protocol::ALPN {
                        // Existing peer_storage handling (unchanged)
                        let remote = conn.remote_id();
                        let remote_str = remote.to_string();
                        let allowed_spaces = {
                            let s = state.read().await;
                            s.allowed_peers.get(&remote_str).cloned()
                        };
                        match allowed_spaces {
                            Some(spaces) if !spaces.is_empty() => {
                                eprintln!("[PeerStorage] Accepted connection from {remote} (access to {} spaces)", spaces.len());
                                handle_connection(conn, state).await;
                            }
                            _ => {
                                eprintln!("[PeerStorage] Rejected connection from {remote}: not registered in any shared space");
                            }
                        }
                    } else if alpn_bytes == crate::space_delivery::local::protocol::ALPN {
                        // Dispatch to space delivery handler
                        let handler = {
                            let s = state.read().await;
                            s.delivery_handler.clone()
                        };
                        if let Some(handler) = handler {
                            eprintln!("[SpaceDelivery] Accepted delivery connection from {}", conn.remote_id());
                            handler.handle_connection(conn).await;
                        } else {
                            eprintln!("[SpaceDelivery] Rejected connection: no delivery handler registered");
                        }
                    } else {
                        eprintln!("[Endpoint] Rejected connection with unknown ALPN: {:?}", String::from_utf8_lossy(alpn_bytes));
                    }
                }
                Err(e) => {
                    eprintln!("[Endpoint] Failed to accept connection: {e}");
                }
            }
        });
    }
}
```

**Step 4: Add method to register delivery handler**

In `PeerEndpoint`, add:

```rust
/// Register a handler for space delivery connections.
pub async fn set_delivery_handler(&self, handler: Arc<dyn DeliveryConnectionHandler>) {
    self.state.write().await.delivery_handler = Some(handler);
}
```

**Step 5: Verify compilation**

Run: `cd /home/haex/Projekte/haex-vault/src-tauri && cargo check 2>&1 | tail -10`

**Step 6: Commit**

```
feat: dual-ALPN dispatching for peer_storage and space_delivery
```

---

### Task 4: Implement leader buffer operations

The leader stores MLS messages, key packages, welcomes, and pending commits in the `_no_sync` tables. These operations are the core of the delivery service.

**Files:**
- Modify: `src-tauri/src/space_delivery/local/leader.rs`

**Step 1: Implement leader buffer CRUD**

```rust
//! Leader-side logic: buffering MLS messages, key packages, welcomes, and pending commits.

use crate::database::core::{execute_with_crdt, select_with_crdt};
use crate::database::DbConnection;
use crate::database::with_connection;
use crate::crdt::hlc::HlcService;
use super::error::DeliveryError;
use std::sync::Mutex;

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
            "INSERT INTO haex_local_ds_messages_no_sync (space_id, sender_did, message_type, message_blob) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![space_id, sender_did, message_type, message_blob],
        ).map_err(|e| crate::database::error::DatabaseError::QueryError { reason: e.to_string() })?;
        Ok(conn.last_insert_rowid())
    }).map_err(|e| DeliveryError::Database { reason: e.to_string() })
}

/// Fetch MLS messages after a given ID.
pub fn fetch_messages(
    db: &DbConnection,
    space_id: &str,
    after_id: Option<i64>,
) -> Result<Vec<(i64, String, String, Vec<u8>, String)>, DeliveryError> {
    with_connection(db, |conn| {
        let after = after_id.unwrap_or(0);
        let mut stmt = conn.prepare(
            "SELECT id, sender_did, message_type, message_blob, created_at \
             FROM haex_local_ds_messages_no_sync \
             WHERE space_id = ?1 AND id > ?2 ORDER BY id ASC"
        ).map_err(|e| crate::database::error::DatabaseError::QueryError { reason: e.to_string() })?;

        let rows = stmt.query_map(rusqlite::params![space_id, after], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Vec<u8>>(3)?,
                row.get::<_, String>(4)?,
            ))
        }).map_err(|e| crate::database::error::DatabaseError::QueryError { reason: e.to_string() })?;

        let mut result = Vec::new();
        for row in rows {
            result.push(row.map_err(|e| crate::database::error::DatabaseError::QueryError { reason: e.to_string() })?);
        }
        Ok(result)
    }).map_err(|e| DeliveryError::Database { reason: e.to_string() })
}

/// Store a key package for a target DID.
pub fn store_key_package(
    db: &DbConnection,
    space_id: &str,
    target_did: &str,
    package_blob: &[u8],
) -> Result<String, DeliveryError> {
    let id = uuid::Uuid::new_v4().to_string();
    with_connection(db, |conn| {
        conn.execute(
            "INSERT INTO haex_local_ds_key_packages_no_sync (id, space_id, target_did, package_blob) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![id, space_id, target_did, package_blob],
        ).map_err(|e| crate::database::error::DatabaseError::QueryError { reason: e.to_string() })?;
        Ok(id)
    }).map_err(|e| DeliveryError::Database { reason: e.to_string() })
}

/// Fetch and consume (delete) one key package for a target DID. Single-use per MLS spec.
pub fn consume_key_package(
    db: &DbConnection,
    space_id: &str,
    target_did: &str,
) -> Result<Option<Vec<u8>>, DeliveryError> {
    with_connection(db, |conn| {
        let result: Option<(String, Vec<u8>)> = conn.query_row(
            "SELECT id, package_blob FROM haex_local_ds_key_packages_no_sync \
             WHERE space_id = ?1 AND target_did = ?2 LIMIT 1",
            rusqlite::params![space_id, target_did],
            |row| Ok((row.get(0)?, row.get(1)?)),
        ).optional()
        .map_err(|e| crate::database::error::DatabaseError::QueryError { reason: e.to_string() })?;

        if let Some((id, blob)) = result {
            conn.execute(
                "DELETE FROM haex_local_ds_key_packages_no_sync WHERE id = ?1",
                rusqlite::params![id],
            ).map_err(|e| crate::database::error::DatabaseError::QueryError { reason: e.to_string() })?;
            Ok(Some(blob))
        } else {
            Ok(None)
        }
    }).map_err(|e| DeliveryError::Database { reason: e.to_string() })
}

/// Store a welcome message for a recipient.
pub fn store_welcome(
    db: &DbConnection,
    space_id: &str,
    recipient_did: &str,
    welcome_blob: &[u8],
) -> Result<String, DeliveryError> {
    let id = uuid::Uuid::new_v4().to_string();
    with_connection(db, |conn| {
        conn.execute(
            "INSERT INTO haex_local_ds_welcomes_no_sync (id, space_id, recipient_did, welcome_blob) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![id, space_id, recipient_did, welcome_blob],
        ).map_err(|e| crate::database::error::DatabaseError::QueryError { reason: e.to_string() })?;
        Ok(id)
    }).map_err(|e| DeliveryError::Database { reason: e.to_string() })
}

/// Fetch and mark consumed all welcomes for a recipient DID.
pub fn consume_welcomes(
    db: &DbConnection,
    space_id: &str,
    recipient_did: &str,
) -> Result<Vec<Vec<u8>>, DeliveryError> {
    with_connection(db, |conn| {
        let mut stmt = conn.prepare(
            "SELECT id, welcome_blob FROM haex_local_ds_welcomes_no_sync \
             WHERE space_id = ?1 AND recipient_did = ?2 AND consumed = 0"
        ).map_err(|e| crate::database::error::DatabaseError::QueryError { reason: e.to_string() })?;

        let rows: Vec<(String, Vec<u8>)> = stmt.query_map(
            rusqlite::params![space_id, recipient_did],
            |row| Ok((row.get(0)?, row.get(1)?)),
        ).map_err(|e| crate::database::error::DatabaseError::QueryError { reason: e.to_string() })?
        .filter_map(|r| r.ok())
        .collect();

        // Mark consumed
        for (id, _) in &rows {
            conn.execute(
                "UPDATE haex_local_ds_welcomes_no_sync SET consumed = 1 WHERE id = ?1",
                rusqlite::params![id],
            ).map_err(|e| crate::database::error::DatabaseError::QueryError { reason: e.to_string() })?;
        }

        Ok(rows.into_iter().map(|(_, blob)| blob).collect())
    }).map_err(|e| DeliveryError::Database { reason: e.to_string() })
}

/// Store a pending commit (for crash recovery).
pub fn store_pending_commit(
    db: &DbConnection,
    space_id: &str,
    commit_blob: &[u8],
) -> Result<String, DeliveryError> {
    let id = uuid::Uuid::new_v4().to_string();
    with_connection(db, |conn| {
        conn.execute(
            "INSERT INTO haex_local_ds_pending_commits_no_sync (id, space_id, commit_blob) VALUES (?1, ?2, ?3)",
            rusqlite::params![id, space_id, commit_blob],
        ).map_err(|e| crate::database::error::DatabaseError::QueryError { reason: e.to_string() })?;
        Ok(id)
    }).map_err(|e| DeliveryError::Database { reason: e.to_string() })
}

/// Clear all buffer tables for a space (called when leadership ends).
pub fn clear_buffers(db: &DbConnection, space_id: &str) -> Result<(), DeliveryError> {
    with_connection(db, |conn| {
        conn.execute("DELETE FROM haex_local_ds_messages_no_sync WHERE space_id = ?1", rusqlite::params![space_id])
            .map_err(|e| crate::database::error::DatabaseError::QueryError { reason: e.to_string() })?;
        conn.execute("DELETE FROM haex_local_ds_key_packages_no_sync WHERE space_id = ?1", rusqlite::params![space_id])
            .map_err(|e| crate::database::error::DatabaseError::QueryError { reason: e.to_string() })?;
        conn.execute("DELETE FROM haex_local_ds_welcomes_no_sync WHERE space_id = ?1", rusqlite::params![space_id])
            .map_err(|e| crate::database::error::DatabaseError::QueryError { reason: e.to_string() })?;
        conn.execute("DELETE FROM haex_local_ds_pending_commits_no_sync WHERE space_id = ?1", rusqlite::params![space_id])
            .map_err(|e| crate::database::error::DatabaseError::QueryError { reason: e.to_string() })?;
        Ok(())
    }).map_err(|e| DeliveryError::Database { reason: e.to_string() })
}
```

NOTE: Use `core::select()` and `core::execute()` for all `_no_sync` table access (not `select_with_crdt`/`execute_with_crdt` — those add CRDT overhead for non-CRDT tables, and not `with_connection` directly — that skips parameter conversion). The code in Step 1 above uses `with_connection` for illustration but the implementer MUST use `core::select(sql, params, &db)` and `core::execute(sql, params, &db)` instead, matching the existing pattern in `database/mod.rs`. Table names: `haex_local_delivery_messages_no_sync`, `haex_local_delivery_key_packages_no_sync`, `haex_local_delivery_welcomes_no_sync`, `haex_local_delivery_pending_commits_no_sync`.

**Step 2: Verify compilation**

Run: `cd /home/haex/Projekte/haex-vault/src-tauri && cargo check 2>&1 | tail -10`

Note: `uuid` crate may need to be added to Cargo.toml. Check if it's already there, if not add `uuid = { version = "1", features = ["v4"] }`.

**Step 3: Commit**

```
feat: implement leader buffer operations for local delivery
```

---

### Task 5: Implement housekeeping

Cleanup routines that delete expired buffer data based on configurable TTLs.

**Files:**
- Modify: `src-tauri/src/space_delivery/local/housekeeping.rs`

**Step 1: Implement housekeeping**

```rust
//! Cleanup routines for expired buffer data.

use crate::database::DbConnection;
use crate::database::with_connection;
use super::error::DeliveryError;

/// Default TTL values (used when vault settings are not configured)
pub const DEFAULT_MESSAGE_TTL_DAYS: i64 = 7;
pub const DEFAULT_KEY_PACKAGE_TTL_HOURS: i64 = 24;
pub const DEFAULT_WELCOME_TTL_DAYS: i64 = 7;
pub const DEFAULT_PENDING_COMMIT_TTL_HOURS: i64 = 1;

/// Run all cleanup routines for a space.
pub fn cleanup_space(
    db: &DbConnection,
    space_id: &str,
    message_ttl_days: i64,
    key_package_ttl_hours: i64,
    welcome_ttl_days: i64,
    pending_commit_ttl_hours: i64,
) -> Result<CleanupStats, DeliveryError> {
    with_connection(db, |conn| {
        let messages = conn.execute(
            "DELETE FROM haex_local_ds_messages_no_sync \
             WHERE space_id = ?1 AND created_at < datetime('now', ?2)",
            rusqlite::params![space_id, format!("-{message_ttl_days} days")],
        ).unwrap_or(0);

        let key_packages = conn.execute(
            "DELETE FROM haex_local_ds_key_packages_no_sync \
             WHERE space_id = ?1 AND created_at < datetime('now', ?2)",
            rusqlite::params![space_id, format!("-{key_package_ttl_hours} hours")],
        ).unwrap_or(0);

        let welcomes = conn.execute(
            "DELETE FROM haex_local_ds_welcomes_no_sync \
             WHERE space_id = ?1 AND (consumed = 1 OR created_at < datetime('now', ?2))",
            rusqlite::params![space_id, format!("-{welcome_ttl_days} days")],
        ).unwrap_or(0);

        let pending_commits = conn.execute(
            "DELETE FROM haex_local_ds_pending_commits_no_sync \
             WHERE space_id = ?1 AND created_at < datetime('now', ?2)",
            rusqlite::params![space_id, format!("-{pending_commit_ttl_hours} hours")],
        ).unwrap_or(0);

        Ok(CleanupStats {
            messages_deleted: messages,
            key_packages_deleted: key_packages,
            welcomes_deleted: welcomes,
            pending_commits_deleted: pending_commits,
        })
    }).map_err(|e| DeliveryError::Database { reason: e.to_string() })
}

#[derive(Debug, Default)]
pub struct CleanupStats {
    pub messages_deleted: usize,
    pub key_packages_deleted: usize,
    pub welcomes_deleted: usize,
    pub pending_commits_deleted: usize,
}
```

**Step 2: Verify + Commit**

```
feat: implement delivery buffer housekeeping
```

---

### Task 6: Implement leader connection handler

The leader handles incoming QUIC connections from peers, dispatching requests to the buffer operations.

**Files:**
- Modify: `src-tauri/src/space_delivery/local/leader.rs` (add connection handler at the bottom)

**Step 1: Add handler struct and trait implementation**

At the bottom of `leader.rs`, add:

```rust
use std::sync::Arc;
use std::collections::HashMap;
use tokio::sync::RwLock;
use crate::database::DbConnection;
use crate::peer_storage::endpoint::DeliveryConnectionHandler;
use super::protocol::{self, Request, Response, Notification};
use super::types::ConnectedPeer;

/// State held by the leader for active delivery sessions.
pub struct LeaderState {
    /// Database connection
    pub db: DbConnection,
    /// Space ID this leader serves
    pub space_id: String,
    /// Currently connected peers (endpoint_id → peer info)
    pub connected_peers: Arc<RwLock<HashMap<String, ConnectedPeer>>>,
    /// Notification senders for connected peers (endpoint_id → sender)
    pub notification_senders: Arc<RwLock<HashMap<String, tokio::sync::mpsc::Sender<Notification>>>>,
}

/// The delivery connection handler registered with PeerEndpoint.
pub struct LeaderConnectionHandler {
    pub state: Arc<LeaderState>,
}

#[async_trait::async_trait]
impl DeliveryConnectionHandler for LeaderConnectionHandler {
    async fn handle_connection(&self, conn: iroh::endpoint::Connection) {
        let remote = conn.remote_id();
        let remote_str = remote.to_string();
        let state = self.state.clone();

        // Accept bidirectional streams for request/response
        loop {
            match conn.accept_bi().await {
                Ok((send, mut recv)) => {
                    let state = state.clone();
                    let remote_str = remote_str.clone();
                    tokio::spawn(async move {
                        if let Err(e) = handle_delivery_stream(send, &mut recv, &state, &remote_str).await {
                            eprintln!("[SpaceDelivery] Stream error from {remote_str}: {e}");
                        }
                    });
                }
                Err(_) => {
                    // Connection closed — remove from connected peers
                    state.connected_peers.write().await.remove(&remote_str);
                    state.notification_senders.write().await.remove(&remote_str);
                    eprintln!("[SpaceDelivery] Peer {remote_str} disconnected");
                    break;
                }
            }
        }
    }
}

async fn handle_delivery_stream(
    mut send: iroh::endpoint::SendStream,
    recv: &mut iroh::endpoint::RecvStream,
    state: &LeaderState,
    _sender_endpoint_id: &str,
) -> Result<(), super::error::DeliveryError> {
    let request = protocol::read_request(recv)
        .await
        .map_err(|e| super::error::DeliveryError::ProtocolError { reason: e.to_string() })?;

    let response = match request {
        Request::MlsUploadKeyPackages { space_id, packages } => {
            if space_id != state.space_id {
                Response::Error { message: format!("Wrong space: expected {}", state.space_id) }
            } else {
                for pkg_b64 in &packages {
                    match base64_decode(pkg_b64) {
                        Ok(blob) => { let _ = store_key_package(&state.db, &space_id, "", &blob); }
                        Err(e) => return Ok(send_response(&mut send, Response::Error { message: e }).await?),
                    }
                }
                Response::Ok
            }
        }
        Request::MlsFetchKeyPackage { space_id, target_did } => {
            match consume_key_package(&state.db, &space_id, &target_did)? {
                Some(blob) => Response::KeyPackage { package: base64_encode(&blob) },
                None => Response::Error { message: "No key package available".into() },
            }
        }
        Request::MlsSendMessage { space_id, message, message_type } => {
            match base64_decode(&message) {
                Ok(blob) => {
                    let msg_id = store_message(&state.db, &space_id, "", &message_type, &blob)?;
                    // Notify connected peers
                    let senders = state.notification_senders.read().await;
                    for (_, sender) in senders.iter() {
                        let _ = sender.try_send(Notification::Mls {
                            space_id: space_id.clone(),
                            message_type: message_type.clone(),
                        });
                    }
                    Response::MessageStored { message_id: msg_id }
                }
                Err(e) => Response::Error { message: e },
            }
        }
        Request::MlsFetchMessages { space_id, after_id } => {
            let msgs = fetch_messages(&state.db, &space_id, after_id)?;
            let entries: Vec<protocol::MlsMessageEntry> = msgs.into_iter().map(|(id, sender_did, msg_type, blob, created_at)| {
                protocol::MlsMessageEntry {
                    id,
                    sender_did,
                    message_type: msg_type,
                    message: base64_encode(&blob),
                    created_at,
                }
            }).collect();
            Response::Messages { messages: entries }
        }
        Request::MlsSendWelcome { space_id, recipient_did, welcome } => {
            match base64_decode(&welcome) {
                Ok(blob) => {
                    store_welcome(&state.db, &space_id, &recipient_did, &blob)?;
                    Response::Ok
                }
                Err(e) => Response::Error { message: e },
            }
        }
        Request::MlsFetchWelcomes { space_id } => {
            // Use sender's DID (needs to be passed via Announce first)
            // For now, return empty — will be wired up with Announce identity
            Response::Welcomes { welcomes: vec![] }
        }
        Request::SyncPush { space_id, changes } => {
            // TODO: Phase 6.4 — integrate with CRDT sync engine
            Response::Ok
        }
        Request::SyncPull { space_id, after_timestamp } => {
            // TODO: Phase 6.4 — integrate with CRDT sync engine
            Response::SyncChanges { changes: serde_json::Value::Array(vec![]) }
        }
        Request::Announce { did, endpoint_id, label, claims } => {
            let peer = ConnectedPeer {
                endpoint_id: endpoint_id.clone(),
                did,
                label,
                claims: claims.unwrap_or_default().into_iter().map(|c| super::types::PeerClaim {
                    claim_type: c.claim_type,
                    value: c.value,
                }).collect(),
                connected_at: chrono::Utc::now().to_rfc3339(),
            };
            state.connected_peers.write().await.insert(endpoint_id, peer);
            Response::Ok
        }
    };

    send_response(&mut send, response).await
}

async fn send_response(
    send: &mut iroh::endpoint::SendStream,
    response: Response,
) -> Result<(), super::error::DeliveryError> {
    let bytes = protocol::encode(&response)
        .map_err(|e| super::error::DeliveryError::ProtocolError { reason: e.to_string() })?;
    send.write_all(&bytes).await
        .map_err(|e| super::error::DeliveryError::ProtocolError { reason: e.to_string() })?;
    send.finish()
        .map_err(|e| super::error::DeliveryError::ProtocolError { reason: e.to_string() })?;
    Ok(())
}

fn base64_encode(data: &[u8]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(data)
}

fn base64_decode(s: &str) -> Result<Vec<u8>, String> {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.decode(s).map_err(|e| format!("Invalid base64: {e}"))
}
```

Note: Check if `base64`, `chrono`, and `async-trait` crates are already in Cargo.toml. If not, they need to be added.

**Step 2: Verify + Commit**

```
feat: implement leader connection handler for space delivery
```

---

### Task 7: Implement Tauri commands

The frontend needs commands to start/stop leader mode, get status, and connect as peer.

**Files:**
- Modify: `src-tauri/src/space_delivery/local/commands.rs`
- Modify: `src-tauri/src/lib.rs` (register commands)

**Step 1: Implement commands**

```rust
//! Tauri commands for the local delivery service.

use std::collections::HashMap;
use std::sync::Arc;
use tauri::State;
use tokio::sync::RwLock;
use crate::AppState;
use super::error::DeliveryError;
use super::leader::{LeaderState, LeaderConnectionHandler};
use super::types::{DeliveryStatus, LeaderInfo};

/// Start leader mode for a local space.
#[tauri::command]
pub async fn local_ds_start(
    state: State<'_, AppState>,
    space_id: String,
) -> Result<(), String> {
    let leader_state = Arc::new(LeaderState {
        db: crate::database::DbConnection(state.db.0.clone()),
        space_id: space_id.clone(),
        connected_peers: Arc::new(RwLock::new(HashMap::new())),
        notification_senders: Arc::new(RwLock::new(HashMap::new())),
    });

    let handler = Arc::new(LeaderConnectionHandler {
        state: leader_state,
    });

    let endpoint = state.peer_storage.lock().await;
    endpoint.set_delivery_handler(handler).await;

    eprintln!("[SpaceDelivery] Started leader mode for space {space_id}");
    Ok(())
}

/// Stop leader mode — clears buffers and unregisters handler.
#[tauri::command]
pub async fn local_ds_stop(
    state: State<'_, AppState>,
    space_id: String,
) -> Result<(), String> {
    // Clear buffer tables
    super::leader::clear_buffers(
        &crate::database::DbConnection(state.db.0.clone()),
        &space_id,
    ).map_err(|e| e.to_string())?;

    // Remove delivery handler
    let endpoint = state.peer_storage.lock().await;
    endpoint.state.write().await.delivery_handler = None;

    eprintln!("[SpaceDelivery] Stopped leader mode for space {space_id}");
    Ok(())
}

/// Get the current delivery status.
#[tauri::command]
pub async fn local_ds_status(
    state: State<'_, AppState>,
) -> Result<DeliveryStatus, String> {
    let endpoint = state.peer_storage.lock().await;
    let peer_state = endpoint.state.read().await;

    let is_leader = peer_state.delivery_handler.is_some();

    // If leader, get connected peers and buffer counts
    if is_leader {
        // We'd need access to LeaderState here — for now return basic info
        Ok(DeliveryStatus {
            is_leader: true,
            space_id: None, // TODO: store in AppState or query from handler
            connected_peers: vec![],
            buffered_messages: 0,
            buffered_welcomes: 0,
            buffered_key_packages: 0,
        })
    } else {
        Ok(DeliveryStatus {
            is_leader: false,
            space_id: None,
            connected_peers: vec![],
            buffered_messages: 0,
            buffered_welcomes: 0,
            buffered_key_packages: 0,
        })
    }
}

/// Get the current leader for a local space (using CRDT priorities + mDNS).
#[tauri::command]
pub async fn local_ds_get_leader(
    state: State<'_, AppState>,
    space_id: String,
) -> Result<Option<LeaderInfo>, String> {
    // Query haex_space_devices for this space, ordered by leader_priority
    let sql = "SELECT device_endpoint_id, leader_priority FROM haex_space_devices \
               WHERE space_id = ?1 ORDER BY leader_priority ASC, device_endpoint_id ASC LIMIT 1"
        .to_string();
    let params = vec![serde_json::Value::String(space_id.clone())];

    let rows = crate::database::core::select_with_crdt(sql, params, &state.db)
        .map_err(|e| format!("Failed to query space devices: {e}"))?;

    if let Some(row) = rows.first() {
        let endpoint_id = row.get(0).and_then(|v| v.as_str()).unwrap_or_default().to_string();
        let priority = row.get(1).and_then(|v| v.as_i64()).unwrap_or(10) as i32;
        Ok(Some(LeaderInfo { endpoint_id, priority, space_id }))
    } else {
        Ok(None)
    }
}
```

**Step 2: Register commands in lib.rs**

In `src-tauri/src/lib.rs`, add to the `.invoke_handler(tauri::generate_handler![...])` list:

```rust
space_delivery::local::commands::local_ds_start,
space_delivery::local::commands::local_ds_stop,
space_delivery::local::commands::local_ds_status,
space_delivery::local::commands::local_ds_get_leader,
```

**Step 3: Verify + Commit**

```
feat: add Tauri commands for local delivery service
```

---

### Task 8: Verify full build

**Step 1: cargo check**

Run: `cd /home/haex/Projekte/haex-vault/src-tauri && cargo check 2>&1 | tail -20`

Fix any compilation errors.

**Step 2: cargo clippy (optional)**

Run: `cd /home/haex/Projekte/haex-vault/src-tauri && cargo clippy 2>&1 | tail -20`

Fix any warnings that aren't about unused code (unused is expected — peer.rs, election.rs, discovery.rs are stubs).

**Step 3: Frontend type check**

Run: `cd /home/haex/Projekte/haex-vault && npx vue-tsc --noEmit 2>&1 | tail -5`

**Step 4: Commit if any fixes needed**

```
fix: resolve build issues in space_delivery module
```

---

## Summary of Changes

| File | Change |
|------|--------|
| `src-tauri/src/space_delivery/mod.rs` | NEW: Module declaration |
| `src-tauri/src/space_delivery/local/mod.rs` | NEW: Submodule declarations |
| `src-tauri/src/space_delivery/local/protocol.rs` | NEW: ALPN, Request/Response/Notification types, wire format |
| `src-tauri/src/space_delivery/local/types.rs` | NEW: ConnectedPeer, DeliveryStatus, LeaderInfo (TS-exported) |
| `src-tauri/src/space_delivery/local/error.rs` | NEW: DeliveryError enum |
| `src-tauri/src/space_delivery/local/leader.rs` | NEW: Buffer CRUD + connection handler |
| `src-tauri/src/space_delivery/local/housekeeping.rs` | NEW: TTL-based cleanup |
| `src-tauri/src/space_delivery/local/commands.rs` | NEW: Tauri commands (start, stop, status, get_leader) |
| `src-tauri/src/space_delivery/local/peer.rs` | Stub (Phase 6.4) |
| `src-tauri/src/space_delivery/local/election.rs` | Stub (Phase 6.3) |
| `src-tauri/src/space_delivery/local/discovery.rs` | Stub (Phase 6.3) |
| `src-tauri/src/peer_storage/endpoint.rs` | Dual-ALPN, DeliveryConnectionHandler trait, dispatch |
| `src-tauri/src/lib.rs` | Register space_delivery module + commands |

## What's deferred to later phases

- **Phase 6.3:** Election logic (`election.rs`), mDNS discovery (`discovery.rs`), graceful handoff
- **Phase 6.4:** Frontend integration (`peer.rs` client-side connect, `useMlsDelivery` LocalDeliveryService, Sync orchestrator integration)
- **Phase 6.5:** Invite flow for local spaces
