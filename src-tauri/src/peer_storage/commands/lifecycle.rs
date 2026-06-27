//! Endpoint lifecycle + diagnostics commands.

use tauri::State;

use super::helpers::{load_own_identity_for_device, reload_state_from_db};
use crate::critical::CriticalFailureCode;
use crate::database::DbConnection;
use crate::peer_storage::error::PeerStorageError;
use crate::AppState;

// ============================================================================
// Endpoint lifecycle commands
// ============================================================================

/// Start the peer storage endpoint and load shares for this device from DB
#[tauri::command(rename_all = "camelCase")]
pub async fn peer_storage_start(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    relay_url: Option<String>,
) -> Result<PeerStorageStartInfo, PeerStorageError> {
    let mut endpoint = state.peer_storage.write().await;

    // Bail out before mutating endpoint state. `set_own_identity` below
    // asserts when the endpoint is already running, so without this guard
    // a second `peer_storage_start` call would panic instead of returning
    // the recoverable `EndpointAlreadyRunning` error that `start` raises.
    if endpoint.is_running() {
        return Err(PeerStorageError::EndpointAlreadyRunning);
    }

    // Store AppHandle so the accept loop can use android_fs for Content URI shares
    endpoint.set_app_handle(app.clone()).await;

    // Load the device's identity so the quic_did_auth handshake can prove
    // our DID to peers and reject inbound peers that present mismatched
    // UCANs. Must run before `start` — the accept loop is spawned inside
    // `start` and reads the identity slot from then on.
    let own_endpoint_id = endpoint.endpoint_id().to_string();
    let own_identity = load_own_identity_for_device(&state, &own_endpoint_id)?;
    endpoint.set_own_identity(own_identity.clone());

    // Load shares and allowed peers from DB before starting
    reload_state_from_db(&state, &*endpoint).await?;

    // Phase 2 DoS-defence: snapshot `dosDefence.*` settings now, before the
    // accept loop spawns. The Default config (Phase 2 defaults) is already
    // installed by `PeerState::default`, so a load failure or empty rowset
    // falls back to those values without breaking start. Hot-reload on a
    // later settings edit is deferred — stop the endpoint and start it
    // again to pick up new values (calling `peer_storage_start` while it is
    // already running returns `EndpointAlreadyRunning`). Matches L4's
    // "snapshot at leader start" stance.
    let dos_config =
        crate::space_delivery::local::dos_defence::config::DosDefenceConfig::load(&state.db);
    endpoint.set_dos_config(dos_config).await;

    // Phase 3: install the FloodMode runtime so the accept loop performs
    // contacts-only escalation during DDoS episodes and emits one-shot
    // `FloodDdos` critical notifications. Falls back to Phase 2 semantics
    // if the critical-sink slot is empty (e.g. vault closed mid-start).
    let sink_clone = state
        .critical_sink
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .clone();
    if let Some(sink) = sink_clone {
        let runtime_db = crate::database::DbConnection(state.db.0.clone());
        let runtime = std::sync::Arc::new(
            crate::space_delivery::local::dos_defence::state::DosDefenceRuntime::load(
                runtime_db, sink,
            ),
        );
        endpoint.set_dos_runtime(runtime).await;
    } else {
        eprintln!(
            "[DosDefence Phase 3] critical_sink unavailable at peer_storage_start; \
             running with Phase 2 semantics until next start"
        );
    }

    let node_id = endpoint.start(relay_url).await?;

    // Register the unified multi-space handler so this device can accept
    // PushInvite/ClaimInvite from peers and route leader requests by space_id.
    // With an empty leader map it only handles PushInvite.
    {
        let has_handler = endpoint.state.read().await.delivery_handler.is_some();
        if !has_handler {
            let db_conn = DbConnection(state.db.0.clone());
            let hlc_clone = state
                .lock_or_fail(
                    &state.hlc,
                    CriticalFailureCode::HlcMutexPoisoned,
                    "peer_storage::commands::peer_storage_start",
                    serde_json::json!({}),
                )?
                .clone();
            let handler = std::sync::Arc::new(
                crate::space_delivery::local::multi_leader::MultiSpaceLeaderHandler {
                    leaders: state.leader_state.clone(),
                    db: db_conn,
                    hlc: std::sync::Arc::new(std::sync::Mutex::new(hlc_clone)),
                    app_handle: app.clone(),
                    own_endpoint_id: own_endpoint_id.clone(),
                    own_identity: std::sync::Arc::new(std::sync::Mutex::new(None)),
                    endpoint_dids: std::sync::Arc::new(tokio::sync::RwLock::new(
                        std::collections::HashMap::new(),
                    )),
                },
            );
            // Mirror the same device identity onto the delivery handler so
            // its server-initiated quic_did_auth handshake can run on every
            // incoming connection. Same identity material as peer_storage —
            // the slot exists per-handler so peer_storage and space_delivery
            // can be configured independently if that ever becomes useful.
            handler.set_own_identity(own_identity.clone());
            endpoint.set_delivery_handler(handler).await;
        }
    }

    // Clone the iroh endpoint handle before dropping the write lock so the
    // relay wait below does not block concurrent read operations (e.g. local_delivery_connect).
    let iroh_ep = endpoint.endpoint_ref().cloned();
    drop(endpoint);

    // Wait briefly for relay connection so we can advertise our relay URL to peers
    let relay_url = if let Some(ep) = iroh_ep {
        match tokio::time::timeout(std::time::Duration::from_secs(5), ep.online()).await {
            Ok(()) => ep
                .addr()
                .relay_urls()
                .next()
                .cloned()
                .map(|u| u.to_string()),
            Err(_) => None,
        }
    } else {
        None
    };

    Ok(PeerStorageStartInfo {
        node_id: node_id.to_string(),
        relay_url,
    })
}

