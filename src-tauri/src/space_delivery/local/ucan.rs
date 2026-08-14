//! UCAN helpers specific to space delivery (DB lookups).
//!
//! Token creation and verification are in the shared `crate::ucan` module.

use crate::database::DbConnection;
use crate::space_delivery::local::error::DeliveryError;
use crate::ucan::{cap_from_str, Cap};

// Re-export from shared module so existing callers keep working
pub use crate::ucan::create_delegated_ucan;

/// UCAN expiry used for all member tokens we mint in this codebase. The
/// active-membership check in `is_active_space_member` is the real access
/// gate — the `exp` field is kept for UCAN-standard conformance and as a
/// defense-in-depth failsafe. Value is seconds from `now` that resolves to
/// well past any realistic deployment lifetime (~100 years).
pub const MEMBER_UCAN_EXPIRES_IN_SECONDS: u64 = 100 * 365 * 86_400;

/// Admin identity loaded from the database.
pub struct AdminIdentity {
    pub did: String,
    pub private_key_base64: String,
    pub root_ucan: String,
}

/// Load the admin identity for a space from the database.
///
/// Finds the identity that issued the root UCAN (`space/admin` capability) for
/// this space and returns its DID, private key, and the root token string.
pub fn load_admin_identity(
    db: &DbConnection,
    space_id: &str,
) -> Result<AdminIdentity, DeliveryError> {
    // 1. Find the root UCAN token for this space (capability = 'space/admin')
    let ucan_sql = "SELECT issuer_did, token \
                     FROM haex_ucan_tokens \
                     WHERE space_id = ?1 AND capability = 'space/admin' \
                     LIMIT 1"
        .to_string();
    let ucan_params = vec![serde_json::Value::String(space_id.to_string())];

    let ucan_rows =
        crate::database::core::select_with_crdt(ucan_sql, ucan_params, db).map_err(|e| {
            DeliveryError::Database {
                reason: format!("Failed to query UCAN tokens: {}", e),
            }
        })?;

    let ucan_row = ucan_rows.first().ok_or_else(|| DeliveryError::Database {
        reason: format!("No admin UCAN found for space {}", space_id),
    })?;

    let issuer_did = ucan_row
        .first()
        .and_then(|v| v.as_str())
        .ok_or_else(|| DeliveryError::Database {
            reason: "Missing issuer_did in UCAN row".to_string(),
        })?
        .to_string();

    let root_ucan = ucan_row
        .get(1)
        .and_then(|v| v.as_str())
        .ok_or_else(|| DeliveryError::Database {
            reason: "Missing token in UCAN row".to_string(),
        })?
        .to_string();

    // 2. Look up the identity by DID to get the private key
    let identity_sql = "SELECT private_key \
                        FROM haex_identities \
                        WHERE did = ?1 \
                        LIMIT 1"
        .to_string();
    let identity_params = vec![serde_json::Value::String(issuer_did.clone())];

    let identity_rows = crate::database::core::select_with_crdt(identity_sql, identity_params, db)
        .map_err(|e| DeliveryError::Database {
            reason: format!("Failed to query identities: {}", e),
        })?;

    let identity_row = identity_rows
        .first()
        .ok_or_else(|| DeliveryError::Database {
            reason: format!("Identity not found for DID {}", issuer_did),
        })?;

    let private_key_base64 = identity_row
        .first()
        .and_then(|v| v.as_str())
        .ok_or_else(|| DeliveryError::Database {
            reason: "Missing private_key in identity row".to_string(),
        })?
        .to_string();

    Ok(AdminIdentity {
        did: issuer_did,
        private_key_base64,
        root_ucan,
    })
}

