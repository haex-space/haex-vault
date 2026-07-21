//! Lifecycle commands: start/stop leader mode, broadcast commits, status.

use std::collections::HashMap;
use std::sync::Arc;

use tauri::State;
use tokio::sync::RwLock;

use crate::critical::CriticalFailureCode;
use crate::database::DbConnection;
use crate::AppState;

use super::super::invite_tokens;
use super::super::leader::LeaderState;
use super::super::types::DeliveryStatus;

/// Start leader mode for a local space.
/// Inserts a new LeaderState into the shared map. On the first call,
/// registers the MultiSpaceLeaderHandler on the QUIC endpoint.
#[tauri::command]
pub async fn local_delivery_start(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    space_id: String,
) -> Result<(), String> {
    let db_conn = DbConnection(state.db.0.clone());
    let existing_tokens =
        invite_tokens::load_invite_tokens(&db_conn, &space_id).unwrap_or_default();

    let hlc_clone = state
        .lock_or_fail(
            &state.hlc,
            CriticalFailureCode::HlcMutexPoisoned,
            "space_delivery::local::commands::local_delivery_start",
            serde_json::json!({}),
        )
        .map_err(|e| e.to_string())?
        .clone();

    let dos_config = super::super::dos_defence::config::DosDefenceConfig::load(&db_conn);
    let reject_tracker = super::super::dos_defence::tracker::RejectRateTracker::new(
        std::time::Duration::from_secs(1),
    );
    let flood_notifier = super::super::dos_defence::notifier::SingleSourceNotifier::new();

    // Snapshot the sink so the leader can emit single-source-flood
    // banners without holding the global sink-slot mutex across reject
    // paths. Falls back to None when the vault is opened without a sink
    // (pre-mount or tests) — emission becomes a silent no-op.
    let critical_sink = {
        let guard = state
            .critical_sink
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        guard.clone()
    };
    let log_sink = state.log_sink_snapshot();

    let leader_state = Arc::new(LeaderState {
        db: db_conn,
        hlc: Arc::new(std::sync::Mutex::new(hlc_clone)),
        app_handle: app.clone(),
        space_id: space_id.clone(),
        connected_peers: Arc::new(RwLock::new(HashMap::new())),
        notification_senders: Arc::new(RwLock::new(HashMap::new())),
        invite_tokens: Arc::new(RwLock::new(existing_tokens)),
        reject_tracker: Arc::new(reject_tracker),
        dos_config: Arc::new(dos_config),
        flood_notifier: Arc::new(flood_notifier),
        critical_sink,
        log_sink,
    });

    let mut leaders = state.leader_state.write().await;
    leaders.insert(space_id.clone(), leader_state);
    drop(leaders);

    // The MultiSpaceLeaderHandler is registered once during `peer_storage_start`
    // with the device identity already loaded — re-registering here would
    // overwrite it with a fresh handler whose `own_identity` slot is empty,
    // breaking the server-initiated quic_did_auth handshake. The handler
    // already holds an `Arc` to `state.leader_state`, so the leader row we
    // just inserted is visible to it without any further wiring.

    eprintln!("[SpaceDelivery] Started leader mode for space {space_id}");
    Ok(())
}

/// Broadcast an MLS commit via the local leader buffer.
/// Called by frontend after mls_remove_member (or other commit-producing operations).
#[tauri::command]
pub async fn local_delivery_broadcast_commit(
    state: State<'_, AppState>,
    space_id: String,
    commit: Vec<u8>,
) -> Result<(), String> {
    let leader_state = super::peers::get_leader_state(&state, &space_id).await?;

    // Store commit in buffer
    let msg_id = super::super::buffer::store_message(
        &leader_state.db,
        &space_id,
        "leader",
        "commit",
        &commit,
    )
    .map_err(|e| format!("Failed to store commit: {e}"))?;

    // Track pending ACKs from all space members (not just connected peers)
    let expected_dids: Vec<String> =
        super::super::buffer::get_space_member_dids(&leader_state.db, &space_id)
            .unwrap_or_default();

    if !expected_dids.is_empty() {
        let _ = super::super::buffer::store_pending_commit(
            &leader_state.db,
            &space_id,
            msg_id,
            &expected_dids,
        );
    }

    // Broadcast notification to all connected peers
    let senders = leader_state.notification_senders.read().await;
    for (_, sender) in senders.iter() {
        let _ = sender.try_send(super::super::protocol::Notification::Mls {
            space_id: space_id.clone(),
            message_type: "commit".to_string(),
        });
    }

    eprintln!(
        "[SpaceDelivery] Broadcast commit for space {space_id} (msg_id={msg_id}, expected_acks={})",
        expected_dids.len()
    );
    Ok(())
}

/// Stop leader mode for a space — clears buffers and removes from leader map.
/// The MultiSpaceLeaderHandler stays registered (handles PushInvite even with empty map).
#[tauri::command]
pub async fn local_delivery_stop(
    state: State<'_, AppState>,
    space_id: String,
) -> Result<(), String> {
    super::super::buffer::clear_buffers(&DbConnection(state.db.0.clone()), &space_id)
        .map_err(|e| e.to_string())?;

    state.leader_state.write().await.remove(&space_id);

    eprintln!("[SpaceDelivery] Stopped leader mode for space {space_id}");
    Ok(())
}

/// Get the current delivery status.
#[tauri::command]
pub async fn local_delivery_status(state: State<'_, AppState>) -> Result<DeliveryStatus, String> {
    let leaders = state.leader_state.read().await;

    Ok(DeliveryStatus {
        is_leader: !leaders.is_empty(),
        active_spaces: leaders.keys().cloned().collect(),
        connected_peers: vec![],
        buffered_messages: 0,
        buffered_welcomes: 0,
        buffered_key_packages: 0,
    })
}
