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
//!
//! ## Audit logging
//!
//! Every reject branch writes a `warn` row to `haex_logs` via
//! [`log_to_db`], with `source = Request::op_name(&self)` so operators can
//! filter the in-app log viewer by op when triaging sync failures. Rows
//! are CRDT-synced to the owner; peers never see the audit log directly.
//! Pre-T6, the SyncPush / SyncPull arms wrote these rows directly; the
//! AuthGate consolidation briefly lost that visibility, restored here.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use tokio::sync::RwLock;

use crate::crdt::hlc::HlcService;
use crate::database::DbConnection;
use crate::logging::log_to_db;
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
    hlc: &Arc<Mutex<HlcService>>,
) -> Result<Option<ValidatedUcan>, Response> {
    // 1. Bypass — requests that bootstrap the gate's own preconditions.
    let required = match request.required_capability() {
        Some(level) => level,
        None => return Ok(None),
    };

    let space_id = request.space_id_of();
    let op = request.op_name();

    // 2. Cache lookup. Split into two arms so the diagnostics distinguish
    //    "no peer entry at all" (forged endpoint-id / evicted connection)
    //    from "peer present but cached UCAN is `None`" (ClaimInvite without
    //    follow-up Announce). The peer-facing message stays the same —
    //    vague is good — only the log differs.
    let validated = {
        let peers = connected_peers.read().await;
        let Some(peer) = peers.get(peer_endpoint_id) else {
            log_to_db(
                db,
                hlc,
                "warn",
                op,
                &format!("reject: no peer entry for endpoint={peer_endpoint_id} (forged endpoint-id or evicted connection?)"),
            );
            return Err(Response::Error {
                message: "Access denied: must Announce before sending other requests".to_string(),
            });
        };
        let Some(validated) = peer.validated_ucan.clone() else {
            log_to_db(
                db,
                hlc,
                "warn",
                op,
                &format!("reject: peer endpoint={peer_endpoint_id} has no cached UCAN (ClaimInvite without Announce?)"),
            );
            return Err(Response::Error {
                message: "Access denied: must Announce before sending other requests".to_string(),
            });
        };
        validated
    };

    // 3. Audience binding. The underlying `UcanVerifyError::AudienceMismatch`
    //    Display includes both DIDs — that's useful in logs but is an
    //    enumeration aid for an attacker probing endpoints, so the
    //    peer-facing message is fixed-string and the detail goes only to the
    //    log (with the same chars().take(24) truncation pattern used in
    //    peer_storage/handlers.rs to stay UTF-8-safe).
    if let Err(e) = require_audience(&validated, verified_did) {
        let aud_short: String = validated.audience.chars().take(24).collect();
        let verified_short: String = verified_did.chars().take(24).collect();
        log_to_db(
            db,
            hlc,
            "warn",
            op,
            &format!("reject: UCAN audience != verified peer DID (endpoint={peer_endpoint_id} aud={aud_short} verified={verified_short} err={e})"),
        );
        return Err(Response::Error {
            message: "Access denied: UCAN audience does not match verified peer DID".to_string(),
        });
    }

    // 4. Capability.
    if let Err(e) = require_capability(&validated, space_id, required) {
        log_to_db(
            db,
            hlc,
            "warn",
            op,
            &format!("reject: capability check failed (endpoint={peer_endpoint_id} space={space_id} required={required:?} err={e})"),
        );
        return Err(Response::Error {
            message: format!("Access denied: {e}"),
        });
    }

    // 5. Active membership (revocation kill-switch).
    match super::ucan::is_active_space_member(db, space_id, &validated.audience) {
        Ok(true) => Ok(Some(validated)),
        Ok(false) => {
            let aud_short: String = validated.audience.chars().take(24).collect();
            let space_short: String = space_id.chars().take(24).collect();
            log_to_db(
                db,
                hlc,
                "warn",
                op,
                &format!("reject: not an active member (endpoint={peer_endpoint_id} aud={aud_short} space={space_short})"),
            );
            Err(Response::Error {
                message: "Access denied: not an active member of this space".to_string(),
            })
        }
        Err(e) => {
            let aud_short: String = validated.audience.chars().take(24).collect();
            let space_short: String = space_id.chars().take(24).collect();
            log_to_db(
                db,
                hlc,
                "warn",
                op,
                &format!("reject: membership check DB error (endpoint={peer_endpoint_id} aud={aud_short} space={space_short} err={e})"),
            );
            Err(Response::Error {
                message: format!("Membership check failed: {e}"),
            })
        }
    }
}

#[cfg(test)]
#[path = "auth_gate_tests.rs"]
mod tests;
