# Phase 6.4: Rust-Autonomous Local Sync + Frontend Wiring

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Build a Rust-autonomous sync loop for local spaces — peer client connects to leader, pushes/pulls CRDT changes directly in Rust without frontend involvement. Frontend only triggers start/stop and receives Tauri events for UI updates.

**Architecture:** The sync loop runs entirely in Rust:
1. Peer connects to leader via QUIC (`haex-delivery/1` ALPN)
2. Peer scans dirty tables → sends changes to leader via `SYNC_PUSH`
3. Leader broadcasts `NOTIFY_SYNC` to peers
4. Peer receives notification → pulls changes via `SYNC_PULL` → applies directly to local DB
5. Frontend gets notified via Tauri events for UI refresh

No encryption/decryption needed for local spaces — QUIC provides transport encryption, and data is stored unencrypted in local DB (same as personal vault data).

**Tech Stack:** Rust (tokio, iroh, rusqlite), Tauri events for frontend notification

**Key insight:** For local spaces, we skip the encryption layer entirely. The CRDT changes are transmitted as plain JSON over QUIC (transport-encrypted). This is the same security model as the personal vault — data at rest is protected by SQLCipher, data in transit by QUIC TLS.

---

### Task 1: Implement table scanner in Rust

Port the outbound table scanning logic from `tableScanner.ts` to Rust. This scans dirty tables for column-level changes to push.

**Files:**
- Create: `src-tauri/src/crdt/scanner.rs`
- Modify: `src-tauri/src/crdt/mod.rs` (add `pub mod scanner;`)

**What to implement:**

A Rust function that does what `scanTableForChangesAsync` does in TypeScript, but **without encryption** (for local sync):

```rust
//! Table scanner for outbound CRDT changes.
//!
//! Scans dirty tables for column-level changes that need to be pushed to peers.
//! Used by the local delivery sync loop for unencrypted local-space sync.

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use crate::database::{DbConnection, error::DatabaseError};
use crate::database::core::with_connection;

/// A column-level change ready for transmission (unencrypted, for local sync).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalColumnChange {
    pub table_name: String,
    pub row_pks: String,          // JSON string of PK values
    pub column_name: String,
    pub hlc_timestamp: String,
    pub value: JsonValue,         // Plain value (not encrypted)
    pub device_id: String,
}

/// Scan a table for rows newer than `after_hlc`, return column-level changes.
pub fn scan_table_for_local_changes(
    db: &DbConnection,
    table_name: &str,
    after_hlc: Option<&str>,
    device_id: &str,
) -> Result<Vec<LocalColumnChange>, DatabaseError> {
    // 1. Get table schema (pk columns + data columns)
    // 2. Build SELECT query for all columns + haex_timestamp + haex_column_hlcs
    // 3. Filter WHERE haex_timestamp > after_hlc (if provided)
    // 4. For each row, for each data column:
    //    - Get column HLC from haex_column_hlcs JSON
    //    - If column HLC > after_hlc → create LocalColumnChange
    // 5. Return all changes
}

/// Scan all dirty tables and collect changes for local sync.
pub fn scan_all_dirty_tables_for_local_changes(
    db: &DbConnection,
    after_hlc: Option<&str>,
    device_id: &str,
) -> Result<Vec<LocalColumnChange>, DatabaseError> {
    // 1. get_dirty_tables()
    // 2. For each dirty table, scan_table_for_local_changes()
    // 3. Collect all changes
}
```

Read `src/stores/sync/tableScanner.ts` (lines 209-253) for the exact logic to port. Read `src-tauri/src/crdt/commands.rs` for `get_dirty_tables`, `get_table_schema`, and column filtering patterns.

Key details:
- Exclude CRDT metadata columns (`haex_timestamp`, `haex_column_hlcs`, `haex_tombstone`) from data columns — but DO include `haex_tombstone` as a syncable column (it tracks soft deletes)
- Exclude sync metadata columns (`last_push_hlc_timestamp`, `last_pull_server_timestamp`, `updated_at`, `created_at`)
- Use `haex_column_hlcs` JSON to get per-column HLC, fall back to row-level `haex_timestamp`
- Primary key columns are NOT synced as changes — they identify rows

