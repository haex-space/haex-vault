//! UCAN token creation.
//!
//! Produces tokens compatible with the TypeScript `@haex-space/ucan` library:
//! `base64url(header).base64url(payload).base64url(ed25519_signature)`

use crate::ucan::row_capability::RowCapability;
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use ed25519_dalek::{Signer, SigningKey};
use serde::Serialize;
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

/// Base64url (RFC 4648 §5) without padding — same encoding as @haex-space/ucan.
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
///
/// `row_cap` is skipped when empty so tokens without row-level capabilities
/// remain byte-identical to pre-C.5 tokens on the wire — a puller that does
/// not consume the field sees the exact same JSON it saw before.
#[derive(Serialize)]
struct UcanPayload {
    ucv: &'static str,
    iss: String,
    aud: String,
    cap: HashMap<String, String>,
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    row_cap: HashMap<String, Vec<RowCapability>>,
    exp: u64,
    iat: u64,
    prf: Vec<String>,
    nnc: String,
}

/// Offset of the raw 32-byte Ed25519 seed inside a PKCS8 DER envelope.
///
/// WebCrypto exports Ed25519 private keys as PKCS8 DER with this structure:
///   - 16 bytes of ASN.1 wrapper
///   - 32 bytes of the actual Ed25519 seed
///   Total: 48 bytes.
const PKCS8_ED25519_SEED_OFFSET: usize = 16;
const PKCS8_ED25519_TOTAL_LEN: usize = 48;

// ---------------------------------------------------------------------------
// Error
// ---------------------------------------------------------------------------

