//! Unified pre-dispatch authorisation gate for inbound space-delivery
//! requests.
//!
//! This is the leader-side analogue of `peer_storage::handle_stream`'s
//! Layers 1.25 (cached UCAN lookup) and 2 (capability + active-membership
//! enforcement), lifted out of `leader.rs::handle_delivery_request` so that
//! every request variant flows through the same checks in the same order.
//!
//! ## Pipeline
//!
//! For each inbound `Request`, in order, short-circuiting on the first
//! failure:
//!
//! 1. **Bypass.** If `Request::required_capability()` returns `None`
//!    (`Announce`, `ClaimInvite`, `PushInvite`), return `Ok(None)` — these
//!    requests bootstrap the very state the rest of the pipeline depends
//!    on, so gating them here would be circular.
//! 2. **Cache lookup.** The peer must have already completed an `Announce`
//!    on this connection, which populates `ConnectedPeer::validated_ucan`.
//!    A missing entry — or an entry whose `validated_ucan` is `None` — is
//!    rejected with `"Access denied: must Announce before sending other
//!    requests"`.
//! 3. **Audience binding.** `crate::ucan::require_audience` — the cached
//!    UCAN must have been issued *to the connection-authenticated DID*.
//! 4. **Capability.** `crate::ucan::require_capability` — the UCAN grants
//!    at least the minimum capability the request requires for its space.
//! 5. **Active membership.** `super::ucan::is_active_space_member` —
//!    revocation kill-switch: a tombstoned member's UCAN remains
//!    cryptographically valid, but every request still fails here.
//!
//! ## `None`-rejection invariant (T2 code-review follow-up)
//!
//! Step 2 must reject when the cached `ConnectedPeer::validated_ucan` is
//! `Option::None`. That field is `Option` purely for `Deserialize` / `TS`
//! reasons (the frontend never sees a real UCAN); on the wire a peer that
//! has done `ClaimInvite` without a follow-up `Announce` lands in the
//! `connected_peers` map with `validated_ucan = None`. Silently treating
//! that as a pass would defeat the entire gate — hence the explicit
//! `None`-arm reject below, never `unwrap()` / `expect()`.

use std::collections::HashMap;

use tokio::sync::RwLock;

use crate::database::DbConnection;
use crate::space_delivery::local::protocol::{Request, Response};
use crate::space_delivery::local::types::ConnectedPeer;
use crate::ucan::{require_audience, require_capability, ValidatedUcan};

/// Authorise an inbound `Request` against the connection's cached UCAN and
/// the current membership state.
///
/// Returns:
/// - `Ok(Some(ValidatedUcan))` — request authorised; the caller may use the
///   UCAN for origin attribution (e.g. `authored_by_did`).
/// - `Ok(None)` — request bypasses the gate (see `Request::required_capability`).
/// - `Err(Response::Error { .. })` — auth failed; the caller should send the
///   response back to the peer and return.
pub async fn authorize_request(
    request: &Request,
    verified_did: &str,
    peer_endpoint_id: &str,
    connected_peers: &RwLock<HashMap<String, ConnectedPeer>>,
    db: &DbConnection,
) -> Result<Option<ValidatedUcan>, Response> {
    // 1. Bypass — requests that bootstrap the gate's own preconditions.
    let required = match request.required_capability() {
        Some(level) => level,
        None => return Ok(None),
    };

    let space_id = request.space_id_of();

    // 2. Cache lookup. Both "no peer entry" and "peer entry without UCAN"
    //    are rejected with the same message — see the module-level
    //    `None`-rejection invariant.
    let validated = {
        let peers = connected_peers.read().await;
        match peers.get(peer_endpoint_id).and_then(|p| p.validated_ucan.clone()) {
            Some(v) => v,
            None => {
                return Err(Response::Error {
                    message: "Access denied: must Announce before sending other requests"
                        .to_string(),
                });
            }
        }
    };

    // 3. Audience binding.
    require_audience(&validated, verified_did).map_err(|e| Response::Error {
        message: format!("Access denied: {e}"),
    })?;

    // 4. Capability.
    require_capability(&validated, space_id, required).map_err(|e| Response::Error {
        message: format!("Access denied: {e}"),
    })?;

    // 5. Active membership (revocation kill-switch).
    match super::ucan::is_active_space_member(db, space_id, &validated.audience) {
        Ok(true) => Ok(Some(validated)),
        Ok(false) => Err(Response::Error {
            message: "Access denied: not an active member of this space".to_string(),
        }),
        Err(e) => Err(Response::Error {
            message: format!("Membership check failed: {e}"),
        }),
    }
}

#[cfg(test)]
#[path = "auth_gate_tests.rs"]
mod tests;