/// Load the highest-ranked, non-expired UCAN token held for `(space_id,
/// audience_did)`. Returns `Ok(None)` if the vault has no token — callers
/// should treat that as "not a member, cannot sync".
///
/// A member can hold several independent capability grants at once (e.g.
/// `space/write` and `space/invite` — orthogonal, neither implies the
/// other); this is used where exactly one token must be presented for a
/// whole connection (the Announce handshake caches one `ValidatedUcan` per
/// peer, see `leader::auth`), so it picks whichever held token ranks
/// highest by [`Cap`] discriminant (`Admin > Invite > Write > Read`) —
/// that one carries the most-privileged single capability grant, which
/// under the DB's one-token-per-capability shape is a reasonable proxy for
/// "the token most likely to satisfy an arbitrary downstream check".
/// Under orthogonal caps this is a heuristic, not a proof: post-Task-8b
/// the schema is expected to carry a full [`crate::ucan::CapabilitySet`]
/// per token and the picker can then choose by set-coverage.
/// `ORDER BY issued_at DESC LIMIT 1` used to be the selection here, which
/// broke as soon as a claim could mint more than one token: capabilities
/// issued in the same claim share the same `issued_at` second, so the tie
/// was resolved arbitrarily and could hand back a `space/read` token even
/// when a `space/write` one was also held.
///
/// Resolved fresh on every call: the authoritative source is the DB, not an
/// in-memory cache, so a reconnect after expiry picks up a renewed token
/// without process restart.
pub fn load_active_ucan_for_audience(
    db: &DbConnection,
    space_id: &str,
    audience_did: &str,
) -> Result<Option<String>, DeliveryError> {
    let now_secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);

    let sql = "SELECT capability, token FROM haex_ucan_tokens \
               WHERE space_id = ?1 AND audience_did = ?2 AND expires_at > ?3"
        .to_string();
    let params = vec![
        serde_json::Value::String(space_id.to_string()),
        serde_json::Value::String(audience_did.to_string()),
        serde_json::Value::Number(now_secs.into()),
    ];

    let rows = crate::database::core::select_with_crdt(sql, params, db).map_err(|e| {
        DeliveryError::Database {
            reason: format!("Failed to query UCAN tokens: {}", e),
        }
    })?;

    Ok(rows
        .iter()
        .filter_map(|row| {
            let capability = row.first()?.as_str()?;
            let token = row.get(1)?.as_str()?.to_string();
            let rank = cap_from_str(capability).unwrap_or(Cap::Read);
            Some((rank, token))
        })
        .max_by_key(|(rank, _)| *rank)
        .map(|(_, token)| token))
}

/// Check that `audience_did` is an active (non-tombstoned) member of `space_id`.
///
/// This is the **revocation mechanism**: when an admin removes a member
/// (`db.delete(haex_space_members)` → CRDT tombstone) the member's UCAN
/// remains cryptographically valid but this check rejects every sync
/// request. The MLS commit simultaneously removes the member from the
/// content-encryption epoch, so the two act as a coupled kill-switch.
pub fn is_active_space_member(
    db: &DbConnection,
    space_id: &str,
    audience_did: &str,
) -> Result<bool, DeliveryError> {
    // `select_with_crdt` adds `IFNULL(haex_tombstone, 0) != 1` to every
    // referenced table automatically, so we don't spell out the filter.
    let sql = "SELECT COUNT(*) FROM haex_space_members m \
               JOIN haex_identities i ON m.identity_id = i.id \
               WHERE m.space_id = ?1 AND i.did = ?2"
        .to_string();
    let params = vec![
        serde_json::Value::String(space_id.to_string()),
        serde_json::Value::String(audience_did.to_string()),
    ];

    let rows = crate::database::core::select_with_crdt(sql, params, db).map_err(|e| {
        DeliveryError::Database {
            reason: format!("Failed to check space membership: {}", e),
        }
    })?;

    let count = rows
        .first()
        .and_then(|row| row.first())
        .and_then(|v| v.as_i64())
        .unwrap_or(0);

    Ok(count > 0)
}

