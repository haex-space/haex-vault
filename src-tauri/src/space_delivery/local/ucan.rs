//! UCAN helpers specific to space delivery (DB lookups).
//!
//! Token creation and verification are in the shared `crate::ucan` module.

use crate::database::DbConnection;
use crate::space_delivery::local::error::DeliveryError;

// Re-export from shared module so existing callers keep working
pub use crate::ucan::create_delegated_ucan;

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
