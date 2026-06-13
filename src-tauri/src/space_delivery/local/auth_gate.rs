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
//! 3. **Expiry.** `crate::ucan::require_not_expired` — `validate_token` at
//!    Announce time enforced `exp`, but the cached `ValidatedUcan` survives
//!    the entire QUIC connection lifetime. A long-lived session can outlive
//!    its own UCAN — re-check on every gated request so reconnect-after-
//!    renew is the only way to keep talking.
//! 4. **Audience binding.** `crate::ucan::require_audience` — the cached
//!    UCAN must have been issued *to the connection-authenticated DID*.
//! 5. **Capability.** `crate::ucan::require_capability` — the UCAN grants
//!    at least the minimum capability the request requires for its space.
//! 6. **Active membership.** `super::ucan::is_active_space_member` —
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
//! Every reject branch writes a row to `haex_logs` via [`log_to_db`] with
//! `source = Request::op_name(&self)` and `metadata = {"subsystem":
//! "AuthGate"}`, so operators can filter the in-app log viewer either by
//! per-op `source` or by subsystem (the latter disambiguates Gate rejects
//! from leader-side handler logs that share the same `op_name`).
//!
//! Severity is two-tier:
//!
//! - **`warn`** for the six peer-side reject paths (no peer entry, no
//!   cached UCAN, expired UCAN, audience mismatch, capability check,
//!   revoked membership). Message prefix `"reject: ..."`. These are normal
//!   — a misbehaving or probing peer.
//! - **`error`** for the one internal-failure path (Stage 6b: the
//!   membership-check SQL itself errored, e.g. DB locked, schema drift,
//!   disk full). Message prefix `"internal failure: ..."`. This signals
//!   an operator-actionable vault problem, not peer misbehaviour, and
//!   matches the severity convention `multi_leader.rs` / `push_invite.rs`
//!   use for analogous internal failures.
//!
//! Rows are CRDT-synced to the owner; peers never see the audit log
//! directly. Pre-T6, the SyncPush / SyncPull arms wrote these rows directly;
//! the AuthGate consolidation briefly lost that visibility, restored here.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use tokio::sync::RwLock;

