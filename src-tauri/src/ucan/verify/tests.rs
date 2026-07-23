//! Unit tests for the low-level [`parse_ucan`] pipeline and the cached-UCAN
//! `require_*` helpers.
//!
//! Chain-walk semantics are exercised by the cross-language fixture in
//! [`super::chain_tests`] and by the integration driver at
//! `src-tauri/tests/ucan_chain_vectors.rs`.

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

// ---------------------------------------------------------------------------
// parse_ucan — structure + signature + expiry
// ---------------------------------------------------------------------------

#[test]
fn parse_ucan_extracts_read_capability() {
    let key = random_signing_key();
    let token = make_test_token(&key, "space-123", "space/read", 3600);
    let parsed = parse_ucan(&token).unwrap();
    assert_eq!(
        parsed.capabilities.get("space-123"),
        Some(&CapabilityLevel::Read)
    );
}

#[test]
fn parse_ucan_extracts_write_capability() {
    let key = random_signing_key();
    let token = make_test_token(&key, "space-123", "space/write", 3600);
    let parsed = parse_ucan(&token).unwrap();
    assert_eq!(
        parsed.capabilities.get("space-123"),
        Some(&CapabilityLevel::Write)
    );
}

#[test]
fn parse_ucan_rejects_expired_token() {
    let key = random_signing_key();
    let token = make_test_token(&key, "s", "space/read", 0);
    assert!(matches!(parse_ucan(&token), Err(UcanVerifyError::Expired)));
}

#[test]
fn parse_ucan_rejects_tampered_signature() {
    let key = random_signing_key();
    let mut token = make_test_token(&key, "s", "space/read", 3600);
    // Flip last char
    let last = token.pop().unwrap();
    token.push(if last == 'A' { 'B' } else { 'A' });
    assert!(matches!(
        parse_ucan(&token),
        Err(UcanVerifyError::Signature | UcanVerifyError::MalformedToken(_))
    ));
}

#[test]
fn parse_ucan_populates_proofs_when_present() {
    // Construct a synthetic leaf whose `prf` claim carries one embedded string.
    use ed25519_dalek::Signer;
    let key = random_signing_key();
    let issuer_did = did_from_verifying_key(&key.verifying_key());
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let header = serde_json::json!({"alg": "EdDSA", "typ": "JWT"});
    let payload = serde_json::json!({
        "ucv": "1.0",
        "iss": issuer_did,
        "aud": "did:key:z6MkAudience",
        "cap": { "space:s1": "space/read" },
        "exp": now + 3600,
        "iat": now,
        "prf": ["parent.token.here"],
        "nnc": "n"
    });
    let header_b64 = BASE64URL.encode(serde_json::to_string(&header).unwrap().as_bytes());
    let payload_b64 = BASE64URL.encode(serde_json::to_string(&payload).unwrap().as_bytes());
    let signing_input = format!("{}.{}", header_b64, payload_b64);
    let signature = key.sign(signing_input.as_bytes());
    let token = format!(
        "{}.{}.{}",
        header_b64,
        payload_b64,
        BASE64URL.encode(signature.to_bytes())
    );

    let parsed = parse_ucan(&token).unwrap();
    assert_eq!(parsed.proofs, vec!["parent.token.here".to_string()]);
}

// ---------------------------------------------------------------------------
// require_capability — used by AuthGate on cached UCANs
// ---------------------------------------------------------------------------

fn dummy_validated_ucan(cap: CapabilityLevel, space_id: &str) -> ValidatedUcan {
    let mut caps = HashMap::new();
    caps.insert(space_id.to_string(), cap);
    ValidatedUcan {
        issuer: "did:key:z6MkIssuer".to_string(),
        audience: "did:key:z6MkAudience".to_string(),
        capabilities: caps,
        expires_at: u64::MAX,
        root_did: "did:key:z6MkRoot".to_string(),
    }
}

#[test]
fn require_write_with_only_read_fails() {
    let validated = dummy_validated_ucan(CapabilityLevel::Read, "space-123");
    assert!(matches!(
        require_capability(&validated, "space-123", CapabilityLevel::Write),
        Err(UcanVerifyError::InsufficientCapability { .. })
    ));
}

#[test]
fn require_read_with_write_succeeds() {
    let validated = dummy_validated_ucan(CapabilityLevel::Write, "space-123");
    assert!(require_capability(&validated, "space-123", CapabilityLevel::Read).is_ok());
}

#[test]
fn wrong_space_fails() {
    let validated = dummy_validated_ucan(CapabilityLevel::Admin, "space-123");
    assert!(matches!(
        require_capability(&validated, "other-space", CapabilityLevel::Read),
        Err(UcanVerifyError::MissingCapability { .. })
    ));
}

// ---------------------------------------------------------------------------
// did:key round-trip
// ---------------------------------------------------------------------------