**Verification:** `cargo check`

**Commit:**
```
feat: implement Rust table scanner for local CRDT sync
```

---

### Task 2: Implement peer client in peer.rs

The peer client connects to the leader via QUIC, sends/receives sync data.

**Files:**
- Modify: `src-tauri/src/space_delivery/local/peer.rs`

**What to implement:**

```rust
//! Peer-side logic: connecting to leader, sending/receiving sync data.

use std::sync::Arc;
use crate::database::DbConnection;
use crate::peer_storage::endpoint::PeerEndpoint;
use super::protocol::{self, Request, Response};
use super::error::DeliveryError;

/// A connected peer session with the leader.
pub struct PeerSession {
    /// The QUIC connection to the leader
    conn: iroh::endpoint::Connection,
    /// Our DID for identification
    our_did: String,
    /// Our endpoint ID
    our_endpoint_id: String,
}

impl PeerSession {
    /// Connect to a leader and announce our identity.
    pub async fn connect(
        endpoint: &PeerEndpoint,
        leader_endpoint_id: &str,
        leader_relay_url: Option<&str>,
        our_did: &str,
        our_endpoint_id: &str,
        label: Option<&str>,
    ) -> Result<Self, DeliveryError>

    /// Send a request and read the response.
    async fn request(&self, req: Request) -> Result<Response, DeliveryError>

    /// Push local changes to the leader.
    pub async fn push_changes(&self, changes: serde_json::Value) -> Result<(), DeliveryError>

    /// Pull changes from the leader.
    pub async fn pull_changes(&self, after_timestamp: Option<&str>) -> Result<serde_json::Value, DeliveryError>

    /// Close the connection.
    pub fn close(&self)
}
```

`connect()`:
1. Parse leader_endpoint_id to `iroh::EndpointId`
2. Build `EndpointAddr` (with optional relay_url)
3. Connect with delivery ALPN
4. Send `Announce` request with our_did, endpoint_id, label
5. Return PeerSession

`request()`:
1. Open bidirectional stream on the connection
2. Encode request → write to send stream → finish send
3. Read response from recv stream
4. Return response

`push_changes()`:
1. Create `SyncPush` request with changes
2. Send via `request()`
3. Check for `Response::Ok`

`pull_changes()`:
1. Create `SyncPull` request
2. Send via `request()`
3. Match `Response::SyncChanges` → return changes

**Verification:** `cargo check`

**Commit:**
```
feat: implement peer client for local space delivery
```

---

### Task 3: Implement the Rust sync loop

The autonomous sync loop that runs in the background. Ties together peer client, table scanner, and CRDT apply.

**Files:**
- Create: `src-tauri/src/space_delivery/local/sync_loop.rs`
- Modify: `src-tauri/src/space_delivery/local/mod.rs` (add `pub mod sync_loop;`)

**What to implement:**

```rust
//! Autonomous sync loop for local spaces.
//!
//! Runs entirely in Rust: connects to leader, pushes dirty changes,
//! pulls remote changes, applies them to local DB.

use std::sync::Arc;
use tokio::sync::watch;
use crate::database::DbConnection;
use crate::crdt::hlc::HlcService;
use crate::peer_storage::endpoint::PeerEndpoint;
use super::peer::PeerSession;
use super::error::DeliveryError;

/// Handle to a running sync loop. Drop to stop.
pub struct SyncLoopHandle {
    stop_sender: watch::Sender<bool>,
    task: tokio::task::JoinHandle<()>,
}

impl SyncLoopHandle {
    /// Signal the loop to stop.
    pub fn stop(&self) {
        let _ = self.stop_sender.send(true);
    }
}

/// Start the sync loop for a local space as a peer.
pub async fn start_peer_sync_loop(
    db: DbConnection,
    endpoint: Arc<PeerEndpoint>,  // or clone from state
    leader_endpoint_id: String,
    leader_relay_url: Option<String>,
    space_id: String,
    our_did: String,
    our_endpoint_id: String,
    device_id: String,
    app_handle: tauri::AppHandle,  // For emitting events to frontend
) -> Result<SyncLoopHandle, DeliveryError> {
    let (stop_tx, stop_rx) = watch::channel(false);

    let task = tokio::spawn(async move {
        run_sync_loop(
            db, endpoint, leader_endpoint_id, leader_relay_url,
            space_id, our_did, our_endpoint_id, device_id,
            app_handle, stop_rx,
        ).await;
    });

    Ok(SyncLoopHandle { stop_sender: stop_tx, task })
}
```