/// Resolve a receive-side committer-capability proof carried on an MLS
/// commit's delivery envelope (plan §5.8:
/// `docs/plans/2026-08-13-mls-receive-gate-ucan-on-commit.md`).
///
/// `committer_ucan` is the raw JWT string as carried on the wire
/// (`MlsMessageEntry::committer_ucan`) — not base64-encoded; a UCAN's own
/// dot-separated parts are already base64url and safe as JSON text.
///
/// Runs the full [`validate_token`] pipeline (signature, expiry, `prf`-chain
/// walk to the space's self-signed root, self-certifying `space_id`
/// binding) with `expected_audience` peeked from the token's own `aud` via
/// [`parse_ucan`] — the real audience check (this token's `aud` must equal
/// the commit's MLS-authenticated committer DID) happens downstream in
/// `mls::authorization::authorize_committer_capability`, which is the only
/// place that knows the committer DID. This function's job is purely
/// "is this a validly-chained, non-expired UCAN for this space" — it
/// deliberately owns the entire UCAN-verify blast radius so
/// `mls::authorization` never has to parse a raw token (plan §9).
///
/// Returns `None` if no UCAN was attached, or if the attached token fails
/// to verify for any reason (bad signature, expired, broken chain, wrong
/// space, malformed). The caller (the MLS receive-gate) decides whether the
/// commit actually needed a proof — `None` there correctly falls through to
/// "no proof presented", which is rejected only if one was required.
pub fn resolve_presented_committer_capability(
    db: &DbConnection,
    space_id: &str,
    committer_ucan: Option<&str>,
) -> Option<crate::mls::authorization::PresentedCapability> {
    let token = committer_ucan?;
    // Log each failure branch with a distinct reason so operators can
    // tell "no proof presented" (silent `None` above) apart from "a
    // proof was presented and it failed verification" (any branch below).
    // Token contents are not logged — the reason label alone is enough
    // to triage the gate's decision without exposing the JWT.
    let peeked = match crate::ucan::parse_ucan(token) {
        Ok(p) => p,
        Err(e) => {
            eprintln!(
                "[mls::committer-capability] rejecting presented UCAN for space {space_id}: \
                 parse failed ({e})"
            );
            return None;
        }
    };
    let max_chain_depth = crate::database::core::with_connection(db, |conn| {
        Ok(crate::ucan::read_max_ucan_chain_depth(conn))
    })
    .unwrap_or(crate::ucan::MAX_UCAN_CHAIN_DEPTH_DEFAULT) as usize;

    let validated =
        match crate::ucan::validate_token(token, space_id, &peeked.aud, Cap::Read, max_chain_depth)
        {
            Ok(v) => v,
            Err(e) => {
                eprintln!(
                    "[mls::committer-capability] rejecting presented UCAN for space {space_id}: \
                 validate_token failed ({e})"
                );
                return None;
            }
        };
    let capabilities = match validated.capabilities.get(space_id) {
        Some(set) => set.clone(),
        None => {
            eprintln!(
                "[mls::committer-capability] rejecting presented UCAN for space {space_id}: \
                 token validated but carries no capability for this space"
            );
            return None;
        }
    };
    Some(crate::mls::authorization::PresentedCapability {
        audience_did: validated.audience,
        capabilities,
    })
}

/// Returns `true` if `(space_id, audience_did)` holds **any** UCAN granting
/// write-level capability (`space/write` or `space/admin`) — a member can
/// hold several independent, orthogonal tokens at once (e.g. `space/read`
/// and `space/write` both, from one invite), so this checks every held
/// token rather than inspecting a single arbitrarily-picked row. Returns
/// `false` if no such token is found among any held.
///
/// Used by the push phase to decide whether to include `haex_peer_shares` in
/// the outgoing batch. Read-only members must never attempt to push that table:
/// the leader rejects the entire batch when it sees any non-membership-system
/// row, which leaves the push cursor at t=0 and blocks membership-data uploads.
pub fn has_write_capability(db: &DbConnection, space_id: &str, audience_did: &str) -> bool {
    let now_secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let sql = "SELECT 1 FROM haex_ucan_tokens \
               WHERE space_id = ?1 AND audience_did = ?2 \
               AND capability IN ('space/write', 'space/admin') AND expires_at > ?3 LIMIT 1"
        .to_string();
    let params = vec![
        serde_json::Value::String(space_id.to_string()),
        serde_json::Value::String(audience_did.to_string()),
        serde_json::Value::Number(now_secs.into()),
    ];
    crate::database::core::select_with_crdt(sql, params, db)
        .map(|rows| !rows.is_empty())
        .unwrap_or(false)
}

