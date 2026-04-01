# Phase 6.3: Leader Election + Discovery

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Implement priority-based leader election with parallel peer probing and graceful handoff.

**Architecture:** Peers query `haexSpaceDevices` (CRDT-synced) for all devices in a space with their priorities. Then they probe ALL candidates in parallel via iroh connect (5s timeout). From the set of reachable peers, the one with the lowest priority number becomes leader. iroh's mDNS handles LAN address resolution automatically — no custom discovery needed.

**Tech Stack:** Rust, iroh 0.96, tokio (join_all for parallel probes)

**Key insight:** iroh mDNS is address-resolution, not peer-discovery. We don't enumerate mDNS results — we know EndpointIds from the CRDT and just try to connect. mDNS makes LAN connections work without relay.

---

### Task 1: Implement parallel peer probing in discovery.rs

Query all candidate devices for a space from the DB, then probe them all in parallel to find which are online.

**Files:**
- Modify: `src-tauri/src/space_delivery/local/discovery.rs`

**Implementation:**

```rust
//! Peer discovery: query CRDT for space devices, probe reachability in parallel.

use crate::database::DbConnection;
use crate::peer_storage::endpoint::PeerEndpoint;
use super::error::DeliveryError;
use super::types::LeaderInfo;

/// A candidate device for leader election
#[derive(Debug, Clone)]
pub struct DeviceCandidate {
    pub endpoint_id: String,
    pub relay_url: Option<String>,
    pub priority: i32,
}

/// Query all devices for a space from the CRDT, sorted by priority.
pub fn get_space_device_candidates(
    db: &DbConnection,
    space_id: &str,
) -> Result<Vec<DeviceCandidate>, DeliveryError> {
    let sql = "SELECT device_endpoint_id, relay_url, leader_priority \
               FROM haex_space_devices \
               WHERE space_id = ?1 \
               ORDER BY leader_priority ASC, device_endpoint_id ASC"
        .to_string();
    let params = vec![serde_json::Value::String(space_id.to_string())];

    let rows = crate::database::core::select_with_crdt(sql, params, db)
        .map_err(|e| DeliveryError::Database { reason: e.to_string() })?;

    Ok(rows
        .into_iter()
        .map(|row| DeviceCandidate {
            endpoint_id: row.get(0).and_then(|v| v.as_str()).unwrap_or_default().to_string(),
            relay_url: row.get(1).and_then(|v| v.as_str()).map(|s| s.to_string()),
            priority: row.get(2).and_then(|v| v.as_i64()).unwrap_or(10) as i32,
        })
        .collect())
}

/// Probe all candidates in parallel, return the reachable ones.
/// Uses a timeout (5 seconds) per probe to allow for relay connections.
/// Excludes our own endpoint_id (we don't probe ourselves).
pub async fn probe_reachable_candidates(
    endpoint: &PeerEndpoint,
    candidates: &[DeviceCandidate],
    own_endpoint_id: &str,
) -> Vec<DeviceCandidate> {
    use std::time::Duration;
    use tokio::time::timeout;

    let iroh_endpoint = match endpoint.endpoint_ref() {
        Some(ep) => ep,
        None => return vec![], // Endpoint not running
    };

    let probe_timeout = Duration::from_secs(5);

    let mut handles = Vec::new();

    for candidate in candidates {
        if candidate.endpoint_id == own_endpoint_id {
            // We're always "reachable" to ourselves — include without probing
            let candidate = candidate.clone();
            handles.push(tokio::spawn(async move { Some(candidate) }));
            continue;
        }

        let endpoint_id_str = candidate.endpoint_id.clone();
        let relay_url_str = candidate.relay_url.clone();
        let candidate = candidate.clone();
        let ep = iroh_endpoint.clone();
        let delivery_alpn = crate::space_delivery::local::protocol::ALPN.to_vec();

        handles.push(tokio::spawn(async move {
            let remote_id: iroh::EndpointId = match endpoint_id_str.parse() {
                Ok(id) => id,
                Err(_) => return None,
            };

            let addr = match relay_url_str.and_then(|s| s.parse::<iroh::RelayUrl>().ok()) {
                Some(url) => iroh::endpoint::EndpointAddr::new(remote_id).with_relay_url(url),
                None => iroh::endpoint::EndpointAddr::new(remote_id),
            };

            // Try to connect with timeout — if it succeeds, the peer is reachable
            match timeout(probe_timeout, ep.connect(addr, &delivery_alpn)).await {
                Ok(Ok(conn)) => {
                    // Close the probe connection immediately
                    conn.close(0u32.into(), b"probe");
                    Some(candidate)
                }
                _ => None, // Timeout or connection error
            }
        }));
    }

    let results = futures::future::join_all(handles).await;

    results
        .into_iter()
        .filter_map(|r| r.ok().flatten())
        .collect()
}

/// Find the best leader from reachable candidates (lowest priority number).
pub fn select_leader(reachable: &[DeviceCandidate]) -> Option<&DeviceCandidate> {
    reachable.first() // Already sorted by priority ASC from DB query
}
```

