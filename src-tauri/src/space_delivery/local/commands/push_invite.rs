//! PushInvite command — direct invite delivery to a peer's device via QUIC.

use tauri::{Emitter, State};

use crate::database::DbConnection;
use crate::AppState;

use super::super::types::{OutboxAttemptError, PeerConnectedEvent};

/// Push an invite directly to a peer's device via QUIC.
/// The peer creates a dummy space + pending invite locally.
#[tauri::command]
pub async fn local_delivery_push_invite(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    target_endpoint_id: String,
    space_id: String,
    space_name: String,
    space_type: String,
    token_id: String,
    capabilities: Vec<String>,
    include_history: bool,
    inviter_did: String,
    inviter_label: Option<String>,
    inviter_avatar: Option<String>,
    inviter_avatar_options: Option<String>,
    space_endpoints: Vec<String>,
    origin_url: Option<String>,
    expires_at: String,
    inviter_relay_url: Option<String>,
) -> Result<bool, OutboxAttemptError> {
    let log = |level: &str, msg: &str| {
        let _ =
            crate::logging::insert_log(&state, level, "PushInvite-Send", None, msg, None, "rust");
    };

    // Helper: classify recoverable preconditions as transient so the outbox
    // retries until expiry instead of giving up after a process restart that
    // hasn't finished bringing peer_storage back up.
    let transient = |reason: String| OutboxAttemptError {
        reason,
        transient: true,
    };
    let permanent = |reason: String| OutboxAttemptError {
        reason,
        transient: false,
    };

    log(
        "info",
        &format!(
            "Sending → target={} space={} token={}",
            &target_endpoint_id[..16.min(target_endpoint_id.len())],
            &space_id[..8.min(space_id.len())],
            &token_id[..8.min(token_id.len())]
        ),
    );

    let endpoint = state.peer_storage.read().await;
    if !endpoint.is_running() {
        log("error", "ABORT: peer endpoint not running");
        return Err(transient("Peer endpoint not running".to_string()));
    }
    let iroh_endpoint = endpoint
        .endpoint_ref()
        .ok_or_else(|| transient("Endpoint not running".to_string()))?
        .clone();
    let configured_relay = endpoint.configured_relay_url().cloned();
    drop(endpoint);

    // PushInvite has no per-request relay payload — fall back through
    // configured → live relay only.
    let (addr, relay) = super::super::quic_retry::build_endpoint_addr_with_relay(
        &iroh_endpoint,
        &target_endpoint_id,
        None,
        configured_relay.as_ref(),
    )
    .map_err(|e| permanent(format!("Invalid endpoint ID: {e}")))?;
    match &relay {
        Some(url) => log("info", &format!("Connecting via relay: {url}")),
        None => log("warn", "Connecting without relay (mDNS only)"),
    }
    log(
        "info",
        &format!(
            "Connecting to {target_endpoint_id} (relay={})",
            relay.is_some()
        ),
    );

    // The inviter authenticates as themself; the inviter_did inside the
    // payload must match the connection-verified DID (C8 enforces). Capture
    // the value before moving `inviter_did` into the request struct so the
    // signing key for `inviter_did` can be loaded from `haex_identities`.
    let inviter_did_for_auth = inviter_did.clone();

    let request = super::super::protocol::Request::PushInvite {
        space_id,
        space_name,
        space_type,
        token_id,
        capabilities,
        include_history,
        inviter_did,
        inviter_label,
        inviter_avatar,
        inviter_avatar_options,
        space_endpoints,
        origin_url,
        expires_at,
        inviter_relay_url,
    };

    let bytes = super::super::protocol::encode(&request)
        .map_err(|e| permanent(format!("Encode error: {e}")))?;

    let db_for_identity = DbConnection(state.db.0.clone());
    let inviter_identity = super::super::quic_retry::load_signing_identity_for_did(
        &db_for_identity,
        &inviter_did_for_auth,
    )
    .map_err(|e| {
        log("error", &format!("identity load failed: {e}"));
        // Missing / drifted identity row won't fix itself by retrying — the
        // user has to repair the identity. Surface immediately.
        permanent(e.to_string())
    })?;

    // QUIC connect + send + read with automatic retry on transient failures.
    // `send_request_with_retry` itself retries connection-level blips; any
    // error reaching us here is either a real permanent failure (auth reject,
    // protocol mismatch) or a transient condition that persisted across all
    // local retries — we propagate the transient flag so the outbox can decide
    // whether to schedule another attempt or surface FAILED.
    let response = super::super::quic_retry::send_request_with_retry(
        "PushInvite-Send",
        &iroh_endpoint,
        addr,
        &inviter_did_for_auth,
        &inviter_identity.signing_key,
        &bytes,
    )
    .await
    .map_err(|e| {
        let is_transient = e.is_transient();
        log(
            "error",
            &format!("QUIC send failed (transient={is_transient}): {e}"),
        );
        OutboxAttemptError {
            reason: format!("{e}"),
            transient: is_transient,
        }
    })?;

    // Best-effort: tell the rest of the app we just verified this endpoint
    // is live. Other outbox rows targeting the same endpoint can flush now
    // instead of waiting out their backoff. Emit before returning so the
    // event fires even when `accepted=false` (we still proved liveness).
    let _ = app.emit_to(
        "main",
        crate::event_names::EVENT_PEER_CONNECTED,
        PeerConnectedEvent {
            endpoint_id: target_endpoint_id.clone(),
            // We initiated the connection; the remote's DID was verified
            // server-side via the challenge we responded to but isn't
            // carried back to this layer. None is fine for the flush use
            // case — the listener filters by endpoint_id only.
            verified_did: None,
        },
    );

    match &response {
        super::super::protocol::Response::PushInviteAck { accepted } => {
            log("info", &format!("Response: accepted={accepted}"));
            Ok(*accepted)
        }
        super::super::protocol::Response::Error { message } => {
            log("error", &format!("Response: remote error={message}"));
            // Server-side rejections come from the auth gate or protocol
            // handler — these encode a permanent decision (audience
            // mismatch, unknown capability, policy reject). Retrying won't
            // change the outcome.
            Err(permanent(format!("Remote error: {message}")))
        }
        _ => {
            log("error", "Response: unexpected variant");
            Err(permanent("Unexpected response".to_string()))
        }
    }
}