/// Stop the peer storage endpoint
#[tauri::command]
pub async fn peer_storage_stop(state: State<'_, AppState>) -> Result<(), PeerStorageError> {
    let mut endpoint = state.peer_storage.write().await;
    endpoint.stop().await
}

/// Get the current node ID and running status
#[tauri::command]
pub async fn peer_storage_status(
    state: State<'_, AppState>,
) -> Result<PeerStorageStatus, PeerStorageError> {
    let endpoint = state.peer_storage.read().await;
    Ok(PeerStorageStatus {
        running: endpoint.is_running(),
        node_id: endpoint.endpoint_id().to_string(),
    })
}

/// Reload shares and allowed peers from DB into the running endpoint.
/// Called by the frontend after adding/removing shares or space devices via Drizzle.
#[tauri::command]
pub async fn peer_storage_reload_shares(
    state: State<'_, AppState>,
) -> Result<usize, PeerStorageError> {
    let endpoint = state.peer_storage.read().await;
    reload_state_from_db(&state, &*endpoint).await
}

/// Report whether the active QUIC connection to a peer runs over a direct
/// LAN/WAN path or via the relay. Used to diagnose throughput problems —
/// relay-routed connections cap at ~1 MB/s per stream, which looks like a
/// code-tuning issue but is actually a NAT/hole-punch failure.
///
/// Returns `None` if there is no live cached connection. The caller should
/// first establish one (e.g. via `peer_storage_remote_list`) before asking.
#[tauri::command(rename_all = "camelCase")]
pub async fn peer_storage_diagnose_connection(
    state: State<'_, AppState>,
    node_id: String,
) -> Result<Option<crate::peer_storage::endpoint::ConnectionDiagnostics>, PeerStorageError> {
    let remote_id: iroh::EndpointId =
        node_id
            .parse()
            .map_err(|e| PeerStorageError::ConnectionFailed {
                reason: format!("Invalid EndpointId: {e}"),
            })?;

    let endpoint = state.peer_storage.read().await;
    Ok(endpoint.diagnose_connection(remote_id))
}

/// Force the DDoS contacts-only escalation back to Quiet — the user
/// acknowledges the banner before the `dosDefence.ddos.autoExpirySecs`
/// deadline. No-op if no flood-mode runtime is installed or the current
/// state is already Quiet.
#[tauri::command(rename_all = "camelCase")]
pub async fn dos_defence_end_escalation(
    state: State<'_, AppState>,
) -> Result<(), PeerStorageError> {
    let endpoint = state.peer_storage.read().await;
    if let Some(runtime) = endpoint.dos_runtime().await {
        runtime.end_escalation();
    }
    Ok(())
}

// ============================================================================
// Response types
// ============================================================================

#[derive(Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PeerStorageStartInfo {
    pub node_id: String,
    pub relay_url: Option<String>,
}

#[derive(Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PeerStorageStatus {
    pub running: bool,
    pub node_id: String,
}