NOTE: `endpoint_ref()` needs to exist on PeerEndpoint — check if it does. If not, add a simple accessor:
```rust
pub fn endpoint_ref(&self) -> Option<&Endpoint> {
    self.endpoint.as_ref()
}
```

Also check if `iroh::endpoint::EndpointAddr` is the correct type path and if `Endpoint::clone()` is available. The probe needs a clone of the Endpoint to move into the spawned task. If Endpoint doesn't implement Clone, use `Arc<Endpoint>` or pass a reference differently.

Check if `futures` crate is in Cargo.toml for `join_all`. If not, use `tokio::task::JoinSet` instead.

**Verification:** `cargo check`

**Commit:**
```
feat: implement parallel peer probing for leader discovery
```

---

### Task 2: Implement leader election logic in election.rs

The election module ties discovery results to leader selection and self-promotion.

**Files:**
- Modify: `src-tauri/src/space_delivery/local/election.rs`

**Implementation:**

```rust
//! Priority-based leader election.
//!
//! Flow:
//! 1. Query all devices for a space from CRDT
//! 2. Probe all in parallel (3s timeout)
//! 3. Select lowest-priority reachable device as leader
//! 4. If we ARE the leader → start leader mode
//! 5. If someone else is leader → connect to them as peer

use crate::database::DbConnection;
use crate::peer_storage::endpoint::PeerEndpoint;
use super::discovery::{self, DeviceCandidate};
use super::error::DeliveryError;

/// Result of a leader election round.
#[derive(Debug)]
pub enum ElectionResult {
    /// This device should be the leader.
    SelfIsLeader,
    /// Another device is the leader — connect to it.
    RemoteLeader {
        endpoint_id: String,
        relay_url: Option<String>,
        priority: i32,
    },
    /// No devices are reachable (we're alone and offline).
    NoLeaderFound,
}

/// Run a leader election for a local space.
///
/// Queries all devices, probes reachability in parallel,
/// and determines who should be leader.
pub async fn elect_leader(
    db: &DbConnection,
    endpoint: &PeerEndpoint,
    space_id: &str,
    own_endpoint_id: &str,
) -> Result<ElectionResult, DeliveryError> {
    // 1. Get all candidates from CRDT
    let candidates = discovery::get_space_device_candidates(db, space_id)?;

    if candidates.is_empty() {
        return Ok(ElectionResult::NoLeaderFound);
    }

    // 2. Probe all in parallel
    let reachable = discovery::probe_reachable_candidates(endpoint, &candidates, own_endpoint_id).await;

    if reachable.is_empty() {
        return Ok(ElectionResult::NoLeaderFound);
    }

    // 3. Select leader (lowest priority)
    let leader = discovery::select_leader(&reachable);

    match leader {
        Some(candidate) if candidate.endpoint_id == own_endpoint_id => {
            Ok(ElectionResult::SelfIsLeader)
        }
        Some(candidate) => Ok(ElectionResult::RemoteLeader {
            endpoint_id: candidate.endpoint_id.clone(),
            relay_url: candidate.relay_url.clone(),
            priority: candidate.priority,
        }),
        None => Ok(ElectionResult::NoLeaderFound),
    }
}
```

**Verification:** `cargo check`

**Commit:**
```
feat: implement priority-based leader election
```

---

### Task 3: Wire election into Tauri commands

Update the commands to use election and add a new `local_delivery_elect` command.

**Files:**
- Modify: `src-tauri/src/space_delivery/local/commands.rs`
- Modify: `src-tauri/src/lib.rs` (register new command)

**New command:**

