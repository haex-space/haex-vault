//! Connection diagnostics — path-type inspection and push-event watcher.

use std::sync::Arc;
use tokio::sync::RwLock;

use iroh::EndpointId;

use tauri::Emitter;

use crate::peer_storage::endpoint::{ConnectionDiagnostics, PathType, PeerState};

// ============================================================================
// Connection diagnostics — shared by on-demand query + push-event watcher
// ============================================================================

/// Extract `ConnectionDiagnostics` from a live iroh `Connection`. Shared
/// between `diagnose_connection` (on-demand query) and the watcher task that
/// emits push events when iroh switches the selected path.
pub(super) fn compute_diagnostics(conn: &iroh::endpoint::Connection) -> ConnectionDiagnostics {
    if conn.close_reason().is_some() {
        return ConnectionDiagnostics {
            path_type: PathType::Closed,
            remote_addr: None,
            rtt_ms: None,
        };
    }

    let info = conn.to_info();
    let (path_type, remote_addr, rtt_ms) = match info.selected_path() {
        Some(path) => {
            let path_type = if path.is_relay() {
                PathType::Relay
            } else if path.is_ip() {
                PathType::Direct
            } else {
                PathType::Unknown
            };
            let rtt_ms = path.rtt().map(|d| d.as_secs_f64() * 1000.0);
            let remote_addr = Some(format!("{:?}", path.remote_addr()));
            (path_type, remote_addr, rtt_ms)
        }
        None => (PathType::Unknown, None, None),
    };

    ConnectionDiagnostics {
        path_type,
        remote_addr,
        rtt_ms,
    }
}

/// Spawn a task that watches `conn.paths()` and emits a Tauri
/// `peer-storage:connection-changed` event each time iroh switches the selected
/// network path (direct↔relay) or the connection is dropped. The task
/// self-terminates when the watcher returns `Disconnected` — i.e., when the
/// underlying iroh `Watchable` is dropped (connection torn down end to end), so
/// there is no explicit lifecycle to manage from the caller side.
///
/// A peer can have more than one connection alive at once (inbound + outbound,
/// or a stale connection lingering across a reconnect), each with its own
/// watcher. We track a per-peer live count in `PeerState::connection_watchers`
/// and only emit the `Closed` diagnostic when the LAST connection goes away —
/// otherwise a transient connection dropping would overwrite a still-valid
/// `online` state and flip the UI offline.
///
/// This replaces a frontend `setInterval` poll: the UI listens once, gets
/// real-time updates, and incurs zero CPU when nothing changes.
pub(super) fn spawn_connection_watcher(
    remote_id: EndpointId,
    conn: iroh::endpoint::Connection,
    state: Arc<RwLock<PeerState>>,
) {
    let node_id_str = remote_id.to_string();
    tokio::spawn(async move {
        use iroh::Watcher;
        let mut watcher = conn.paths();

        // Register this connection before emitting anything, so a concurrent
        // disconnect of a sibling connection can see we're still here.
        {
            let mut s = state.write().await;
            *s.connection_watchers.entry(remote_id).or_insert(0) += 1;
        }

        // Initial snapshot — a frontend that subscribed after the connection
        // was established still gets a value without waiting for the first
        // path switch.
        emit_connection_changed(&state, &node_id_str, &conn).await;

        loop {
            match watcher.updated().await {
                Ok(_paths) => emit_connection_changed(&state, &node_id_str, &conn).await,
                Err(_disconnected) => break,
            }
        }

        // This connection is gone. Deregister and only signal `Closed` once
        // the peer has no live connection left — a stale or duplicate watcher
        // must not clobber the `online` state another connection still owns.
        // Emitting `Closed` explicitly (rather than recomputing from `conn`)
        // avoids the window where `close_reason()` hasn't propagated yet and
        // `compute_diagnostics` would report a stale `online`/path snapshot.
        let remaining = {
            let mut s = state.write().await;
            let count = s.connection_watchers.entry(remote_id).or_insert(0);
            *count = count.saturating_sub(1);
            let remaining = *count;
            if remaining == 0 {
                s.connection_watchers.remove(&remote_id);
            }
            remaining
        };
        if remaining == 0 {
            emit_diagnostics(
                &state,
                &node_id_str,
                ConnectionDiagnostics {
                    path_type: PathType::Closed,
                    remote_addr: None,
                    rtt_ms: None,
                },
            )
            .await;
        }
    });
}

async fn emit_connection_changed(
    state: &Arc<RwLock<PeerState>>,
    node_id_str: &str,
    conn: &iroh::endpoint::Connection,
) {
    emit_diagnostics(state, node_id_str, compute_diagnostics(conn)).await;
}

async fn emit_diagnostics(
    state: &Arc<RwLock<PeerState>>,
    node_id_str: &str,
    diagnostics: ConnectionDiagnostics,
) {
    let app_handle = { state.read().await.app_handle.clone() };
    let Some(app) = app_handle else {
        // App handle is set on Android start and during normal vault boot.
        // Absence means we're running in a context without a frontend (tests,
        // pre-init) — silently skip the emit.
        return;
    };
    let _ = app.emit_to(
        "main",
        crate::event_names::EVENT_PEER_CONNECTION_CHANGED,
        serde_json::json!({
            "nodeId": node_id_str,
            "diagnostics": diagnostics,
        }),
    );
}
