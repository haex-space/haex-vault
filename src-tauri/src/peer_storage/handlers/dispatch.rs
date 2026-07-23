//! Stream-level dispatcher: validate UCAN, dispatch to per-request handlers.

use std::collections::HashSet;
use tokio::sync::RwLock;

use crate::peer_storage::endpoint::PeerState;
use crate::peer_storage::error::PeerStorageError;
use crate::peer_storage::helpers::find_space_for_path;
use crate::peer_storage::protocol::{self, Request, Response};

use super::common::send_response_and_finish;
use super::create_directory::handle_create_directory;
use super::delete::handle_delete;
use super::list::handle_list;
use super::manifest::handle_manifest;
use super::read::handle_read;
use super::stat::handle_stat;
use super::write::handle_write;

pub(in crate::peer_storage) async fn handle_stream(
    mut send: iroh::endpoint::SendStream,
    recv: &mut iroh::endpoint::RecvStream,
    state: &RwLock<PeerState>,
    allowed_spaces: &HashSet<String>,
    verified_remote_did: &str,
) -> Result<(), PeerStorageError> {
    let request =
        protocol::read_request(recv)
            .await
            .map_err(|e| PeerStorageError::ProtocolError {
                reason: e.to_string(),
            })?;

    // TODO(phase2-task4): replace with DB config lookup
    // (haex_vault_settings.max_ucan_chain_depth).
    const DEFAULT_MAX_UCAN_CHAIN_DEPTH: usize = 5;

    // ── Layer 1 (peek): parse UCAN structure + verify signature + expiry.
    // The target `space_id` is only known after path routing below, so we
    // parse cheaply here to inspect the leaf's capability map and defer the
    // full chain-walk pipeline (`validate_token`) until we know which space
    // to bind against. `parse_ucan` verifies signature and expiry — the
    // capability + chain checks then happen inside `validate_token`. ──
    let ucan_token_str = request.ucan_token();
    let parsed_ucan = match crate::ucan::parse_ucan(ucan_token_str) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("[PeerStorage] UCAN parse failed: {e}");
            let resp = Response::Error {
                message: format!("UCAN validation failed: {e}"),
            };
            send_response_and_finish(&mut send, &resp).await.ok();
            return Ok(());
        }
    };

    // ── Layer 1.25: UCAN audience must equal the peer's cryptographically
    // verified DID for this connection. Without this check a peer P could
    // present a UCAN issued to a foreign DID Q over its own iroh transport
    // key — Layer 1 (signature) and the capability/space gates below would
    // both pass. The verified DID was bound to the connection during the
    // quic_did_auth handshake in handle_connection. ──
    if parsed_ucan.aud != verified_remote_did {
        let aud_short =
            crate::logging::log_truncate(&parsed_ucan.aud, crate::logging::LOG_TRUNCATE_DEFAULT);
        let verified_short =
            crate::logging::log_truncate(verified_remote_did, crate::logging::LOG_TRUNCATE_DEFAULT);
        eprintln!(
            "[PeerStorage] UCAN audience != verified peer DID: aud={aud_short} verified={verified_short}"
        );
        let resp = Response::Error {
            message: "Access denied: UCAN audience does not match verified peer DID".to_string(),
        };
        send_response_and_finish(&mut send, &resp).await.ok();
        return Ok(());
    }

    // ── Layer 1.5: narrow allowed_spaces to the intersection with the
    // UCAN's claimed spaces. The connection-accept gate is intentionally
    // coarse ("known peer = accepted"), so this is the first check that
    // ties the request to the peer's UCAN presentation:
    //
    // - If the intersection is empty, the peer is presenting a UCAN they
    //   cannot use (e.g. removed from every space the UCAN names). Reject.
    // - If the intersection is non-empty, use it as the effective allowed
    //   spaces for the rest of the request. This stops a peer with
    //   allowed = {A, B} and a UCAN for {A} from leaking share names in
    //   B via the root listing — handle_list would otherwise return
    //   everything in allowed_spaces.
    let effective_spaces: HashSet<String> = parsed_ucan
        .capabilities
        .keys()
        .filter(|space_id| allowed_spaces.contains(*space_id))
        .cloned()
        .collect();
    if effective_spaces.is_empty() {
        eprintln!(
            "[PeerStorage] Access denied: peer holds a UCAN for spaces it is not registered in"
        );
        let resp = Response::Error {
            message: "Access denied: peer not registered in any of the UCAN's spaces".to_string(),
        };
        send_response_and_finish(&mut send, &resp).await.ok();
        return Ok(());
    }
    let allowed_spaces = &effective_spaces;

    // ── Layer 2 (source of truth): resolve target space from path + run
    // the full Phase-2 pipeline (audience + capability + prf-chain walk +
    // self-certifying `space_id` binding) via `validate_token`. Requests
    // whose path does not land inside any effective share fall through
    // without a chain check — the handler below will short-circuit them
    // (e.g. `handle_list("/")` enumerates only share roots the peer is
    // already authorised for). ──
    let target_space_id = {
        let s = state.read().await;
        let path = match &request {
            Request::List { path, .. }
            | Request::Stat { path, .. }
            | Request::Read { path, .. }
            | Request::Manifest { path, .. }
            | Request::Write { path, .. }
            | Request::Delete { path, .. }
            | Request::CreateDirectory { path, .. } => path.as_str(),
        };
        find_space_for_path(&s.shares, allowed_spaces, path)
    };

    if let Some(space_id) = &target_space_id {
        let required = if request.requires_write() {
            crate::ucan::CapabilityLevel::Write
        } else {
            crate::ucan::CapabilityLevel::Read
        };

        if let Err(e) = crate::ucan::validate_token(
            ucan_token_str,
            space_id,
            verified_remote_did,
            required,
            DEFAULT_MAX_UCAN_CHAIN_DEPTH,
        ) {
            eprintln!("[PeerStorage] UCAN full-validation failed: {e}");
            let resp = Response::Error {
                message: format!("Access denied: {e}"),
            };
            send_response_and_finish(&mut send, &resp).await.ok();
            return Ok(());
        }
    }

    let response = match request {
        Request::List { path, .. } => handle_list(state, &path, allowed_spaces).await,
        Request::Stat { path, .. } => handle_stat(state, &path, allowed_spaces).await,
        Request::Manifest { path, .. } => handle_manifest(state, &path, allowed_spaces).await,
        Request::Read { path, range, .. } => {
            if let Err(e) = handle_read(&mut send, state, &path, range, allowed_spaces).await {
                eprintln!("[PeerStorage] Read error for '{path}': {e}");
                let error_resp = Response::Error {
                    message: format!("{e}"),
                };
                send_response_and_finish(&mut send, &error_resp).await.ok();
                return Err(e);
            }
            return Ok(());
        }
        Request::Write { path, size, .. } => {
            if let Err(e) = handle_write(&mut send, recv, state, &path, size, allowed_spaces).await
            {
                eprintln!("[PeerStorage] Write error for '{path}': {e}");
                let error_resp = Response::Error {
                    message: format!("{e}"),
                };
                send_response_and_finish(&mut send, &error_resp).await.ok();
                return Err(e);
            }
            return Ok(());
        }
        Request::Delete { path, to_trash, .. } => {
            handle_delete(state, &path, to_trash, allowed_spaces).await
        }
        Request::CreateDirectory { path, .. } => {
            handle_create_directory(state, &path, allowed_spaces).await
        }
    };

    send_response_and_finish(&mut send, &response).await
}