```rust
/// Run leader election for a local space.
/// Returns the election result: self is leader, remote leader, or no leader found.
#[tauri::command]
pub async fn local_delivery_elect(
    state: State<'_, AppState>,
    space_id: String,
) -> Result<ElectionResultInfo, String> {
    let db = DbConnection(state.db.0.clone());
    let endpoint = state.peer_storage.lock().await;
    let own_endpoint_id = endpoint.endpoint_id().to_string();

    let result = super::election::elect_leader(&db, &endpoint, &space_id, &own_endpoint_id)
        .await
        .map_err(|e| e.to_string())?;

    match result {
        super::election::ElectionResult::SelfIsLeader => {
            Ok(ElectionResultInfo {
                role: "leader".to_string(),
                leader_endpoint_id: Some(own_endpoint_id),
                leader_priority: None,
                leader_relay_url: None,
            })
        }
        super::election::ElectionResult::RemoteLeader { endpoint_id, relay_url, priority } => {
            Ok(ElectionResultInfo {
                role: "peer".to_string(),
                leader_endpoint_id: Some(endpoint_id),
                leader_priority: Some(priority),
                leader_relay_url: relay_url,
            })
        }
        super::election::ElectionResult::NoLeaderFound => {
            Ok(ElectionResultInfo {
                role: "none".to_string(),
                leader_endpoint_id: None,
                leader_priority: None,
                leader_relay_url: None,
            })
        }
    }
}
```

Add `ElectionResultInfo` to `types.rs`:

```rust
/// Result of a leader election, exposed to frontend.
#[derive(Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ElectionResultInfo {
    /// "leader", "peer", or "none"
    pub role: String,
    pub leader_endpoint_id: Option<String>,
    pub leader_priority: Option<i32>,
    pub leader_relay_url: Option<String>,
}
```

Register `local_delivery_elect` in `lib.rs` alongside the other delivery commands.

**Verification:** `cargo check`

**Commit:**
```
feat: add leader election Tauri command
```

---

### Task 4: Update `local_delivery_get_leader` to use election

The existing `local_delivery_get_leader` command only reads from DB (no reachability check). Update it to optionally run a full election with probing.

**Files:**
- Modify: `src-tauri/src/space_delivery/local/commands.rs`

Replace the existing `local_delivery_get_leader` implementation to use the election module when the endpoint is running:

```rust
#[tauri::command]
pub async fn local_delivery_get_leader(
    state: State<'_, AppState>,
    space_id: String,
) -> Result<Option<LeaderInfo>, String> {
    let db = DbConnection(state.db.0.clone());
    let endpoint = state.peer_storage.lock().await;

    if !endpoint.is_running() {
        // Endpoint not running — fall back to DB-only query (no probing)
        let candidates = super::discovery::get_space_device_candidates(&db, &space_id)
            .map_err(|e| e.to_string())?;
        return Ok(candidates.first().map(|c| LeaderInfo {
            endpoint_id: c.endpoint_id.clone(),
            priority: c.priority,
            space_id,
        }));
    }

    // Endpoint running — do full election with parallel probing
    let own_endpoint_id = endpoint.endpoint_id().to_string();
    let result = super::election::elect_leader(&db, &endpoint, &space_id, &own_endpoint_id)
        .await
        .map_err(|e| e.to_string())?;

    match result {
        super::election::ElectionResult::SelfIsLeader => {
            Ok(Some(LeaderInfo {
                endpoint_id: own_endpoint_id,
                priority: 0, // Self — exact priority not critical
                space_id,
            }))
        }
        super::election::ElectionResult::RemoteLeader { endpoint_id, priority, .. } => {
            Ok(Some(LeaderInfo { endpoint_id, priority, space_id }))
        }
        super::election::ElectionResult::NoLeaderFound => Ok(None),
    }
}
```

**Verification:** `cargo check`

**Commit:**
```
feat: update get_leader to use parallel probing when endpoint is running
```

---

### Task 5: Verify full build

**Step 1:** `cd /home/haex/Projekte/haex-vault/src-tauri && cargo check 2>&1 | tail -15`
**Step 2:** `cd /home/haex/Projekte/haex-vault && npx vue-tsc --noEmit 2>&1 | tail -5`

Fix any issues. Commit fixes if needed.

---

## Summary

| File | Change |
|------|--------|
| `space_delivery/local/discovery.rs` | Parallel peer probing via QUIC connect + timeout |
| `space_delivery/local/election.rs` | Priority-based leader election using probe results |
| `space_delivery/local/types.rs` | Add `ElectionResultInfo` (TS-exported) |
| `space_delivery/local/commands.rs` | New `local_delivery_elect`, updated `local_delivery_get_leader` |
| `peer_storage/endpoint.rs` | May need `endpoint_ref()` accessor |
| `lib.rs` | Register `local_delivery_elect` command |

## What's NOT in this phase

- **Graceful handoff** — deferred. When a higher-priority device comes online, the frontend will detect it on next election and switch. No automatic mid-connection handoff yet.
- **Periodic re-election** — the frontend decides when to call `local_delivery_elect` (on app start, on sync, on timer). No Rust-side scheduler.
- **Notification of leader change** — not needed yet. Frontend polls or reacts to connectivity changes.
