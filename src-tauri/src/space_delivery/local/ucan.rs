//! UCAN helpers specific to space delivery (DB lookups).
//!
//! Token creation and verification are in the shared `crate::ucan` module.

use crate::database::DbConnection;
use crate::space_delivery::local::error::DeliveryError;

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

    let ucan_rows = crate::database::core::select_with_crdt(ucan_sql, ucan_params, db)
        .map_err(|e| DeliveryError::Database {
            reason: format!("Failed to query UCAN tokens: {}", e),
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

    let identity_rows =
        crate::database::core::select_with_crdt(identity_sql, identity_params, db).map_err(
            |e| DeliveryError::Database {
                reason: format!("Failed to query identities: {}", e),
            },
        )?;

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

/// Load the most recently issued, non-expired UCAN token for `(space_id,
/// audience_did)`. Returns `Ok(None)` if the vault has no token — callers
/// should treat that as "not a member, cannot sync".
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

    let sql = "SELECT token FROM haex_ucan_tokens \
               WHERE space_id = ?1 AND audience_did = ?2 AND expires_at > ?3 \
               ORDER BY issued_at DESC LIMIT 1"
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
        .first()
        .and_then(|row| row.first())
        .and_then(|v| v.as_str())
        .map(|s| s.to_string()))
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

/// Returns `true` if the local UCAN for this `(space_id, audience_did)` grants
/// write-level capability (`space/write` or `space/admin`). Returns `false` for
/// `space/read` members, or if no token is found.
///
/// Used by the push phase to decide whether to include `haex_peer_shares` in
/// the outgoing batch. Read-only members must never attempt to push that table:
/// the leader rejects the entire batch when it sees any non-membership-system
/// row, which leaves the push cursor at t=0 and blocks membership-data uploads.
pub fn has_write_capability(
    db: &DbConnection,
    space_id: &str,
    audience_did: &str,
) -> bool {
    let sql = "SELECT capability FROM haex_ucan_tokens \
               WHERE space_id = ?1 AND audience_did = ?2 \
               ORDER BY issued_at DESC LIMIT 1"
        .to_string();
    let params = vec![
        serde_json::Value::String(space_id.to_string()),
        serde_json::Value::String(audience_did.to_string()),
    ];
    crate::database::core::select_with_crdt(sql, params, db)
        .ok()
        .and_then(|rows| rows.into_iter().next())
        .and_then(|row| row.into_iter().next())
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .map(|cap| cap != "space/read")
        .unwrap_or(false)
}
