//! UCAN token creation in Rust for local space invites.
//!
//! Produces tokens compatible with the TypeScript `@haex-space/ucan` library:
//! `base64url(header).base64url(payload).base64url(ed25519_signature)`

use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use ed25519_dalek::{Signer, SigningKey};
use serde::Serialize;
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::database::DbConnection;
use crate::space_delivery::local::error::DeliveryError;

/// Base64url alphabet (RFC 4648 §5) without padding.
const BASE64URL: base64::engine::GeneralPurpose = base64::engine::GeneralPurpose::new(
    &base64::alphabet::URL_SAFE,
    base64::engine::general_purpose::NO_PAD,
);

/// UCAN header — always fixed for EdDSA / JWT.
#[derive(Serialize)]
struct UcanHeader {
    alg: &'static str,
    typ: &'static str,
}

const HEADER: UcanHeader = UcanHeader {
    alg: "EdDSA",
    typ: "JWT",
};

/// UCAN payload matching the TypeScript `UcanPayload` interface.
#[derive(Serialize)]
struct UcanPayload {
    ucv: &'static str,
    iss: String,
    aud: String,
    cap: HashMap<String, String>,
    exp: u64,
    iat: u64,
    prf: Vec<String>,
    nnc: String,
}

/// Admin identity loaded from the database.
pub struct AdminIdentity {
    pub did: String,
    pub private_key_base64: String,
    pub root_ucan: String,
}

