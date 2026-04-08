//! UCAN token verification.
//!
//! Validates incoming UCAN tokens (EdDSA / JWT format) that are compatible with
//! the TypeScript `@haex-space/ucan` library.

use base64::Engine;
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

/// Base64url (RFC 4648 §5) without padding — same encoding as @haex-space/ucan.
const BASE64URL: base64::engine::GeneralPurpose = base64::engine::GeneralPurpose::new(
    &base64::alphabet::URL_SAFE,
    base64::engine::general_purpose::NO_PAD,
);

/// Ed25519 multicodec prefix used in did:key
const ED25519_MULTICODEC: [u8; 2] = [0xed, 0x01];

// ---------------------------------------------------------------------------
// Capability levels
// ---------------------------------------------------------------------------

/// Capability levels in ascending order of privilege.
/// Matches the hierarchy in @haex-space/ucan: read < write < invite < admin.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum CapabilityLevel {
    Read = 1,
    Write = 2,
    Invite = 3,
    Admin = 4,
}

impl CapabilityLevel {
    pub fn from_capability_string(capability: &str) -> Option<Self> {
        match capability {
            "space/read" => Some(Self::Read),
            "space/write" => Some(Self::Write),
            "space/invite" => Some(Self::Invite),
            "space/admin" => Some(Self::Admin),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------
// Validated result
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct ValidatedUcan {
    pub issuer: String,
    pub audience: String,
    /// space_id → capability level
    pub capabilities: HashMap<String, CapabilityLevel>,
    pub expires_at: u64,
}

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

#[derive(Debug, thiserror::Error)]
pub enum UcanVerifyError {
    #[error("Malformed token: {0}")]
    MalformedToken(String),
    #[error("Invalid signature")]
    InvalidSignature,
    #[error("Token expired")]
    Expired,
    #[error("Missing capability for space {space_id}")]
    MissingCapability { space_id: String },
    #[error("Insufficient capability: need {required:?}, have {actual:?}")]
    InsufficientCapability {
        required: CapabilityLevel,
        actual: CapabilityLevel,
    },
    #[error("Unknown capability: {0}")]
    UnknownCapability(String),
}

// ---------------------------------------------------------------------------
// Layer 1: validate token structure + signature + expiry
// ---------------------------------------------------------------------------

/// Validate a UCAN token's structure, Ed25519 signature, and expiry.
///
/// This is the **first line of defense** — call before any business logic.
pub fn validate_token(token: &str) -> Result<ValidatedUcan, UcanVerifyError> {
    let parts: Vec<&str> = token.split('.').collect();
    if parts.len() != 3 {
        return Err(UcanVerifyError::MalformedToken(
            "expected 3 dot-separated parts".into(),
        ));
    }

    // Decode payload
    let payload_bytes = BASE64URL
        .decode(parts[1])
        .map_err(|e| UcanVerifyError::MalformedToken(format!("payload base64: {e}")))?;
    let payload: serde_json::Value = serde_json::from_slice(&payload_bytes)
        .map_err(|e| UcanVerifyError::MalformedToken(format!("payload JSON: {e}")))?;

    // Extract issuer DID → Ed25519 public key
    let issuer = payload["iss"]
        .as_str()
        .ok_or_else(|| UcanVerifyError::MalformedToken("missing iss".into()))?;
    let verifying_key = public_key_from_did(issuer)?;

    // Verify signature over "header.payload"
    let signing_input = format!("{}.{}", parts[0], parts[1]);
    let sig_bytes = BASE64URL
        .decode(parts[2])
        .map_err(|e| UcanVerifyError::MalformedToken(format!("signature base64: {e}")))?;
    let sig_array: [u8; 64] = sig_bytes
        .try_into()
        .map_err(|_| UcanVerifyError::MalformedToken("signature must be 64 bytes".into()))?;
    verifying_key
        .verify(signing_input.as_bytes(), &Signature::from_bytes(&sig_array))
        .map_err(|_| UcanVerifyError::InvalidSignature)?;

    // Check expiry
    let exp = payload["exp"]
        .as_u64()
        .ok_or_else(|| UcanVerifyError::MalformedToken("missing exp".into()))?;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    if now >= exp {
        return Err(UcanVerifyError::Expired);
    }

    // Parse capabilities: { "space:<id>": "space/write", ... }
    let audience = payload["aud"].as_str().unwrap_or_default().to_string();
    let cap_obj = payload["cap"]
        .as_object()
        .ok_or_else(|| UcanVerifyError::MalformedToken("missing cap object".into()))?;

    let mut capabilities = HashMap::new();
    for (resource, capability_value) in cap_obj {
        if let Some(space_id) = resource.strip_prefix("space:") {
            let cap_str = capability_value
                .as_str()
                .ok_or_else(|| UcanVerifyError::MalformedToken("capability must be string".into()))?;
            let level = CapabilityLevel::from_capability_string(cap_str)
                .ok_or_else(|| UcanVerifyError::UnknownCapability(cap_str.into()))?;
            capabilities.insert(space_id.to_string(), level);
        }
    }

    Ok(ValidatedUcan {
        issuer: issuer.to_string(),
        audience,
        capabilities,
        expires_at: exp,
    })
}

// ---------------------------------------------------------------------------
// Layer 2: check capability matches operation
// ---------------------------------------------------------------------------

/// Check that a validated UCAN grants at least the required capability for a space.
///
/// This is the **source of truth** — call within each handler after layer-1 passed.
pub fn require_capability(
    validated: &ValidatedUcan,
    space_id: &str,
    required: CapabilityLevel,
) -> Result<(), UcanVerifyError> {
    let actual = validated
        .capabilities
        .get(space_id)
        .ok_or_else(|| UcanVerifyError::MissingCapability {
            space_id: space_id.to_string(),
        })?;

    if *actual >= required {
        Ok(())
    } else {
        Err(UcanVerifyError::InsufficientCapability {
            required,
            actual: *actual,
        })
    }
}

// ---------------------------------------------------------------------------
// did:key → Ed25519 public key
// ---------------------------------------------------------------------------

/// Extract an Ed25519 `VerifyingKey` from a `did:key:z6Mk...` DID.
///
/// Format: `did:key:z` + base58btc( 0xed01 + 32-byte-pubkey )
fn public_key_from_did(did: &str) -> Result<VerifyingKey, UcanVerifyError> {
    let multibase_key = did
        .strip_prefix("did:key:")
        .ok_or_else(|| UcanVerifyError::MalformedToken("DID must start with did:key:".into()))?;

    let base58_str = multibase_key
        .strip_prefix('z')
        .ok_or_else(|| UcanVerifyError::MalformedToken("expected z (base58btc) prefix".into()))?;

    let decoded = bs58::decode(base58_str)
        .into_vec()
        .map_err(|e| UcanVerifyError::MalformedToken(format!("base58 decode: {e}")))?;

    if decoded.len() < 2 || decoded[0..2] != ED25519_MULTICODEC {
        return Err(UcanVerifyError::MalformedToken(
            "missing Ed25519 multicodec prefix 0xed01".into(),
        ));
    }

    let key_bytes: [u8; 32] = decoded[2..]
        .try_into()
        .map_err(|_| UcanVerifyError::MalformedToken("Ed25519 key must be 32 bytes".into()))?;

    VerifyingKey::from_bytes(&key_bytes)
        .map_err(|e| UcanVerifyError::MalformedToken(format!("invalid Ed25519 key: {e}")))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::SigningKey;

    fn did_from_verifying_key(verifying_key: &VerifyingKey) -> String {
        let mut bytes = Vec::with_capacity(34);
        bytes.extend_from_slice(&ED25519_MULTICODEC);
        bytes.extend_from_slice(verifying_key.as_bytes());
        format!("did:key:z{}", bs58::encode(bytes).into_string())
    }

    fn make_test_token(
        signing_key: &SigningKey,
        space_id: &str,
        capability: &str,
        expires_in: u64,
    ) -> String {
        use ed25519_dalek::Signer;

        let issuer_did = did_from_verifying_key(&signing_key.verifying_key());
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let header = serde_json::json!({"alg": "EdDSA", "typ": "JWT"});
        let payload = serde_json::json!({
            "ucv": "1.0",
            "iss": issuer_did,
            "aud": "did:key:z6MkAudience",
            "cap": { format!("space:{}", space_id): capability },
            "exp": now + expires_in,
            "iat": now,
            "prf": [],
            "nnc": "test-nonce"
        });

        let header_b64 = BASE64URL.encode(serde_json::to_string(&header).unwrap().as_bytes());
        let payload_b64 = BASE64URL.encode(serde_json::to_string(&payload).unwrap().as_bytes());
        let signing_input = format!("{}.{}", header_b64, payload_b64);
        let signature = signing_key.sign(signing_input.as_bytes());
        format!(
            "{}.{}.{}",
            header_b64,
            payload_b64,
            BASE64URL.encode(signature.to_bytes())
        )
    }

    fn random_signing_key() -> SigningKey {
        let mut seed = [0u8; 32];
        rand::fill(&mut seed);
        SigningKey::from_bytes(&seed)
    }

    #[test]
    fn valid_read_token() {
        let key = random_signing_key();
        let token = make_test_token(&key, "space-123", "space/read", 3600);
        let validated = validate_token(&token).unwrap();
        assert_eq!(validated.capabilities.get("space-123"), Some(&CapabilityLevel::Read));
    }

    #[test]
    fn valid_write_token() {
        let key = random_signing_key();
        let token = make_test_token(&key, "space-123", "space/write", 3600);
        let validated = validate_token(&token).unwrap();
        assert_eq!(validated.capabilities.get("space-123"), Some(&CapabilityLevel::Write));
    }

    #[test]
    fn expired_token_rejected() {
        let key = random_signing_key();
        let token = make_test_token(&key, "s", "space/read", 0);
        assert!(matches!(validate_token(&token), Err(UcanVerifyError::Expired)));
    }

    #[test]
    fn tampered_signature_rejected() {
        let key = random_signing_key();
        let mut token = make_test_token(&key, "s", "space/read", 3600);
        // Flip last char
        let last = token.pop().unwrap();
        token.push(if last == 'A' { 'B' } else { 'A' });
        assert!(matches!(
            validate_token(&token),
            Err(UcanVerifyError::InvalidSignature | UcanVerifyError::MalformedToken(_))
        ));
    }

    #[test]
    fn require_write_with_only_read_fails() {
        let key = random_signing_key();
        let token = make_test_token(&key, "space-123", "space/read", 3600);
        let validated = validate_token(&token).unwrap();
        assert!(matches!(
            require_capability(&validated, "space-123", CapabilityLevel::Write),
            Err(UcanVerifyError::InsufficientCapability { .. })
        ));
    }

    #[test]
    fn require_read_with_write_succeeds() {
        let key = random_signing_key();
        let token = make_test_token(&key, "space-123", "space/write", 3600);
        let validated = validate_token(&token).unwrap();
        assert!(require_capability(&validated, "space-123", CapabilityLevel::Read).is_ok());
    }

    #[test]
    fn wrong_space_fails() {
        let key = random_signing_key();
        let token = make_test_token(&key, "space-123", "space/admin", 3600);
        let validated = validate_token(&token).unwrap();
        assert!(matches!(
            require_capability(&validated, "other-space", CapabilityLevel::Read),
            Err(UcanVerifyError::MissingCapability { .. })
        ));
    }

    #[test]
    fn did_roundtrip() {
        let key = random_signing_key();
        let did = did_from_verifying_key(&key.verifying_key());
        let recovered = public_key_from_did(&did).unwrap();
        assert_eq!(recovered.as_bytes(), key.verifying_key().as_bytes());
    }

    #[test]
    fn capability_ordering() {
        assert!(CapabilityLevel::Admin > CapabilityLevel::Invite);
        assert!(CapabilityLevel::Invite > CapabilityLevel::Write);
        assert!(CapabilityLevel::Write > CapabilityLevel::Read);
    }
}