#[cfg(test)]
mod multi_capability_lookup_tests {
    //! Regression coverage for the second bug found via the haex-e2e-tests
    //! write-capability spec: a member holding two independently-issued
    //! UCANs from the same claim (e.g. `space/read` + `space/write`, both
    //! `issued_at` the same second) got `has_write_capability` / an
    //! Announce token resolved via `ORDER BY issued_at DESC LIMIT 1` —
    //! ties resolved arbitrarily, sometimes handing back the `space/read`
    //! row and silently treating a write-capable member as read-only.

    use super::{has_write_capability, load_active_ucan_for_audience};
    use crate::database::DbConnection;
    use rusqlite::Connection;
    use std::sync::{Arc, Mutex};

    const SPACE_ID: &str = "space-1";
    const AUDIENCE_DID: &str = "did:key:member";

    fn seed_db(rows: &[(&str, &str, i64)]) -> DbConnection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE haex_ucan_tokens (
                id TEXT PRIMARY KEY,
                space_id TEXT NOT NULL,
                token TEXT NOT NULL,
                capability TEXT NOT NULL,
                issuer_did TEXT NOT NULL,
                audience_did TEXT NOT NULL,
                issued_at INTEGER NOT NULL,
                expires_at INTEGER NOT NULL
            );",
        )
        .unwrap();
        for (id, capability, issued_at) in rows {
            conn.execute(
                "INSERT INTO haex_ucan_tokens \
                 (id, space_id, token, capability, issuer_did, audience_did, issued_at, expires_at) \
                 VALUES (?1, ?2, ?3, ?4, 'did:key:admin', ?5, ?6, 9999999999)",
                rusqlite::params![id, SPACE_ID, format!("token-{id}"), capability, AUDIENCE_DID, issued_at],
            )
            .unwrap();
        }
        DbConnection(Arc::new(Mutex::new(Some(conn))))
    }

    #[test]
    fn has_write_capability_finds_write_row_even_when_read_row_ties_on_issued_at() {
        // Same issued_at second — reproduces the exact tie from a single
        // claim issuing multiple UCANs together.
        let db = seed_db(&[("read", "space/read", 1000), ("write", "space/write", 1000)]);
        assert!(has_write_capability(&db, SPACE_ID, AUDIENCE_DID));
    }

    #[test]
    fn has_write_capability_false_when_only_read_is_held() {
        let db = seed_db(&[("read", "space/read", 1000)]);
        assert!(!has_write_capability(&db, SPACE_ID, AUDIENCE_DID));
    }

    #[test]
    fn load_active_ucan_prefers_write_token_over_tied_read_token() {
        let db = seed_db(&[("read", "space/read", 1000), ("write", "space/write", 1000)]);
        let token = load_active_ucan_for_audience(&db, SPACE_ID, AUDIENCE_DID)
            .unwrap()
            .unwrap();
        assert_eq!(token, "token-write");
    }

    #[test]
    fn load_active_ucan_returns_none_when_no_token_held() {
        let db = seed_db(&[]);
        assert!(load_active_ucan_for_audience(&db, SPACE_ID, AUDIENCE_DID)
            .unwrap()
            .is_none());
    }

    /// Regression for the CodeRabbit finding that `has_write_capability`'s
    /// query had no `expires_at` filter, unlike its sibling
    /// `load_active_ucan_for_audience` just above it — an expired
    /// `space/write` row would still make this return `true`.
    #[test]
    fn has_write_capability_ignores_expired_write_row() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE haex_ucan_tokens (
                id TEXT PRIMARY KEY,
                space_id TEXT NOT NULL,
                token TEXT NOT NULL,
                capability TEXT NOT NULL,
                issuer_did TEXT NOT NULL,
                audience_did TEXT NOT NULL,
                issued_at INTEGER NOT NULL,
                expires_at INTEGER NOT NULL
            );",
        )
        .unwrap();
        conn.execute(
            "INSERT INTO haex_ucan_tokens \
             (id, space_id, token, capability, issuer_did, audience_did, issued_at, expires_at) \
             VALUES ('write', ?1, 'token-write', 'space/write', 'did:key:admin', ?2, 1000, 1001)",
            rusqlite::params![SPACE_ID, AUDIENCE_DID],
        )
        .unwrap();
        let db = DbConnection(Arc::new(Mutex::new(Some(conn))));

        assert!(!has_write_capability(&db, SPACE_ID, AUDIENCE_DID));
    }
}