`run_sync_loop` implementation:
1. Connect to leader via `PeerSession::connect()`
2. Loop (checking `stop_rx` each iteration):
   a. **Push:** Scan dirty tables → serialize as JSON → `session.push_changes()`
   b. **Pull:** `session.pull_changes()` → deserialize → `apply_remote_changes_in_transaction()`
   c. **Emit Tauri event** `local-sync-completed` with affected tables
   d. **Wait** for notification from leader (or poll interval as fallback, e.g. 5s)
3. On disconnect: try to reconnect (with backoff), or run re-election

For the push side:
- Use `scan_all_dirty_tables_for_local_changes()` from Task 1
- Serialize changes as JSON → `SyncPush` request
- On success: `clear_dirty_table()` for each table pushed

For the pull side:
- `SyncPull` request → get changes as JSON
- Convert to `Vec<RemoteColumnChange>` format (matching what `apply_remote_changes_in_transaction` expects)
- Call the CRDT apply function directly from Rust

**Important:** The `apply_remote_changes_in_transaction` function expects `decrypted_value` — for local sync, the values ARE already plain (no encryption layer). Map `LocalColumnChange.value` → `RemoteColumnChange.decrypted_value`.

**Verification:** `cargo check`

**Commit:**
```
feat: implement autonomous Rust sync loop for local spaces
```

---

### Task 4: Wire sync loop into Tauri commands

Update commands to start/stop the sync loop alongside leader mode.

**Files:**
- Modify: `src-tauri/src/space_delivery/local/commands.rs`
- Modify: `src-tauri/src/lib.rs` (add SyncLoopHandle to AppState if needed)

**New/updated commands:**

```rust
/// Connect to a local space leader and start syncing.
#[tauri::command]
pub async fn local_delivery_connect(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    space_id: String,
    leader_endpoint_id: String,
    leader_relay_url: Option<String>,
    identity_did: String,
) -> Result<(), String>
```

This command:
1. Gets our endpoint_id from `state.peer_storage`
2. Gets device_id from state
3. Calls `start_peer_sync_loop()`
4. Stores the `SyncLoopHandle` (needs a place in AppState — add `sync_loops: tokio::sync::Mutex<HashMap<String, SyncLoopHandle>>`)

```rust
/// Disconnect from a local space leader and stop syncing.
#[tauri::command]
pub async fn local_delivery_disconnect(
    state: State<'_, AppState>,
    space_id: String,
) -> Result<(), String>
```

This command:
1. Looks up SyncLoopHandle for space_id
2. Calls `handle.stop()`
3. Removes from HashMap

**AppState addition in lib.rs:**

Add to AppState struct:
```rust
pub local_sync_loops: tokio::sync::Mutex<HashMap<String, space_delivery::local::sync_loop::SyncLoopHandle>>,
```

Initialize with:
```rust
local_sync_loops: tokio::sync::Mutex::new(HashMap::new()),
```

Register new commands.

**Verification:** `cargo check`

**Commit:**
```
feat: add connect/disconnect commands for local sync loop
```

---

### Task 5: Handle leader-side sync (SyncPush/SyncPull in leader handler)

Currently `SyncPush` and `SyncPull` in the leader handler return stubs. Implement them.

**Files:**
- Modify: `src-tauri/src/space_delivery/local/leader.rs`

**SyncPush handler:**
The leader receives changes from a peer and applies them to its own DB.

```rust
Request::SyncPush { space_id, changes } => {
    // 1. Deserialize changes from JSON into Vec<RemoteColumnChange>
    // 2. Call apply_remote_changes_in_transaction() with the leader's DB
    // 3. Broadcast NOTIFY_SYNC to all connected peers (except sender)
    // 4. Return Response::Ok
}
```

