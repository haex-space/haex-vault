//! UCAN validation + capability/audience/membership gates used by the
//! Announce bypass.

use super::super::protocol::Response;
use crate::database::core::with_connection;
use crate::database::DbConnection;
use crate::ucan::{
    read_max_ucan_chain_depth, require_audience, require_capability, validate_token, Cap,
    ValidatedUcan, MAX_UCAN_CHAIN_DEPTH_DEFAULT,
};

/// Validate a UCAN token carried in a space-delivery request and return a
/// structured Error response on any failure. Runs the full Phase-2 pipeline:
/// signature, expiry, audience, capability floor, `prf` chain walk to a
/// self-signed root, and self-certifying `space_id` binding.
///
/// The chain-walk depth cap is read *just-in-time* from
/// `haex_vault_settings` via [`read_max_ucan_chain_depth`] on every
/// Announce. Choice (a) from the task brief: the caller
/// (`handle_delivery_request`) already has `&state.db`, and Announce runs
/// exactly once per connection — the DB read is not on any hot path so
/// caching it would be pure overhead here. `peer_storage::handle_stream`
/// takes the opposite trade (cached atomic) because it runs per-request.
///
/// Used by the `Announce` bypass — the only request variant that reaches the
/// leader without a cached `ValidatedUcan`. Every subsequent request on the
/// same connection is authorised via the `auth_gate`, which reads the cached
/// UCAN populated here.
pub(super) fn require_valid_ucan(
    ucan_token: &str,
    space_id: &str,
    expected_audience: &str,
    capability_needed: Cap,
    op: &str,
    db: &DbConnection,
) -> Result<ValidatedUcan, Response> {
    let max_chain_depth = with_connection(db, |conn| Ok(read_max_ucan_chain_depth(conn)))
        .unwrap_or(MAX_UCAN_CHAIN_DEPTH_DEFAULT) as usize;

    validate_token(
        ucan_token,
        space_id,
        expected_audience,
        capability_needed,
        max_chain_depth,
    )
    .map_err(|e| {
        eprintln!("[SpaceDelivery] {op}: UCAN validation failed: {e}");
        Response::Error {
            message: format!("UCAN validation failed: {e}"),
        }
    })
}

/// Check that a validated UCAN grants the required capability for `space_id`,
/// that the UCAN's `aud` matches the **announced peer DID** (replay-protection),
/// and that the audience is still an active member of the space.
///
/// Three concentric gates:
///
/// 1. **Audience match (`require_audience`)** — the UCAN must have been issued
///    *to the peer presenting it*. Without this check, a peer P who obtained
///    another member's UCAN (e.g. by snooping or replay) could present it
///    over its own authenticated QUIC channel; the capability+membership
///    checks below would both pass.
/// 2. **Capability (`require_capability`)** — the UCAN grants at least the
///    operation's minimum capability for `space_id`.
/// 3. **Active membership (`is_active_space_member`)** — revocation
///    kill-switch: when the admin tombstones a member, the UCAN remains
///    cryptographically valid but every request is rejected here.
///
/// Returns an Error response on any failure.
///
/// **Post-T6 usage.** After the unified `auth_gate` was wired in (T5) and
/// the per-arm redundant checks were removed (T6), this helper has exactly
/// one caller: the `Announce` arm. Announce is the gate's bypass —
/// `auth_gate` returns `Ok(None)` for it because Announce is what
/// *populates* the cached `ValidatedUcan` the gate reads on subsequent
/// requests. So the same three concentric checks still need to run, just
/// **here**, before the UCAN is cached. Every other request variant gets
/// these checks from the gate.
pub(super) fn require_ucan_capability(
    validated: &ValidatedUcan,
    space_id: &str,
    required: Cap,
    peer_did: &str,
    op: &str,
    db: &crate::database::DbConnection,
) -> Result<(), Response> {
    require_audience(validated, peer_did).map_err(|e| {
        eprintln!("[SpaceDelivery] {op}: audience mismatch: {e}");
        Response::Error {
            message: format!("Access denied: {e}"),
        }
    })?;

    require_capability(validated, space_id, required).map_err(|e| {
        eprintln!("[SpaceDelivery] {op}: capability check failed: {e}");
        Response::Error {
            message: format!("Access denied: {e}"),
        }
    })?;

    match super::super::ucan::is_active_space_member(db, space_id, &validated.audience) {
        Ok(true) => Ok(()),
        Ok(false) => {
            eprintln!(
                "[SpaceDelivery] {op}: audience {} is not an active member of space {}",
                validated.audience, space_id
            );
            Err(Response::Error {
                message: "Access denied: not an active member of this space".to_string(),
            })
        }
        Err(e) => {
            eprintln!("[SpaceDelivery] {op}: membership check failed: {e}");
            Err(Response::Error {
                message: format!("Membership check failed: {e}"),
            })
        }
    }
}

// `check_space_membership` and `check_write_capability` have been removed.
// They authorised peers by the DID they announced and a lookup against
// `haex_ucan_tokens` — trusting an unauthenticated self-declaration.
// Capability enforcement now happens at the unified `auth_gate` for every
// non-bypass request (see `super::auth_gate::authorize_request`); the
// `require_valid_ucan` + `require_ucan_capability` helpers above are kept
// only for the `Announce` bypass, which must validate and cache the UCAN
// the gate later reads.