/// Offset of the raw 32-byte Ed25519 seed inside a PKCS8 DER envelope.
///
/// WebCrypto exports Ed25519 private keys as PKCS8 DER with this structure:
///   - 16 bytes of ASN.1 wrapper (SEQUENCE, version, AlgorithmIdentifier, OCTET STRING header)
///   - 32 bytes of the actual Ed25519 seed
///   Total: 48 bytes.
const PKCS8_ED25519_SEED_OFFSET: usize = 16;
const PKCS8_ED25519_TOTAL_LEN: usize = 48;

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Create a delegated UCAN token signed with Ed25519.
///
/// The token format is `base64url(header).base64url(payload).base64url(signature)`
/// and is byte-compatible with tokens produced by `@haex-space/ucan` in TypeScript.
pub fn create_delegated_ucan(
    issuer_did: &str,
    issuer_private_key_base64: &str,
    audience_did: &str,
    space_id: &str,
    capability: &str,
    parent_ucan: Option<&str>,
    expires_in_seconds: u64,
) -> Result<String, DeliveryError> {
    // 1. Decode PKCS8 private key and extract 32-byte Ed25519 seed
    let signing_key = signing_key_from_pkcs8_base64(issuer_private_key_base64)?;

    // 2. Build timestamps
    let now = unix_now();

    // 3. Generate 12-byte random nonce, base64url-encoded
    let nonce = generate_nonce();

    // 4. Build capability map: "space:<spaceId>" → capability
    let mut cap = HashMap::new();
    cap.insert(format!("space:{}", space_id), capability.to_string());

    // 5. Build proofs array
    let prf = match parent_ucan {
        Some(token) => vec![token.to_string()],
        None => vec![],
    };

    // 6. Assemble payload
    let payload = UcanPayload {
        ucv: "1.0",
        iss: issuer_did.to_string(),
        aud: audience_did.to_string(),
        cap,
        exp: now + expires_in_seconds,
        iat: now,
        prf,
        nnc: nonce,
    };

    // 7. Encode header + payload as base64url JSON
    let header_b64 = encode_json_base64url(&HEADER)?;
    let payload_b64 = encode_json_base64url(&payload)?;

    // 8. Sign "header.payload" with Ed25519
    let signing_input = format!("{}.{}", header_b64, payload_b64);
    let signature = signing_key.sign(signing_input.as_bytes());
    let signature_b64 = BASE64URL.encode(signature.to_bytes());

    // 9. Assemble token
    Ok(format!("{}.{}.{}", header_b64, payload_b64, signature_b64))
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

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Extract an Ed25519 `SigningKey` from a Base64-encoded PKCS8 DER blob.
fn signing_key_from_pkcs8_base64(base64_key: &str) -> Result<SigningKey, DeliveryError> {
    let der = BASE64.decode(base64_key).map_err(|e| DeliveryError::ProtocolError {
        reason: format!("Invalid Base64 in private key: {}", e),
    })?;

    if der.len() != PKCS8_ED25519_TOTAL_LEN {
        return Err(DeliveryError::ProtocolError {
            reason: format!(
                "Unexpected PKCS8 DER length: expected {}, got {}",
                PKCS8_ED25519_TOTAL_LEN,
                der.len()
            ),
        });
    }

    let seed: [u8; 32] = der[PKCS8_ED25519_SEED_OFFSET..PKCS8_ED25519_TOTAL_LEN]
        .try_into()
        .map_err(|_| DeliveryError::ProtocolError {
            reason: "Failed to extract 32-byte Ed25519 seed from PKCS8".to_string(),
        })?;

    Ok(SigningKey::from_bytes(&seed))
}

/// Serialize a value to JSON and encode as base64url (no padding).
fn encode_json_base64url<T: Serialize>(value: &T) -> Result<String, DeliveryError> {
    let json = serde_json::to_string(value).map_err(|e| DeliveryError::ProtocolError {
        reason: format!("JSON serialization failed: {}", e),
    })?;
    Ok(BASE64URL.encode(json.as_bytes()))
}

/// Generate a 12-byte random nonce, base64url-encoded.
fn generate_nonce() -> String {
    let mut bytes = [0u8; 12];
    rand::fill(&mut bytes);
    BASE64URL.encode(bytes)
}

/// Current Unix timestamp in seconds.
fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before UNIX epoch")
        .as_secs()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Verify that a round-trip PKCS8 → SigningKey → VerifyingKey works and that
    /// the produced token has three dot-separated parts.
    #[test]
    fn test_create_delegated_ucan_structure() {
        // Generate a fresh keypair to get a known-good PKCS8 blob.
        let mut seed = [0u8; 32];
        rand::fill(&mut seed);
        let signing_key = SigningKey::from_bytes(&seed);
        let seed = signing_key.to_bytes();

        // Build a minimal PKCS8 DER envelope (48 bytes total).
        // ASN.1 structure for Ed25519 PKCS8:
        //   SEQUENCE {
        //     INTEGER 0 (version)
        //     SEQUENCE { OID 1.3.101.112 (Ed25519) }
        //     OCTET STRING { OCTET STRING (32 bytes seed) }
        //   }
        let pkcs8_prefix: [u8; 16] = [
            0x30, 0x2e, // SEQUENCE, length 46
            0x02, 0x01, 0x00, // INTEGER 0
            0x30, 0x05, // SEQUENCE, length 5
            0x06, 0x03, 0x2b, 0x65, 0x70, // OID 1.3.101.112
            0x04, 0x22, // OCTET STRING, length 34
            0x04, 0x20, // OCTET STRING, length 32
        ];
        let mut pkcs8 = Vec::with_capacity(48);
        pkcs8.extend_from_slice(&pkcs8_prefix);
        pkcs8.extend_from_slice(&seed);
        assert_eq!(pkcs8.len(), 48);

        let private_key_b64 = BASE64.encode(&pkcs8);

        let token = create_delegated_ucan(
            "did:key:z6MkIssuer",
            &private_key_b64,
            "did:key:z6MkAudience",
            "test-space-id",
            "space/write",
            Some("parent.ucan.token"),
            3600,
        )
        .expect("token creation should succeed");

        // Token must have 3 dot-separated parts
        let parts: Vec<&str> = token.split('.').collect();
        assert_eq!(parts.len(), 3, "UCAN must have 3 parts");

        // Decode and verify header
        let header_json: serde_json::Value =
            serde_json::from_slice(&BASE64URL.decode(parts[0]).unwrap()).unwrap();
        assert_eq!(header_json["alg"], "EdDSA");
        assert_eq!(header_json["typ"], "JWT");

        // Decode and verify payload fields
        let payload_json: serde_json::Value =
            serde_json::from_slice(&BASE64URL.decode(parts[1]).unwrap()).unwrap();
        assert_eq!(payload_json["ucv"], "1.0");
        assert_eq!(payload_json["iss"], "did:key:z6MkIssuer");
        assert_eq!(payload_json["aud"], "did:key:z6MkAudience");
        assert_eq!(
            payload_json["cap"]["space:test-space-id"], "space/write"
        );
        assert_eq!(payload_json["prf"][0], "parent.ucan.token");
        assert!(payload_json["exp"].as_u64().unwrap() > payload_json["iat"].as_u64().unwrap());
        assert!(!payload_json["nnc"].as_str().unwrap().is_empty());

        // Verify the Ed25519 signature
        let verifying_key = signing_key.verifying_key();
        let signing_input = format!("{}.{}", parts[0], parts[1]);
        let sig_bytes = BASE64URL.decode(parts[2]).unwrap();
        let sig_array: [u8; 64] = sig_bytes.try_into().expect("signature must be 64 bytes");
        let signature = ed25519_dalek::Signature::from_bytes(&sig_array);
        use ed25519_dalek::Verifier;
        verifying_key
            .verify(signing_input.as_bytes(), &signature)
            .expect("signature must be valid");
    }

    #[test]
    fn test_nonce_uniqueness() {
        let n1 = generate_nonce();
        let n2 = generate_nonce();
        assert_ne!(n1, n2, "nonces should be unique");
        // 12 bytes → 16 base64url chars
        assert_eq!(n1.len(), 16);
    }

    #[test]
    fn test_pkcs8_extraction_rejects_wrong_length() {
        let short_key = BASE64.encode(&[0u8; 32]);
        let result = signing_key_from_pkcs8_base64(&short_key);
        assert!(result.is_err());
    }
}