#[test]
fn did_roundtrip() {
    let key = random_signing_key();
    let did = did_from_verifying_key(&key.verifying_key());
    let recovered = public_key_from_did(&did).unwrap();
    assert_eq!(recovered.as_bytes(), key.verifying_key().as_bytes());
}

// ---------------------------------------------------------------------------
// CapabilityLevel ordering + lattice
// ---------------------------------------------------------------------------

#[test]
fn capability_ordering() {
    assert!(CapabilityLevel::Admin > CapabilityLevel::Invite);
    assert!(CapabilityLevel::Invite > CapabilityLevel::Write);
    assert!(CapabilityLevel::Write > CapabilityLevel::Read);
}

#[test]
fn allows_admin_allows_admin() {
    assert!(CapabilityLevel::Admin.allows(&CapabilityLevel::Admin));
}

#[test]
fn allows_admin_allows_invite() {
    assert!(CapabilityLevel::Admin.allows(&CapabilityLevel::Invite));
}

#[test]
fn allows_admin_allows_write() {
    assert!(CapabilityLevel::Admin.allows(&CapabilityLevel::Write));
}

#[test]
fn allows_admin_allows_read() {
    assert!(CapabilityLevel::Admin.allows(&CapabilityLevel::Read));
}

#[test]
fn allows_invite_allows_invite() {
    assert!(CapabilityLevel::Invite.allows(&CapabilityLevel::Invite));
}

#[test]
fn allows_invite_allows_write() {
    assert!(CapabilityLevel::Invite.allows(&CapabilityLevel::Write));
}

#[test]
fn allows_invite_allows_read() {
    assert!(CapabilityLevel::Invite.allows(&CapabilityLevel::Read));
}

#[test]
fn allows_invite_does_not_allow_admin() {
    assert!(!CapabilityLevel::Invite.allows(&CapabilityLevel::Admin));
}

#[test]
fn allows_write_allows_write() {
    assert!(CapabilityLevel::Write.allows(&CapabilityLevel::Write));
}

#[test]
fn allows_write_allows_read() {
    assert!(CapabilityLevel::Write.allows(&CapabilityLevel::Read));
}

#[test]
fn allows_write_does_not_allow_invite() {
    assert!(!CapabilityLevel::Write.allows(&CapabilityLevel::Invite));
}

#[test]
fn allows_write_does_not_allow_admin() {
    assert!(!CapabilityLevel::Write.allows(&CapabilityLevel::Admin));
}

#[test]
fn allows_read_allows_read() {
    assert!(CapabilityLevel::Read.allows(&CapabilityLevel::Read));
}

#[test]
fn allows_read_does_not_allow_write() {
    assert!(!CapabilityLevel::Read.allows(&CapabilityLevel::Write));
}

#[test]
fn allows_read_does_not_allow_invite() {
    assert!(!CapabilityLevel::Read.allows(&CapabilityLevel::Invite));
}

#[test]
fn allows_read_does_not_allow_admin() {
    assert!(!CapabilityLevel::Read.allows(&CapabilityLevel::Admin));
}

// ---------------------------------------------------------------------------
// Audience replay-protection helper
// ---------------------------------------------------------------------------

#[test]
fn require_audience_rejects_mismatch() {
    let validated = dummy_validated_ucan(CapabilityLevel::Read, "s");
    // dummy_validated_ucan sets audience = "did:key:z6MkAudience"
    let result = require_audience(&validated, "did:key:z6MkOtherPeer");
    assert!(
        matches!(result, Err(UcanVerifyError::AudienceMismatch { .. })),
        "UCAN audience mismatch must be rejected to prevent token replay \
         against the wrong recipient"
    );
}

#[test]
fn require_audience_accepts_match() {
    let validated = dummy_validated_ucan(CapabilityLevel::Read, "s");
    assert!(require_audience(&validated, "did:key:z6MkAudience").is_ok());
}

#[test]
fn require_audience_rejects_empty_expected() {
    let validated = dummy_validated_ucan(CapabilityLevel::Read, "s");
    let result = require_audience(&validated, "");
    assert!(matches!(
        result,
        Err(UcanVerifyError::EmptyExpectedAudience)
    ));
}

#[test]
fn require_audience_rejects_empty_expected_regardless_of_token_audience() {
    // Construct a ValidatedUcan with an empty audience by hand — this
    // shape cannot come out of validate_token (which rejects empty aud),
    // but guards the require_audience contract against a future change
    // that loosens validate_token's audience check.
    let validated = ValidatedUcan {
        issuer: "did:key:z6MkIssuer".to_string(),
        audience: String::new(),
        capabilities: HashMap::new(),
        expires_at: 0,
        root_did: "did:key:z6MkRoot".to_string(),
    };
    let result = require_audience(&validated, "");
    assert!(
        matches!(result, Err(UcanVerifyError::EmptyExpectedAudience)),
        "an empty expected_audience must never be accepted, even when \
         the validated audience is also empty (defense in depth: empty \
         == empty would silently bypass the replay-protection layer)"
    );
}
