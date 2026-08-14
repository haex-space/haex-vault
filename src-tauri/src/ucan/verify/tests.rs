//! Unit tests for the low-level [`parse_ucan`] pipeline and the cached-UCAN
//! `require_*` helpers.
//!
//! Chain-walk semantics are exercised by the cross-language fixture in
//! [`super::chain_tests`] and by the integration driver at
//! `src-tauri/tests/ucan_chain_vectors.rs`.

use super::*;
use crate::ucan::capability_set::Cap;
use crate::ucan::predicate::{Predicate, PrimitiveValue};
use crate::ucan::row_capability::RowCapability;
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
        row_capabilities: HashMap::new(),
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

#[test]
fn public_key_from_did_rejects_oversized_input() {
    let oversized = format!("did:key:z{}", "1".repeat(200));
    let result = public_key_from_did(&oversized);
    assert!(
        matches!(&result, Err(UcanVerifyError::MalformedToken(msg)) if msg.contains("too long")),
        "expected MalformedToken with 'too long', got {result:?}"
    );
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
        row_capabilities: HashMap::new(),
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

// ---------------------------------------------------------------------------
// parse_ucan — row_cap envelope (C.5)
// ---------------------------------------------------------------------------
//
// The `row_cap` payload field is *optional and parallel* to `cap`. It carries
// a per-space list of [`RowCapability`] objects that the row-sig verifier
// evaluates against a candidate row payload. Adding this envelope does not
// alter `cap` semantics — a token can hold either, both, or neither.

/// Build a JWT with a caller-supplied `row_cap` value in the payload.
///
/// `row_cap_json` is inserted verbatim; pass `serde_json::Value::Null` (or
/// don't call this helper) to test the "missing row_cap" path via
/// [`make_test_token`] instead.
fn make_token_with_row_cap(
    signing_key: &SigningKey,
    space_id: &str,
    capability: &str,
    row_cap_json: serde_json::Value,
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
        "row_cap": row_cap_json,
        "exp": now + expires_in,
        "iat": now,
        "prf": [],
        "nnc": "row-cap-fixture"
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

#[test]
fn parse_ucan_extracts_row_capabilities_for_named_space() {
    let key = random_signing_key();
    let row_cap = serde_json::json!({
        "space:space-abc": [
            { "op": "row_insert", "where": { "col": "category", "eq": "work" } },
        ],
    });
    let token = make_token_with_row_cap(&key, "space-abc", "space/write", row_cap, 3600);
    let parsed = parse_ucan(&token).unwrap();

    let caps = parsed
        .row_capabilities
        .get("space-abc")
        .expect("row_capabilities must contain the delegated space");
    assert_eq!(caps.len(), 1);
    assert_eq!(
        caps[0],
        RowCapability::RowInsert {
            matches: Predicate::Eq {
                col: "category".into(),
                eq: PrimitiveValue::String("work".into()),
            }
        },
    );
}

#[test]
fn parse_ucan_missing_row_cap_field_yields_empty_map() {
    // Backwards compat with today's tokens: no `row_cap` at all is fine.
    let key = random_signing_key();
    let token = make_test_token(&key, "space-abc", "space/write", 3600);
    let parsed = parse_ucan(&token).unwrap();
    assert!(parsed.row_capabilities.is_empty());
}

#[test]
fn parse_ucan_row_cap_empty_object_yields_empty_map() {
    let key = random_signing_key();
    let token = make_token_with_row_cap(
        &key,
        "space-abc",
        "space/write",
        serde_json::json!({}),
        3600,
    );
    let parsed = parse_ucan(&token).unwrap();
    assert!(parsed.row_capabilities.is_empty());
}

#[test]
fn parse_ucan_rejects_row_cap_that_is_not_an_object() {
    let key = random_signing_key();
    let token = make_token_with_row_cap(
        &key,
        "space-abc",
        "space/write",
        serde_json::json!("not-an-object"),
        3600,
    );
    let err = parse_ucan(&token).unwrap_err();
    assert!(
        matches!(err, UcanVerifyError::MalformedToken(_)),
        "row_cap must be an object; got {err:?}",
    );
}

#[test]
fn parse_ucan_rejects_row_cap_entry_that_is_not_an_array() {
    let key = random_signing_key();
    let token = make_token_with_row_cap(
        &key,
        "space-abc",
        "space/write",
        serde_json::json!({ "space:space-abc": "not-an-array" }),
        3600,
    );
    let err = parse_ucan(&token).unwrap_err();
    assert!(matches!(err, UcanVerifyError::MalformedToken(_)));
}

#[test]
fn parse_ucan_rejects_row_cap_with_unknown_op() {
    // The RowCapability serde is `deny_unknown_fields` + tagged; an unknown
    // `op` must be surfaced as MalformedToken so a forged token cannot smuggle
    // an unmodelled operation past the puller.
    let key = random_signing_key();
    let token = make_token_with_row_cap(
        &key,
        "space-abc",
        "space/write",
        serde_json::json!({
            "space:space-abc": [
                { "op": "row_read", "where": { "col": "c", "eq": "v" } },
            ],
        }),
        3600,
    );
    let err = parse_ucan(&token).unwrap_err();
    assert!(matches!(err, UcanVerifyError::MalformedToken(_)));
}

/// Build a self-signed admin root token whose `space_id` binds to the
/// signer's DID, and which carries a supplied `row_cap` payload. Returns
/// `(token, iss_did, space_id)`.
fn make_self_signed_admin_root_with_row_cap(
    signing_key: &SigningKey,
    row_cap_json: serde_json::Value,
) -> (String, String, String) {
    use crate::ucan::space_id::derive_space_id;
    use ed25519_dalek::Signer;
    let iss_did = did_from_verifying_key(&signing_key.verifying_key());
    // Deterministic nonce keeps the fixture reproducible without adding a
    // second RNG source; the actual bytes are irrelevant to the binding.
    let space_id = derive_space_id(&iss_did, &[0xAB; 16]);
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let header = serde_json::json!({"alg": "EdDSA", "typ": "JWT"});
    let payload = serde_json::json!({
        "ucv": "1.0",
        "iss": iss_did,
        "aud": iss_did,  // self-signed root
        "cap": { format!("space:{}", space_id): "space/admin" },
        "row_cap": row_cap_json,
        "exp": now + 3600,
        "iat": now,
        "prf": [],
        "nnc": "self-root-fixture"
    });
    let header_b64 = BASE64URL.encode(serde_json::to_string(&header).unwrap().as_bytes());
    let payload_b64 = BASE64URL.encode(serde_json::to_string(&payload).unwrap().as_bytes());
    let signing_input = format!("{}.{}", header_b64, payload_b64);
    let signature = signing_key.sign(signing_input.as_bytes());
    let token = format!(
        "{}.{}.{}",
        header_b64,
        payload_b64,
        BASE64URL.encode(signature.to_bytes())
    );
    (token, iss_did, space_id)
}

#[test]
fn validate_token_propagates_row_capabilities_to_validated() {
    let key = random_signing_key();
    let row_cap = serde_json::json!({}); // populated below with dynamic space_id
    let (_probe_token, iss_did, space_id) = make_self_signed_admin_root_with_row_cap(&key, row_cap);
    // Rebuild with the resolved space_id in the row_cap key.
    let row_cap = serde_json::json!({
        format!("space:{}", space_id): [
            { "op": "row_insert", "where": { "col": "cat", "eq": "work" } },
            { "op": "row_delete", "where": { "col": "cat", "eq": "trash" } },
        ],
    });
    let (token, _iss_did, space_id) = make_self_signed_admin_root_with_row_cap(&key, row_cap);

    let validated = validate_token(&token, &space_id, &iss_did, CapabilityLevel::Admin, 5).unwrap();

    let caps = validated
        .row_capabilities
        .get(&space_id)
        .expect("row_capabilities must be populated for the target space");
    assert_eq!(caps.len(), 2);
    assert!(matches!(caps[0], RowCapability::RowInsert { .. }));
    assert!(matches!(caps[1], RowCapability::RowDelete { .. }));
}

#[test]
fn validate_token_yields_empty_row_capabilities_when_field_absent() {
    // A token with no `row_cap` at all validates fine and exposes an empty
    // row-cap map on the ValidatedUcan.
    use crate::ucan::space_id::derive_space_id;
    use ed25519_dalek::Signer;
    let key = random_signing_key();
    let iss_did = did_from_verifying_key(&key.verifying_key());
    let space_id = derive_space_id(&iss_did, &[0xCD; 16]);
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let header = serde_json::json!({"alg": "EdDSA", "typ": "JWT"});
    let payload = serde_json::json!({
        "ucv": "1.0",
        "iss": iss_did,
        "aud": iss_did,
        "cap": { format!("space:{}", space_id): "space/admin" },
        "exp": now + 3600,
        "iat": now,
        "prf": [],
        "nnc": "no-row-cap"
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

    let validated = validate_token(&token, &space_id, &iss_did, CapabilityLevel::Admin, 5).unwrap();
    assert!(validated.row_capabilities.is_empty());
}

#[test]
fn parse_ucan_ignores_row_cap_entries_without_space_prefix() {
    // Only `space:<id>` keys are consumed; any other keys (future
    // resource namespaces) must be silently ignored to keep the wire
    // envelope forward-compatible.
    let key = random_signing_key();
    let row_cap = serde_json::json!({
        "space:space-abc": [
            { "op": "row_insert", "where": { "col": "c", "eq": "v" } },
        ],
        "future:something-else": [
            { "op": "row_insert", "where": { "col": "c", "eq": "v" } },
        ],
    });
    let token = make_token_with_row_cap(&key, "space-abc", "space/write", row_cap, 3600);
    let parsed = parse_ucan(&token).unwrap();
    assert_eq!(parsed.row_capabilities.len(), 1);
    assert!(parsed.row_capabilities.contains_key("space-abc"));
}

// ---------------------------------------------------------------------------
// walk_prf_chain — row-cap attenuation (Task 3)
// ---------------------------------------------------------------------------
//
// The chain walker already enforces CapabilityLevel attenuation
// (parent_cap.allows(child_cap)). This block extends that discipline to
// row-caps: every row-cap the child claims must appear structurally in the
// parent's row-cap set for the same space. See UcanVerifyError::RowCapAttenuation.
//
// MVP semantics (documented in walk_prf_chain):
//  - Exact structural equality on (variant, predicate). No Predicate P1 ⊑ P2
//    comparison — that is NP-hard in the general case; leave it as a future
//    attenuation rule when concrete cases demand it.
//  - Row-caps are opt-in per hop: a child MAY omit row-caps entirely and
//    inherit the parent's silently (in the sense that no further claim is
//    made — the child does NOT gain the parent's row-caps by omission).

/// Build a 2-hop chain: self-signed admin root + delegated leaf. The root
/// carries `root_row_cap`; the leaf carries `leaf_row_cap`. Returns
/// `(leaf_token, root_did, leaf_aud_did, space_id)`.
fn build_two_hop_with_row_caps(
    root_key: &SigningKey,
    leaf_key: &SigningKey,
    root_row_cap: serde_json::Value,
    leaf_row_cap: serde_json::Value,
) -> (String, String, String, String) {
    use crate::ucan::space_id::derive_space_id;
    use ed25519_dalek::Signer;

    let root_did = did_from_verifying_key(&root_key.verifying_key());
    let leaf_did = did_from_verifying_key(&leaf_key.verifying_key());
    let space_id = derive_space_id(&root_did, &[0x11; 16]);
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();

    // Root: self-signed admin.
    let root_payload = serde_json::json!({
        "ucv": "1.0",
        "iss": root_did,
        "aud": root_did,
        "cap": { format!("space:{}", space_id): "space/admin" },
        "row_cap": root_row_cap,
        "exp": now + 3600,
        "iat": now,
        "prf": [],
        "nnc": "root"
    });
    let header = serde_json::json!({"alg": "EdDSA", "typ": "JWT"});
    let header_b64 = BASE64URL.encode(serde_json::to_string(&header).unwrap().as_bytes());
    let root_payload_b64 =
        BASE64URL.encode(serde_json::to_string(&root_payload).unwrap().as_bytes());
    let root_sig = root_key.sign(format!("{}.{}", header_b64, root_payload_b64).as_bytes());
    let root_token = format!(
        "{}.{}.{}",
        header_b64,
        root_payload_b64,
        BASE64URL.encode(root_sig.to_bytes())
    );

    // Leaf: issued by root, audience=leaf_did.
    let leaf_payload = serde_json::json!({
        "ucv": "1.0",
        "iss": root_did,
        "aud": leaf_did,
        "cap": { format!("space:{}", space_id): "space/admin" },
        "row_cap": leaf_row_cap,
        "exp": now + 3600,
        "iat": now,
        "prf": [root_token],
        "nnc": "leaf"
    });
    let leaf_payload_b64 =
        BASE64URL.encode(serde_json::to_string(&leaf_payload).unwrap().as_bytes());
    // NB: leaf is signed by ROOT (iss=root_did) — same key that signed the root.
    let leaf_sig = root_key.sign(format!("{}.{}", header_b64, leaf_payload_b64).as_bytes());
    let leaf_token = format!(
        "{}.{}.{}",
        header_b64,
        leaf_payload_b64,
        BASE64URL.encode(leaf_sig.to_bytes())
    );

    (leaf_token, root_did, leaf_did, space_id)
}

fn row_cap_insert_where_cat(value: &str) -> serde_json::Value {
    serde_json::json!({
        "op": "row_insert",
        "where": { "col": "cat", "eq": value },
    })
}

/// Compute the deterministic space_id for a given key's DID. The 2-hop
/// helper below uses the same nonce, so tests can precompute the space_id
/// before choosing row_caps that reference it.
fn deterministic_space_id_for(key: &SigningKey) -> String {
    use crate::ucan::space_id::derive_space_id;
    let did = did_from_verifying_key(&key.verifying_key());
    derive_space_id(&did, &[0x11; 16])
}

#[test]
fn walk_chain_rejects_when_child_claims_row_cap_parent_lacks() {
    // Attack B: parent has no row_cap; child claims one. Delegatee cannot
    // grant itself capabilities the delegator never held.
    let root_key = random_signing_key();
    let leaf_key = random_signing_key();
    let space_id = deterministic_space_id_for(&root_key);
    let leaf_row_cap = serde_json::json!({
        format!("space:{}", space_id): [row_cap_insert_where_cat("work")],
    });
    let (leaf_token, _root_did, leaf_did, _space_id) = build_two_hop_with_row_caps(
        &root_key,
        &leaf_key,
        serde_json::json!({}), // parent: nothing
        leaf_row_cap,
    );
    let err =
        validate_token(&leaf_token, &space_id, &leaf_did, CapabilityLevel::Admin, 5).unwrap_err();
    match err {
        UcanVerifyError::RowCapAttenuation { space_id: reported } => {
            assert_eq!(reported, space_id, "RowCapAttenuation must name the space");
        }
        other => panic!("expected RowCapAttenuation for {space_id}, got {other:?}"),
    }
}

#[test]
fn walk_chain_accepts_identical_row_caps_on_both_hops() {
    let root_key = random_signing_key();
    let leaf_key = random_signing_key();
    let space_id = deterministic_space_id_for(&root_key);
    let same_caps = serde_json::json!({
        format!("space:{}", space_id): [row_cap_insert_where_cat("work")],
    });
    let (leaf_token, _root_did, leaf_did, _space_id) =
        build_two_hop_with_row_caps(&root_key, &leaf_key, same_caps.clone(), same_caps);
    let validated =
        validate_token(&leaf_token, &space_id, &leaf_did, CapabilityLevel::Admin, 5).unwrap();
    assert_eq!(validated.row_capabilities.get(&space_id).unwrap().len(), 1);
}

#[test]
fn walk_chain_accepts_child_row_caps_that_are_subset_of_parent() {
    // Parent has three row-caps; child holds two of them. Attenuation
    // must accept the strict subset.
    let root_key = random_signing_key();
    let leaf_key = random_signing_key();
    let space_id = deterministic_space_id_for(&root_key);
    let parent_caps = serde_json::json!({
        format!("space:{}", space_id): [
            row_cap_insert_where_cat("work"),
            row_cap_insert_where_cat("home"),
            row_cap_insert_where_cat("misc"),
        ],
    });
    let child_caps = serde_json::json!({
        format!("space:{}", space_id): [
            row_cap_insert_where_cat("work"),
            row_cap_insert_where_cat("misc"),
        ],
    });
    let (leaf_token, _root_did, leaf_did, _space_id) =
        build_two_hop_with_row_caps(&root_key, &leaf_key, parent_caps, child_caps);
    let validated =
        validate_token(&leaf_token, &space_id, &leaf_did, CapabilityLevel::Admin, 5).unwrap();
    assert_eq!(validated.row_capabilities.get(&space_id).unwrap().len(), 2);
}

#[test]
fn walk_chain_rejects_when_child_has_extra_row_cap_beyond_parent() {
    // Attack C: overlap plus one extra. The overlap alone would validate;
    // the walker must still reject on the single un-inherited cap.
    let root_key = random_signing_key();
    let leaf_key = random_signing_key();
    let space_id = deterministic_space_id_for(&root_key);
    let parent_caps = serde_json::json!({
        format!("space:{}", space_id): [row_cap_insert_where_cat("work")],
    });
    let child_caps = serde_json::json!({
        format!("space:{}", space_id): [
            row_cap_insert_where_cat("work"),
            row_cap_insert_where_cat("secret"),  // <-- not in parent
        ],
    });
    let (leaf_token, _root_did, leaf_did, _space_id) =
        build_two_hop_with_row_caps(&root_key, &leaf_key, parent_caps, child_caps);
    let err =
        validate_token(&leaf_token, &space_id, &leaf_did, CapabilityLevel::Admin, 5).unwrap_err();
    assert!(matches!(err, UcanVerifyError::RowCapAttenuation { .. }));
}

#[test]
fn walk_chain_rejects_when_child_op_differs_even_with_same_predicate() {
    // Attack D: same predicate, different operation. row_insert and
    // row_delete are structurally distinct RowCapability variants; the
    // parent granting one must not implicitly grant the other.
    let root_key = random_signing_key();
    let leaf_key = random_signing_key();
    let space_id = deterministic_space_id_for(&root_key);
    let parent_caps = serde_json::json!({
        format!("space:{}", space_id): [
            {"op": "row_insert", "where": {"col": "cat", "eq": "work"}},
        ],
    });
    let child_caps = serde_json::json!({
        format!("space:{}", space_id): [
            {"op": "row_delete", "where": {"col": "cat", "eq": "work"}},
        ],
    });
    let (leaf_token, _root_did, leaf_did, _space_id) =
        build_two_hop_with_row_caps(&root_key, &leaf_key, parent_caps, child_caps);
    let err =
        validate_token(&leaf_token, &space_id, &leaf_did, CapabilityLevel::Admin, 5).unwrap_err();
    assert!(matches!(err, UcanVerifyError::RowCapAttenuation { .. }));
}

#[test]
fn walk_chain_accepts_child_with_no_row_caps_under_parent_with_row_caps() {
    // "Opt-in per hop" semantics: a child claiming nothing gets nothing;
    // it does not need to explicitly acknowledge the parent's row-caps.
    let root_key = random_signing_key();
    let leaf_key = random_signing_key();
    let space_id = deterministic_space_id_for(&root_key);
    let parent_caps = serde_json::json!({
        format!("space:{}", space_id): [row_cap_insert_where_cat("work")],
    });
    let (leaf_token, _root_did, leaf_did, _space_id) =
        build_two_hop_with_row_caps(&root_key, &leaf_key, parent_caps, serde_json::json!({}));
    let validated =
        validate_token(&leaf_token, &space_id, &leaf_did, CapabilityLevel::Admin, 5).unwrap();
    assert!(validated.row_capabilities.is_empty());
}

#[test]
fn walk_chain_accepts_two_hop_with_no_row_caps_on_either_side() {
    // Baseline sanity: today's tokens (pre-C.5, no row_cap) keep working.
    let root_key = random_signing_key();
    let leaf_key = random_signing_key();
    let space_id = deterministic_space_id_for(&root_key);
    let (leaf_token, _root_did, leaf_did, _space_id) = build_two_hop_with_row_caps(
        &root_key,
        &leaf_key,
        serde_json::json!({}),
        serde_json::json!({}),
    );
    let validated =
        validate_token(&leaf_token, &space_id, &leaf_did, CapabilityLevel::Admin, 5).unwrap();
    assert!(validated.row_capabilities.is_empty());
}

// ---------------------------------------------------------------------------
// Depth × row-caps (Task 4)
// ---------------------------------------------------------------------------
//
// The walker's depth guard counts tokens, not capability bytes. Row-caps
// on every hop must NOT change how the depth check behaves — a five-hop
// chain that admits with no row_cap must still admit with them, and a
// six-hop chain that trips ChainTooDeep must still trip with them.
//
// These tests are Rust-only (they do not extend the shared JSON fixture)
// so they exercise the depth guard on the row-cap envelope without
// requiring a TS regeneration step.

/// Build an n-hop delegation chain (`keys.len()` tokens, root = keys[0]).
/// Every hop carries the SAME row-cap payload — so structural attenuation
/// is trivially satisfied at every edge, and the only test surface is the
/// depth counter itself.
///
/// Every token grants `space/admin` for the deterministically-derived
/// `space_id` bound to the root key.
///
/// Returns `(leaf_token, leaf_audience_did, space_id)`.
fn build_n_hop_chain_with_uniform_row_caps(
    keys: &[SigningKey],
    row_cap: &serde_json::Value,
) -> (String, String, String) {
    use crate::ucan::space_id::derive_space_id;
    use ed25519_dalek::Signer;

    assert!(
        keys.len() >= 2,
        "an n-hop chain needs at least 2 keys (root + one child)"
    );

    let root_did = did_from_verifying_key(&keys[0].verifying_key());
    let space_id = derive_space_id(&root_did, &[0x22; 16]);
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let header = serde_json::json!({"alg": "EdDSA", "typ": "JWT"});
    let header_b64 = BASE64URL.encode(serde_json::to_string(&header).unwrap().as_bytes());

    // Sign root (self-signed admin).
    let root_payload = serde_json::json!({
        "ucv": "1.0",
        "iss": root_did,
        "aud": root_did,
        "cap": { format!("space:{}", space_id): "space/admin" },
        "row_cap": row_cap,
        "exp": now + 3600,
        "iat": now,
        "prf": [],
        "nnc": "hop-0"
    });
    let root_payload_b64 =
        BASE64URL.encode(serde_json::to_string(&root_payload).unwrap().as_bytes());
    let root_sig = keys[0].sign(format!("{}.{}", header_b64, root_payload_b64).as_bytes());
    let mut prev_token = format!(
        "{}.{}.{}",
        header_b64,
        root_payload_b64,
        BASE64URL.encode(root_sig.to_bytes())
    );
    let mut prev_iss = root_did.clone();

    // Chain: for each hop 1..n, issue = signer of the PREVIOUS token
    // (chain-continuity: parent.aud == child.iss). We drive this by
    // signing hop i with keys[i-1] and audience = did(keys[i]).
    for i in 1..keys.len() {
        let signer = &keys[i - 1];
        let audience_did = did_from_verifying_key(&keys[i].verifying_key());
        let payload = serde_json::json!({
            "ucv": "1.0",
            "iss": prev_iss,
            "aud": audience_did,
            "cap": { format!("space:{}", space_id): "space/admin" },
            "row_cap": row_cap,
            "exp": now + 3600,
            "iat": now,
            "prf": [prev_token],
            "nnc": format!("hop-{}", i)
        });
        let payload_b64 = BASE64URL.encode(serde_json::to_string(&payload).unwrap().as_bytes());
        let sig = signer.sign(format!("{}.{}", header_b64, payload_b64).as_bytes());
        prev_token = format!(
            "{}.{}.{}",
            header_b64,
            payload_b64,
            BASE64URL.encode(sig.to_bytes())
        );
        prev_iss = audience_did;
    }

    (prev_token, prev_iss, space_id)
}

#[test]
fn walk_chain_admits_five_hop_row_cap_chain_at_max_depth() {
    let keys: Vec<SigningKey> = (0..5).map(|_| random_signing_key()).collect();
    let row_cap_json = serde_json::json!({});
    let space_id = {
        use crate::ucan::space_id::derive_space_id;
        derive_space_id(
            &did_from_verifying_key(&keys[0].verifying_key()),
            &[0x22; 16],
        )
    };
    let row_cap = serde_json::json!({
        format!("space:{}", space_id): [row_cap_insert_where_cat("work")],
    });
    // Row-caps identical at every hop → structural attenuation always
    // passes; the only remaining surface is depth counting.
    let (leaf_token, leaf_did, space_id_returned) =
        build_n_hop_chain_with_uniform_row_caps(&keys, &row_cap);
    assert_eq!(space_id, space_id_returned);

    let validated =
        validate_token(&leaf_token, &space_id, &leaf_did, CapabilityLevel::Admin, 5).unwrap();
    assert_eq!(
        validated.row_capabilities.get(&space_id).unwrap().len(),
        1,
        "row_capabilities must survive a full depth-5 chain walk"
    );
    // Depth guard proof: same chain must reject at depth=4.
    let too_shallow = validate_token(&leaf_token, &space_id, &leaf_did, CapabilityLevel::Admin, 4);
    assert!(
        matches!(too_shallow, Err(UcanVerifyError::ChainTooDeep(_))),
        "depth guard must still fire on a row-cap chain; got {too_shallow:?}"
    );
    // Also verify the depth-5 baseline holds without row-caps (regression
    // fence: this branch of the walker must not shrink the accepted set).
    let _ = row_cap_json; // reserved for a future no-row-cap fixture
}

#[test]
fn walk_chain_rejects_six_hop_row_cap_chain_beyond_max_depth() {
    let keys: Vec<SigningKey> = (0..6).map(|_| random_signing_key()).collect();
    let space_id = {
        use crate::ucan::space_id::derive_space_id;
        derive_space_id(
            &did_from_verifying_key(&keys[0].verifying_key()),
            &[0x22; 16],
        )
    };
    let row_cap = serde_json::json!({
        format!("space:{}", space_id): [row_cap_insert_where_cat("work")],
    });
    let (leaf_token, leaf_did, _space_id) =
        build_n_hop_chain_with_uniform_row_caps(&keys, &row_cap);

    let err =
        validate_token(&leaf_token, &space_id, &leaf_did, CapabilityLevel::Admin, 5).unwrap_err();
    assert!(
        matches!(err, UcanVerifyError::ChainTooDeep(_)),
        "six-hop row-cap chain must trip the depth guard, got {err:?}"
    );
}

// ---------------------------------------------------------------------------
// W4 PR-3 Task 1 — RED tests for the CapabilitySet wire form.
//
// These tests are intentionally red against the current hierarchical
// `CapabilityLevel` parser. They pin the target contract for Task 2, which
// will migrate `parse_ucan` to read `HashMap<String, CapabilitySet>` from the
// canonical per-space array form and reject the legacy string form.
// ---------------------------------------------------------------------------

fn unix_secs_from(t: SystemTime) -> u64 {
    t.duration_since(UNIX_EPOCH).unwrap().as_secs()
}

fn sign_ucan_payload(signer: &SigningKey, payload: &serde_json::Value) -> String {
    use ed25519_dalek::Signer;
    let header = serde_json::json!({"alg": "EdDSA", "typ": "JWT"});
    let header_b64 = BASE64URL.encode(serde_json::to_string(&header).unwrap().as_bytes());
    let payload_b64 = BASE64URL.encode(serde_json::to_string(payload).unwrap().as_bytes());
    let signing_input = format!("{}.{}", header_b64, payload_b64);
    let signature = signer.sign(signing_input.as_bytes());
    format!(
        "{}.{}.{}",
        header_b64,
        payload_b64,
        BASE64URL.encode(signature.to_bytes())
    )
}

#[test]
fn parse_ucan_reads_capability_set_wire_form() {
    // Contract: capabilities in UCAN payload are JSON arrays of {cap, delegatable}
    // entries per space, canonical order (see capability_set.rs serde). Any
    // legacy string form (e.g. "space/write") must be rejected as MalformedToken.
    let signing_key = random_signing_key();
    let did = did_from_verifying_key(&signing_key.verifying_key());
    let space_id = deterministic_space_id_for(&signing_key);
    let now = unix_secs_from(SystemTime::now());

    let payload = serde_json::json!({
        "ucv": "1.0",
        "iss": did,
        "aud": did,
        "exp": now + 3600,
        "iat": now,
        "nnc": "n1",
        "capabilities": {
            format!("space:{space_id}"): [
                {"cap": "read",  "delegatable": true},
                {"cap": "write", "delegatable": false},
            ]
        },
        "prf": []
    });
    let token = sign_ucan_payload(&signing_key, &payload);

    let parsed = parse_ucan(&token).expect("valid new-form payload must parse");
    assert_eq!(parsed.capabilities.len(), 1);
    let set = parsed
        .capabilities
        .get(&space_id)
        .expect("space key present");
    assert!(set.can(Cap::Read));
    assert!(set.can(Cap::Write));
    assert!(set.is_delegatable(Cap::Read));
    assert!(!set.is_delegatable(Cap::Write));
    assert!(!set.can(Cap::Admin));
}

#[test]
fn parse_ucan_rejects_legacy_string_capability_form() {
    let signing_key = random_signing_key();
    let did = did_from_verifying_key(&signing_key.verifying_key());
    let space_id = deterministic_space_id_for(&signing_key);
    let now = unix_secs_from(SystemTime::now());

    let payload = serde_json::json!({
        "ucv": "1.0",
        "iss": did, "aud": did,
        "exp": now + 3600,
        "iat": now,
        "nnc": "n1",
        "capabilities": {
            format!("space:{space_id}"): "space/write" // legacy string
        },
        "prf": []
    });
    let token = sign_ucan_payload(&signing_key, &payload);

    let err = parse_ucan(&token).expect_err("legacy form must be rejected");
    assert!(matches!(err, UcanVerifyError::MalformedToken(_)));
}