**SyncPull handler:**
The leader scans its DB for changes newer than the peer's timestamp and returns them.

```rust
Request::SyncPull { space_id, after_timestamp } => {
    // 1. Use scan_all_dirty_tables_for_local_changes() to get changes
    //    (or better: scan ALL tables newer than after_timestamp, not just dirty ones)
    // 2. Serialize as JSON
    // 3. Return Response::SyncChanges { changes }
}
```

Note: For pull, we scan all CRDT tables (not just dirty), because the peer might be catching up from a long time ago. Use the same scanning logic from Task 1 but scan all CRDT tables, not just dirty ones.

**Verification:** `cargo check`

**Commit:**
```
feat: implement SyncPush and SyncPull in leader handler
```

---

### Task 6: Update createLocalSpaceAsync for MLS group

Local spaces need an MLS group for the epoch key (used by the sync engine to determine encryption mode).

**Files:**
- Modify: `src/stores/spaces.ts`

**Update `createLocalSpaceAsync`:**

After `await persistSpaceAsync(space)`, add MLS group creation:

```typescript
// Create MLS group for this space (same as remote spaces)
await invoke('mls_create_group', { spaceId: id })
await invoke('mls_export_epoch_key', { spaceId: id })

// Create root UCAN for this space
const identityStore = useIdentityStore()
const identity = identityStore.identities[0]
if (identity) {
  const rootUcan = await createRootUcanAsync(identity.did, identity.privateKey, id)
  const db = getDb()
  if (db) await persistUcanAsync(db, id, rootUcan)
}
```

**Verification:** `npx vue-tsc --noEmit`

**Commit:**
```
feat: create MLS group and root UCAN for local spaces
```

---

### Task 7: Frontend wiring — emit Tauri events, listen in orchestrator

The sync loop in Rust emits Tauri events when sync completes. The frontend listens and refreshes stores.

**Files:**
- Modify: `src/stores/sync/orchestrator/index.ts` or create `src/stores/sync/orchestrator/local.ts`

**Tauri event to emit from Rust** (in sync_loop.rs):
```rust
app_handle.emit("local-sync-completed", serde_json::json!({
    "spaceId": space_id,
    "tables": affected_tables,
})).ok();
```

**Frontend listener** (in orchestrator startup):
```typescript
import { listen } from '@tauri-apps/api/event'

// Listen for local sync completions
await listen('local-sync-completed', async (event) => {
  const { tables } = event.payload as { spaceId: string; tables: string[] }
  if (tables.length > 0) {
    await emit('sync:tables-updated', { tables })
  }
})
```

This triggers the existing store reload mechanism — no changes needed to syncEvents.ts.

**Verification:** `npx vue-tsc --noEmit`

**Commit:**
```
feat: wire local sync events to frontend store refresh
```

---

### Task 8: Verify full build

**Step 1:** `cargo check`
**Step 2:** `npx vue-tsc --noEmit`
**Step 3:** Test that the plan document is committed

---

## Summary

| File | Change |
|------|--------|
| `src-tauri/src/crdt/scanner.rs` | NEW: Table scanner for unencrypted local sync |
| `src-tauri/src/space_delivery/local/peer.rs` | Peer client (QUIC connect, request/response) |
| `src-tauri/src/space_delivery/local/sync_loop.rs` | NEW: Autonomous background sync loop |
| `src-tauri/src/space_delivery/local/leader.rs` | SyncPush/SyncPull handlers (was stubs) |
| `src-tauri/src/space_delivery/local/commands.rs` | `local_delivery_connect/disconnect` commands |
| `src-tauri/src/lib.rs` | AppState + command registration |
| `src/stores/spaces.ts` | MLS group creation for local spaces |
| `src/stores/sync/orchestrator/` | Local sync event listener |

## What's deferred

- **Graceful handoff:** Re-election on disconnect (Phase 6.3 provides primitives, but auto-reconnect is future work)
- **Conflict resolution UI:** CRDT handles conflicts silently for now
- **Notification stream:** Currently pull-based/polling; push notifications over long-lived QUIC stream is future optimization
