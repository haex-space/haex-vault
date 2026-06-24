//! Sync-loop lifecycle: initial connect, the reconnect-on-failure outer loop,
//! and mode-aware session establishment.

use std::sync::Arc;
use std::time::Duration;

use tauri::Emitter;
use tokio::sync::{watch, Notify};

use super::super::error::DeliveryError;
use super::super::peer::PeerSession;
use super::super::push_cursor::{load_last_mls_cursor, load_last_push_hlc};
use super::{cycle, log_sync, SyncLoopHandle, SyncMode, MAX_RECONNECT_BACKOFF, POLL_INTERVAL};
use crate::database::DbConnection;

/// Open a `PeerSession` appropriate for `mode`.
///
/// [`SyncMode::SpaceScoped`] uses [`PeerSession::connect`] (DID-auth + UCAN +
/// Announce) — the existing shared-space path, unchanged. [`SyncMode::OwnerVault`]
/// uses [`PeerSession::connect_owner`] (DID-auth only, NO UCAN, NO Announce):
/// the owner's own devices have no UCAN for themselves, and the security gate
/// is the same-owner DID-auth handshake which `connect_owner` still runs.
#[allow(clippy::too_many_arguments)]
pub(super) async fn connect_for_mode(
    mode: &SyncMode,
    iroh_endpoint: &iroh::Endpoint,
    leader_endpoint_id: &str,
    leader_relay_url: Option<&str>,
    space_id: &str,
    our_did: &str,
    our_signing_key: &ed25519_dalek::SigningKey,
    our_endpoint_id: &str,
    db: &DbConnection,
) -> Result<PeerSession, DeliveryError> {
    match mode {
        SyncMode::SpaceScoped => {
            PeerSession::connect(
                iroh_endpoint,
                leader_endpoint_id,
                leader_relay_url,
                space_id,
                our_did,
                our_signing_key,
                our_endpoint_id,
                Some("sync-loop"),
                db,
            )
            .await
        }
        SyncMode::OwnerVault { .. } => {
            PeerSession::connect_owner(
                iroh_endpoint,
                leader_endpoint_id,
                leader_relay_url,
                our_did,
                our_signing_key,
                our_endpoint_id,
            )
            .await
        }
    }
}

/// Start the sync loop as a peer connecting to a leader.
///
/// The loop will:
/// 1. Connect to the leader via `PeerSession`
/// 2. Scan dirty tables for outbound changes
/// 3. Push changes to the leader
/// 4. Pull changes from the leader
/// 5. Apply them to the local DB
/// 6. Emit Tauri events for frontend UI refresh
/// 7. Repeat with a poll interval, stoppable via the returned handle
pub async fn start_peer_sync_loop(
    db: DbConnection,
    iroh_endpoint: iroh::Endpoint,
    mode: SyncMode,
    leader_endpoint_id: String,
    leader_relay_url: Option<String>,
    space_id: String,
    our_did: String,
    our_endpoint_id: String,
    device_id: String,
    app_handle: tauri::AppHandle,
) -> Result<SyncLoopHandle, DeliveryError> {
    log_sync(
        &app_handle,
        "info",
        &format!(
            "connecting: space={} leader={} our_did={}",
            &space_id[..8.min(space_id.len())],
            &leader_endpoint_id[..16.min(leader_endpoint_id.len())],
            &our_did[..24.min(our_did.len())],
        ),
    );

    // Load the identity's signing key once for the lifetime of this loop.
    // Every (re)connect drives the server-initiated quic_did_auth handshake,
    // and the loop reconnects internally on transient failure — keeping the
    // key in-process avoids repeated DB hits + private-key derivations.
    let our_identity = super::super::quic_retry::load_signing_identity_for_did(&db, &our_did)?;

    // Establish initial connection. UCAN is loaded from the DB inside
    // `PeerSession::connect`, so reconnect-after-expiry gets a fresh token
    // without any state plumbing up here.
    // Bounded retry around the initial connect. A single transient relay/
    // handshake hiccup — common in the relay-only docker-split CI network —
    // would otherwise hard-fail the whole sync loop, leaving the leader's
    // `connected_peers` empty until some external trigger reconnects, which
    // surfaces as the 110s "Vault B device row never synced" cross-vault
    // flake. The internal reconnect loop only runs *after* this first connect
    // succeeds, so the initial attempt needs its own retry. `connect` carries
    // no timeout of its own, so each attempt is bounded here too. Only
    // `ConnectionFailed` is retried; `AccessDenied`/`ProtocolError` are
    // deterministic and fail fast.
    let session = match super::super::quic_retry::retry_transient(
        "sync-loop initial connect",
        || async {
            match tokio::time::timeout(
                Duration::from_secs(10),
                connect_for_mode(
                    &mode,
                    &iroh_endpoint,
                    &leader_endpoint_id,
                    leader_relay_url.as_deref(),
                    &space_id,
                    &our_did,
                    &our_identity.signing_key,
                    &our_endpoint_id,
                    &db,
                ),
            )
            .await
            {
                Ok(result) => result,
                Err(_) => Err(DeliveryError::ConnectionFailed {
                    reason: "initial connect timed out after 10s".to_string(),
                }),
            }
        },
        |e| matches!(e, DeliveryError::ConnectionFailed { .. }),
    )
    .await
    {
        Ok(s) => {
            log_sync(
                &app_handle,
                "info",
                &format!(
                    "connected: space={} leader={}",
                    &space_id[..8.min(space_id.len())],
                    &leader_endpoint_id[..16.min(leader_endpoint_id.len())],
                ),
            );
            s
        }
        Err(e) => {
            log_sync(
                &app_handle,
                "error",
                &format!(
                    "connect failed after retries: space={} leader={} err={}",
                    &space_id[..8.min(space_id.len())],
                    &leader_endpoint_id[..16.min(leader_endpoint_id.len())],
                    e,
                ),
            );
            return Err(e);
        }
    };

    let (stop_tx, stop_rx) = watch::channel(false);
    let wakeup = Arc::new(Notify::new());

    let task = tokio::spawn(run_sync_loop(
        db,
        iroh_endpoint,
        session,
        mode,
        leader_endpoint_id,
        leader_relay_url,
        space_id,
        our_did,
        our_identity.signing_key,
        our_endpoint_id,
        device_id,
        app_handle,
        stop_rx,
        wakeup.clone(),
    ));

    Ok(SyncLoopHandle {
        stop_sender: stop_tx,
        wakeup,
        task,
    })
}