#[derive(Debug, thiserror::Error)]
pub enum UcanCreateError {
    #[error("Invalid private key: {0}")]
    InvalidKey(String),
    #[error("Serialization error: {0}")]
    Serialization(String),
    #[error("Clock error: {0}")]
    ClockError(String),
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Create a delegated UCAN token signed with Ed25519.
///
/// The token format is `base64url(header).base64url(payload).base64url(signature)`
/// and is byte-compatible with tokens produced by `@haex-space/ucan` in TypeScript.
///
/// `row_capabilities` is the optional C.5 envelope: a `space_id →
/// Vec<RowCapability>` map that lands in the payload's `row_cap` claim. Pass
/// `None` (or an empty map) for tokens that only grant space-level
/// capabilities — the `row_cap` key is omitted from the payload in that case
/// so pre-C.5 puller tokens remain byte-identical on the wire.
///
/// Keys in `row_capabilities` are **bare** `space_id` strings (no `space:`
/// prefix). This function prefixes them at emit time, symmetric to how
/// `parse_ucan` strips the prefix on ingest.
#[allow(clippy::too_many_arguments)]
pub fn create_delegated_ucan(
    issuer_did: &str,
    issuer_private_key_base64: &str,
    audience_did: &str,
    space_id: &str,
    capability: &str,
    row_capabilities: Option<HashMap<String, Vec<RowCapability>>>,
    parent_ucan: Option<&str>,
    expires_in_seconds: u64,
) -> Result<String, UcanCreateError> {
    let signing_key = signing_key_from_pkcs8_base64(issuer_private_key_base64)?;

    let now = unix_now()?;
    let nonce = generate_nonce();

    let mut cap = HashMap::new();
    cap.insert(format!("space:{}", space_id), capability.to_string());

    let row_cap = row_capabilities
        .map(|map| {
            map.into_iter()
                .map(|(id, caps)| (format!("space:{}", id), caps))
                .collect::<HashMap<_, _>>()
        })
        .unwrap_or_default();

    let prf = match parent_ucan {
        Some(token) => vec![token.to_string()],
        None => vec![],
    };

    let payload = UcanPayload {
        ucv: "1.0",
        iss: issuer_did.to_string(),
        aud: audience_did.to_string(),
        cap,
        row_cap,
        exp: now + expires_in_seconds,
        iat: now,
        prf,
        nnc: nonce,
    };

    let header_b64 = encode_json_base64url(&HEADER)?;
    let payload_b64 = encode_json_base64url(&payload)?;

    let signing_input = format!("{}.{}", header_b64, payload_b64);
    let signature = signing_key.sign(signing_input.as_bytes());
    let signature_b64 = BASE64URL.encode(signature.to_bytes());

    Ok(format!("{}.{}.{}", header_b64, payload_b64, signature_b64))
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Extract an Ed25519 `SigningKey` from a Base64-encoded PKCS8 DER blob.
pub fn signing_key_from_pkcs8_base64(base64_key: &str) -> Result<SigningKey, UcanCreateError> {
    let der = BASE64
        .decode(base64_key)
        .map_err(|e| UcanCreateError::InvalidKey(format!("Invalid Base64: {}", e)))?;

    if der.len() != PKCS8_ED25519_TOTAL_LEN {
        return Err(UcanCreateError::InvalidKey(format!(
            "Unexpected PKCS8 DER length: expected {}, got {}",
            PKCS8_ED25519_TOTAL_LEN,
            der.len()
        )));
    }

    let seed: [u8; 32] = der[PKCS8_ED25519_SEED_OFFSET..PKCS8_ED25519_TOTAL_LEN]
        .try_into()
        .map_err(|_| UcanCreateError::InvalidKey("Failed to extract Ed25519 seed".into()))?;

    Ok(SigningKey::from_bytes(&seed))
}

fn encode_json_base64url<T: Serialize>(value: &T) -> Result<String, UcanCreateError> {
    let json =
        serde_json::to_string(value).map_err(|e| UcanCreateError::Serialization(e.to_string()))?;
    Ok(BASE64URL.encode(json.as_bytes()))
}

fn generate_nonce() -> String {
    let mut bytes = [0u8; 12];
    rand::fill(&mut bytes);
    BASE64URL.encode(bytes)
}

fn unix_secs_from(t: SystemTime) -> Result<u64, UcanCreateError> {
    t.duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .map_err(|e| UcanCreateError::ClockError(format!("system clock before UNIX epoch: {e}")))
}

fn unix_now() -> Result<u64, UcanCreateError> {
    unix_secs_from(SystemTime::now())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn test_pkcs8_key() -> (SigningKey, String) {
        let mut seed = [0u8; 32];
        rand::fill(&mut seed);
        let signing_key = SigningKey::from_bytes(&seed);

        let pkcs8_prefix: [u8; 16] = [
            0x30, 0x2e, 0x02, 0x01, 0x00, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x04, 0x22,
            0x04, 0x20,
        ];
        let mut pkcs8 = Vec::with_capacity(48);
        pkcs8.extend_from_slice(&pkcs8_prefix);
        pkcs8.extend_from_slice(&signing_key.to_bytes());

        let b64 = BASE64.encode(&pkcs8);
        (signing_key, b64)
    }

    #[test]
    fn token_has_three_parts() {
        let (_key, b64) = test_pkcs8_key();
        let token = create_delegated_ucan(
            "did:key:z6MkIssuer",
            &b64,
            "did:key:z6MkAudience",
            "test-space",
            "space/write",
            None,
            Some("parent.token"),
            3600,
        )
        .unwrap();

        assert_eq!(token.split('.').count(), 3);
    }

    #[test]
    fn token_payload_fields() {
        let (_key, b64) = test_pkcs8_key();
        let token = create_delegated_ucan(
            "did:key:z6MkIssuer",
            &b64,
            "did:key:z6MkAudience",
            "test-space",
            "space/write",
            None,
            None,
            3600,
        )
        .unwrap();

        let parts: Vec<&str> = token.split('.').collect();
        let payload: serde_json::Value =
            serde_json::from_slice(&BASE64URL.decode(parts[1]).unwrap()).unwrap();

        assert_eq!(payload["iss"], "did:key:z6MkIssuer");
        assert_eq!(payload["aud"], "did:key:z6MkAudience");
        assert_eq!(payload["cap"]["space:test-space"], "space/write");
        assert_eq!(payload["ucv"], "1.0");
        assert!(payload["exp"].as_u64().unwrap() > payload["iat"].as_u64().unwrap());
    }

    #[test]
    fn rejects_wrong_key_length() {
        let short = BASE64.encode(&[0u8; 32]);
        assert!(signing_key_from_pkcs8_base64(&short).is_err());
    }

    #[test]
    fn nonce_uniqueness() {
        let n1 = generate_nonce();
        let n2 = generate_nonce();
        assert_ne!(n1, n2);
        assert_eq!(n1.len(), 16); // 12 bytes → 16 base64url chars
    }

    #[test]
    fn unix_secs_from_now_succeeds() {
        assert!(unix_secs_from(SystemTime::now()).is_ok());
    }

    // ------------------------------------------------------------------
    // C.5: row_capabilities envelope
    // ------------------------------------------------------------------

    #[test]
    fn token_carries_row_cap_when_supplied() {
        use crate::ucan::predicate::{Predicate, PrimitiveValue};
        use crate::ucan::row_capability::RowCapability;

        let (_key, b64) = test_pkcs8_key();
        let mut row_caps: HashMap<String, Vec<RowCapability>> = HashMap::new();
        row_caps.insert(
            "test-space".to_string(),
            vec![
                RowCapability::RowInsert {
                    matches: Predicate::Eq {
                        col: "cat".into(),
                        eq: PrimitiveValue::String("work".into()),
                    },
                },
                RowCapability::RowDelete {
                    matches: Predicate::StartsWith {
                        col: "path".into(),
                        starts_with: "/tmp/".into(),
                    },
                },
            ],
        );

        let token = create_delegated_ucan(
            "did:key:z6MkIssuer",
            &b64,
            "did:key:z6MkAudience",
            "test-space",
            "space/write",
            Some(row_caps.clone()),
            None,
            3600,
        )
        .unwrap();

        let parts: Vec<&str> = token.split('.').collect();
        let payload: serde_json::Value =
            serde_json::from_slice(&BASE64URL.decode(parts[1]).unwrap()).unwrap();

        // Wire key is `row_cap`, keyed by `space:<id>` (symmetry with `cap`).
        let row_cap = payload
            .get("row_cap")
            .expect("row_cap must be present on the wire");
        let entry = row_cap
            .get("space:test-space")
            .expect("row_cap must key by space:<id>");
        let arr = entry.as_array().expect("row_cap entry is an array");
        assert_eq!(arr.len(), 2);
        assert_eq!(arr[0]["op"], "row_insert");
        assert_eq!(arr[0]["where"]["col"], "cat");
        assert_eq!(arr[1]["op"], "row_delete");
    }

    #[test]
    fn token_omits_row_cap_when_none_supplied() {
        // Backwards-compat with today's tokens: no row_caps → no `row_cap`
        // key at all (not an empty object). Verifiers that don't consume
        // the field must see a payload identical in shape to pre-C.5.
        let (_key, b64) = test_pkcs8_key();
        let token = create_delegated_ucan(
            "did:key:z6MkIssuer",
            &b64,
            "did:key:z6MkAudience",
            "test-space",
            "space/write",
            None,
            None,
            3600,
        )
        .unwrap();
        let parts: Vec<&str> = token.split('.').collect();
        let payload: serde_json::Value =
            serde_json::from_slice(&BASE64URL.decode(parts[1]).unwrap()).unwrap();
        assert!(
            payload.get("row_cap").is_none(),
            "row_cap key must be absent when caller passes None (got {:?})",
            payload.get("row_cap")
        );
    }

    #[test]
    fn create_then_parse_row_caps_roundtrip() {
        // End-to-end: create emits `row_cap`, parse_ucan reads it into
        // `ParsedUcan.row_capabilities`, values equal what we passed in.
        use crate::ucan::parse_ucan;
        use crate::ucan::predicate::{Predicate, PrimitiveValue};
        use crate::ucan::row_capability::RowCapability;

        let (key, b64) = test_pkcs8_key();
        let issuer_did = {
            let mut bytes = Vec::with_capacity(34);
            bytes.extend_from_slice(&[0xed, 0x01]);
            bytes.extend_from_slice(key.verifying_key().as_bytes());
            format!("did:key:z{}", bs58::encode(bytes).into_string())
        };

        let expected = vec![RowCapability::RowUpdate {
            matches: Predicate::In {
                col: "status".into(),
                values: vec![
                    PrimitiveValue::String("open".into()),
                    PrimitiveValue::String("in_progress".into()),
                ],
            },
        }];
        let mut row_caps = HashMap::new();
        row_caps.insert("s1".to_string(), expected.clone());

        let token = create_delegated_ucan(
            &issuer_did,
            &b64,
            &issuer_did,
            "s1",
            "space/write",
            Some(row_caps),
            None,
            3600,
        )
        .unwrap();

        let parsed = parse_ucan(&token).unwrap();
        assert_eq!(parsed.row_capabilities.get("s1"), Some(&expected));
    }

    #[test]
    fn unix_secs_from_pre_epoch_returns_err() {
        // Previously `unix_now` panicked on times before UNIX_EPOCH via
        // `.expect("system clock before UNIX epoch")`. Verify the
        // refactored function returns a ClockError instead.
        let Some(pre_epoch) = UNIX_EPOCH.checked_sub(std::time::Duration::from_secs(1)) else {
            // Platform can't represent pre-epoch; nothing to assert on.
            return;
        };
        let err = unix_secs_from(pre_epoch).expect_err("pre-epoch must error, not panic");
        assert!(matches!(err, UcanCreateError::ClockError(_)));
    }
}