use crate::crdt::hlc::HlcService;
use crate::database::DbConnection;
use crate::logging::{log_to_db, log_truncate};
use crate::space_delivery::local::protocol::{Request, Response};
use crate::space_delivery::local::types::ConnectedPeer;
use crate::ucan::{require_audience, require_capability, require_not_expired, ValidatedUcan};

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

    // Closure for the six reject paths below: emit one `haex_logs` row with
    // the AuthGate subsystem tag and build the `Response::Error` returned to
    // the caller. Captures `db`, `hlc`, `op` once so every branch reads as
    // (level, log_msg, peer_msg) — the three things that actually differ.
    //
    // `peer_msg` is owned (`String`) on purpose: most callers want a fixed
    // `"Access denied: …"` literal, but the capability arm needs to embed
    // the underlying error via `format!`. Taking `String` lets both cases
    // pass without an extra `.to_string()` on the literal side.
    let gate_reject = |level: &str, log_msg: String, peer_msg: String| -> Response {
        log_to_db(
            db,
            hlc,
            level,
            op,
            &log_msg,
            Some(serde_json::json!({"subsystem": "AuthGate"})),
        );
        Response::Error { message: peer_msg }
    };

    // 2. Cache lookup. Split into three outcomes so the diagnostics distinguish
    //    "no peer entry at all" (forged endpoint-id / evicted connection) from
    //    "peer present but cached UCAN is `None`" (ClaimInvite without
    //    follow-up Announce). The peer-facing message stays the same — vague
    //    is good — only the log differs.
    //
    //    Note on lock scope: we resolve the outcome **inside** the
    //    `connected_peers.read().await` block, then drop the guard before
    //    emitting the audit row. `log_to_db` is a synchronous SQLite write
    //    under a separate `std::sync::Mutex` (db.0). Holding the tokio
    //    RwLock read guard across that write would queue concurrent reject
    //    paths and block any pending `connected_peers.write().await` (the
    //    Announce / peer-cleanup writers) — a measurable starvation window
    //    under a reject-flood. The bigger DoS-defence design (per-peer rate
    //    limits + user notification) is tracked separately, see
    //    docs/plans/2026-06-13-leader-reject-rate-limit.md.
    enum PeerLookup {
        Hit(ValidatedUcan),
        NoEntry,
        NoUcan,
    }
    let lookup = {
        let peers = connected_peers.read().await;
        match peers.get(peer_endpoint_id) {
            None => PeerLookup::NoEntry,
            Some(peer) => match peer.validated_ucan.clone() {
                Some(v) => PeerLookup::Hit(v),
                None => PeerLookup::NoUcan,
            },
        }
    };

    let validated = match lookup {
        PeerLookup::Hit(v) => v,
        PeerLookup::NoEntry => {
            return Err(gate_reject(
                "warn",
                format!("reject: no peer entry for endpoint={peer_endpoint_id} (forged endpoint-id or evicted connection?)"),
                "Access denied: must Announce before sending other requests".to_string(),
            ));
        }
        PeerLookup::NoUcan => {
            return Err(gate_reject(
                "warn",
                format!("reject: peer endpoint={peer_endpoint_id} has no cached UCAN (ClaimInvite without Announce?)"),
                "Access denied: must Announce before sending other requests".to_string(),
            ));
        }
    };

    // 3. Expiry re-check. `validate_token` enforced `exp` at Announce time,
    //    but `ConnectedPeer::validated_ucan` is held for the lifetime of the
    //    QUIC connection — a long-lived session can outlast its own UCAN.
    //    Re-checking here forces a reconnect-after-renew once `expires_at`
    //    is reached.
    if let Err(e) = require_not_expired(&validated) {
        let aud_short = log_truncate(&validated.audience, 24);
        return Err(gate_reject(
            "warn",
            format!("reject: cached UCAN expired (endpoint={peer_endpoint_id} aud={aud_short} expires_at={exp} err={e})", exp = validated.expires_at),
            "Access denied: cached UCAN expired, please Announce again".to_string(),
        ));
    }

    // 4. Audience binding. The underlying `UcanVerifyError::AudienceMismatch`
    //    Display includes both DIDs — that's useful in logs but is an
    //    enumeration aid for an attacker probing endpoints, so the
    //    peer-facing message is fixed-string and the detail goes only to the
    //    log (with the same `log_truncate` 24-char cap as peer_storage and
    //    multi_leader so DIDs don't sprawl across diagnostic lines).
    if let Err(e) = require_audience(&validated, verified_did) {
        let aud_short = log_truncate(&validated.audience, 24);
        let verified_short = log_truncate(verified_did, 24);
        return Err(gate_reject(
            "warn",
            format!("reject: UCAN audience != verified peer DID (endpoint={peer_endpoint_id} aud={aud_short} verified={verified_short} err={e})"),
            "Access denied: UCAN audience does not match verified peer DID".to_string(),
        ));
    }

    // 5. Capability.
    if let Err(e) = require_capability(&validated, space_id, required) {
        return Err(gate_reject(
            "warn",
            format!("reject: capability check failed (endpoint={peer_endpoint_id} space={space_id} required={required:?} err={e})"),
            format!("Access denied: {e}"),
        ));
    }

    // 6. Active membership (revocation kill-switch).
    match super::ucan::is_active_space_member(db, space_id, &validated.audience) {
        Ok(true) => Ok(Some(validated)),
        Ok(false) => {
            let aud_short = log_truncate(&validated.audience, 24);
            let space_short = log_truncate(space_id, 24);
            Err(gate_reject(
                "warn",
                format!("reject: not an active member (endpoint={peer_endpoint_id} aud={aud_short} space={space_short})"),
                "Access denied: not an active member of this space".to_string(),
            ))
        }
        Err(e) => {
            // Distinct severity from the peer-side rejects above: this branch
            // signals an internal vault failure (DB locked, schema drift,
            // disk full, ...), not peer misbehaviour. Logged at `error` level
            // and prefixed `internal failure:` so an operator filtering
            // haex_logs by level=error or by message prefix can separate
            // actionable DB outages from benign peer rejects.
            let aud_short = log_truncate(&validated.audience, 24);
            let space_short = log_truncate(space_id, 24);
            Err(gate_reject(
                "error",
                format!("internal failure: membership check DB error (endpoint={peer_endpoint_id} aud={aud_short} space={space_short} err={e})"),
                format!("Membership check failed: {e}"),
            ))
        }
    }
}

#[cfg(test)]
#[path = "auth_gate_tests.rs"]
mod tests;