/// The main sync loop. Runs until the stop signal is received.
#[allow(clippy::too_many_arguments)]
async fn run_sync_loop(
    db: DbConnection,
    iroh_endpoint: iroh::Endpoint,
    mut session: PeerSession,
    mode: SyncMode,
    leader_endpoint_id: String,
    leader_relay_url: Option<String>,
    space_id: String,
    our_did: String,
    our_signing_key: ed25519_dalek::SigningKey,
    our_endpoint_id: String,
    device_id: String,
    app_handle: tauri::AppHandle,
    mut stop_rx: watch::Receiver<bool>,
    wakeup: Arc<Notify>,
) {
    let mut last_push_hlc: Option<String> = load_last_push_hlc(&db, &space_id, &device_id);
    let mut last_pull_timestamp: Option<String> = None;
    let mut last_mls_message_id: Option<i64> = load_last_mls_cursor(&db, &space_id, &device_id);
    let mut key_packages_refilled = false;

    // Translate our device UUID into the uhlc node-id form once per session
    // so the push scanner can fast-filter ping-pong rows. A non-UUID device_id
    // (only the in-process leader path uses one — see leader.rs) yields None
    // and disables the filter, which is the correct fallback: that path does
    // not push back to itself anyway.
    let our_node: Option<u128> = crate::crdt::hlc::device_uuid_to_hlc_node(&device_id);
    if our_node.is_none() {
        log_sync(
            &app_handle,
            "warn",
            &format!(
                "device_id is not a UUID, scanner origin filter disabled: device={}",
                &device_id[..16.min(device_id.len())],
            ),
        );
    }

    // Resolve our identity UUID once for the membership-row ownership filter.
    // The leader writes haex_space_members rows for other members (during
    // ClaimInvite) using the leader's HLC node, so the origin filter alone is
    // insufficient — we also need to drop rows whose identity_id ≠ ours.
    let our_identity_id: Option<String> = crate::database::core::select_with_crdt(
        "SELECT id FROM haex_identities WHERE did = ?1 LIMIT 1".to_string(),
        vec![serde_json::Value::String(our_did.clone())],
        &db,
    )
    .ok()
    .and_then(|rows| rows.into_iter().next())
    .and_then(|row| row.into_iter().next())
    .and_then(|v| match v {
        serde_json::Value::String(s) => Some(s),
        _ => None,
    });

    // Determine once whether this member may push user-content tables
    // (haex_peer_shares). Read-only members must not: the leader rejects any
    // batch containing non-membership-system rows without Write capability,
    // which would leave the push cursor stuck and block MLS KeyPackage uploads.
    let can_push_user_content = super::super::ucan::has_write_capability(&db, &space_id, &our_did);
    if !can_push_user_content {
        log_sync(
            &app_handle,
            "info",
            &format!(
                "read-only member: push restricted to membership-system tables for space={}",
                &space_id[..8.min(space_id.len())],
            ),
        );
    }

    log_sync(
        &app_handle,
        "info",
        &format!(
            "started: space={} leader={} our_did={} cursor={:?}",
            &space_id[..8.min(space_id.len())],
            &leader_endpoint_id[..16.min(leader_endpoint_id.len())],
            &our_did[..24.min(our_did.len())],
            last_push_hlc.as_deref(),
        ),
    );

    loop {
        // Check if stop was requested
        if *stop_rx.borrow() {
            log_sync(
                &app_handle,
                "info",
                &format!(
                    "stop signal received: space={}",
                    &space_id[..8.min(space_id.len())]
                ),
            );
            break;
        }

        match cycle::run_sync_cycle(
            &db,
            &session,
            &mode,
            &space_id,
            &device_id,
            our_node,
            can_push_user_content,
            our_identity_id.as_deref(),
            &our_endpoint_id,
            &leader_endpoint_id,
            &app_handle,
            &mut last_push_hlc,
            &mut last_pull_timestamp,
            &mut last_mls_message_id,
            &mut key_packages_refilled,
        )
        .await
        {
            Ok(()) => {
                // Cycle completed successfully, wait for next cycle, an
                // external wake-up (force_sync), or a stop signal.
                tokio::select! {
                    _ = tokio::time::sleep(POLL_INTERVAL) => {},
                    _ = wakeup.notified() => {},
                    _ = stop_rx.changed() => {
                        log_sync(&app_handle, "info", &format!("stop during sleep: space={}", &space_id[..8.min(space_id.len())]));
                        break;
                    },
                }
            }
            Err(e) => {
                let endpoint_dead_at_failure = iroh_endpoint.is_closed();
                log_sync(
                    &app_handle,
                    "error",
                    &format!(
                        "cycle failed: space={} err={} endpoint_closed={}",
                        &space_id[..8.min(space_id.len())],
                        e,
                        endpoint_dead_at_failure,
                    ),
                );

                // Attempt reconnection with exponential backoff
                let mut backoff = Duration::from_secs(5);
                let mut reconnect_attempt: u32 = 0;
                loop {
                    if *stop_rx.borrow() {
                        eprintln!("[SyncLoop] Stop signal received during reconnect, exiting");
                        session.close();
                        return;
                    }

                    reconnect_attempt += 1;
                    let endpoint_closed_now = iroh_endpoint.is_closed();
                    eprintln!(
                        "[SyncLoop] Reconnecting in {}s (attempt {}, endpoint_closed={})...",
                        backoff.as_secs(),
                        reconnect_attempt,
                        endpoint_closed_now,
                    );

                    // Emit error event for frontend (main window only).
                    // Tauri v2 emit() broadcasts to every webview — extensions
                    // must not learn about p2p sync state for other spaces.
                    let _ = app_handle.emit_to(
                        "main",
                        "local-sync-error",
                        serde_json::json!({
                            "spaceId": space_id,
                            "error": e.to_string(),
                            "reconnecting": true,
                            "endpointClosed": endpoint_closed_now,
                            "attempt": reconnect_attempt,
                        }),
                    );

                    // Wait for backoff duration or stop signal
                    tokio::select! {
                        _ = tokio::time::sleep(backoff) => {},
                        _ = stop_rx.changed() => {
                            eprintln!("[SyncLoop] Stop signal received during backoff, exiting");
                            session.close();
                            return;
                        },
                    }

                    // Try to reconnect — in space mode this pulls the current
                    // UCAN from the DB so a token renewed during the outage
                    // takes effect here; in owner mode reconnect re-runs only
                    // the DID-auth handshake (no UCAN). Bounded by the same
                    // 10s timeout as the initial connect so a hung handshake
                    // can't wedge the loop and the next iteration still
                    // observes the stop signal.
                    let reconnect_result = match tokio::time::timeout(
                        Duration::from_secs(10),
                        connect_for_mode(
                            &mode,
                            &iroh_endpoint,
                            &leader_endpoint_id,
                            leader_relay_url.as_deref(),
                            &space_id,
                            &our_did,
                            &our_signing_key,
                            &our_endpoint_id,
                            &db,
                        ),
                    )
                    .await
                    {
                        Ok(result) => result,
                        Err(_) => Err(DeliveryError::ConnectionFailed {
                            reason: "reconnect timed out after 10s".to_string(),
                        }),
                    };
                    match reconnect_result {
                        Ok(new_session) => {
                            log_sync(
                                &app_handle,
                                "info",
                                &format!(
                                    "reconnected: space={} after {} attempt(s)",
                                    &space_id[..8.min(space_id.len())],
                                    reconnect_attempt,
                                ),
                            );
                            session = new_session;
                            break;
                        }
                        Err(reconnect_err) => {
                            let endpoint_closed_post = iroh_endpoint.is_closed();
                            log_sync(
                                &app_handle,
                                "warn",
                                &format!(
                                "reconnect failed: space={} attempt={} err={} endpoint_closed={}",
                                &space_id[..8.min(space_id.len())],
                                reconnect_attempt,
                                reconnect_err,
                                endpoint_closed_post,
                            ),
                            );
                            backoff = (backoff * 2).min(MAX_RECONNECT_BACKOFF);
                        }
                    }
                }
            }
        }
    }

    session.close();
    eprintln!("[SyncLoop] Stopped for space {}", space_id);
}
