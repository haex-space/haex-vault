//! UCAN token verification.
//!
//! Validates incoming UCAN tokens (EdDSA / JWT format) that are compatible with
//! the TypeScript `@haex-space/ucan` library.
//!
//! ## Two-stage API
//!
//! - [`parse_ucan`] decodes one token, verifies its Ed25519 signature, and
//!   enforces the `exp` claim. It does **not** touch audience, capability, or
//!   the prf chain. Callers use this when the target `space_id` is only known
//!   after inspecting the leaf's capability map (multi-space routing in
//!   `peer_storage::handlers::dispatch`).
//! - [`validate_token`] is the full authorisation pipeline for one leaf token
//!   bound to a known `expected_space_id`: parse + audience + capability +
//!   [`walk_prf_chain`] to a self-signed root + self-certifying `space_id`
//!   binding via [`crate::ucan::space_id::verify_space_id_binding`].
//!
//! The chain walker exists so authorisation no longer depends on the leader
//! having pre-cached a member's UCAN. Any peer can present a leaf that carries
//! its full delegation ancestry inline via the `prf` claim, and any verifier
//! can walk that ancestry to the space-root DID and check that the
//! self-certifying `space_id` binds to it.

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

/// DoS mitigation upper bound on a `did:key:z…` string length before we
/// let `bs58::decode` allocate. A real Ed25519 did:key is ~56 chars; 128 is
/// a generous safety margin. Mirrors the guard in
/// [`crate::ucan::space_id::MAX_SPACE_ID_LEN_BYTES`] — both are network-input
/// facing (via `parse_ucan` on the peek path in `peer_storage`).
const MAX_DID_KEY_LEN_BYTES: usize = 128;

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

    /// Strict-subset lattice for capability attenuation:
    /// `Admin > Invite > Write > Read`.
    ///
    /// Returns `true` iff `self` grants at least what `requested` needs — i.e.
    /// a parent token holding `self` may delegate a child token requesting
    /// `requested`. Used by the chain walker to enforce that a child's
    /// capability is ≤ its parent's along a `prf` UCAN chain.
    ///
    /// The match is written out explicitly (rather than delegating to `Ord`)
    /// so that adding an orthogonal capability later — one that must only
    /// allow itself — forces this arm to be updated by hand rather than being
    /// silently ordered by discriminant.
    pub fn allows(&self, requested: &CapabilityLevel) -> bool {
        use CapabilityLevel::*;
        match (self, requested) {
            (Admin, Admin | Invite | Write | Read) => true,
            (Invite, Invite | Write | Read) => true,
            (Write, Write | Read) => true,
            (Read, Read) => true,
            _ => false,
        }
    }
}

// ---------------------------------------------------------------------------
// Parsed & validated payloads
// ---------------------------------------------------------------------------

/// Parsed UCAN token payload with a verified Ed25519 signature.
///
/// Emitted by [`parse_ucan`]. Signature and `exp` have been checked; audience,
/// capability, and the `prf` chain have **not**. Callers should either drive
/// the full pipeline via [`validate_token`] or handle those checks themselves
/// via the `require_*` helpers.
#[derive(Debug, Clone)]
pub struct ParsedUcan {
    pub iss: String,
    pub aud: String,
    pub exp: u64,
    pub iat: u64,
    /// `space_id → CapabilityLevel` from the `cap` claim. Every entry has
    /// prefix `space:<id>` stripped so the map key is the raw `space_id`.
    pub capabilities: HashMap<String, CapabilityLevel>,
    /// Raw proof tokens (embedded UCAN JWT strings) from the `prf` claim.
    /// Kept as strings so [`walk_prf_chain`] can recursively re-parse them.
    pub proofs: Vec<String>,
}

/// UCAN that has passed the full pipeline for one target space: signature,
/// expiry, audience, capability floor, prf chain walk to a self-signed root,
/// and `space_id`-binding to that root DID.
#[derive(Debug, Clone)]
pub struct ValidatedUcan {
    pub issuer: String,
    pub audience: String,
    /// space_id → capability level from the leaf token. Multi-space UCANs are
    /// permitted at parse time, but [`validate_token`] only chain-verifies
    /// against a single `expected_space_id`.
    pub capabilities: HashMap<String, CapabilityLevel>,
    pub expires_at: u64,
    /// DID of the self-signed chain root — the Space-Root DID that
    /// `space_id` must bind to. Populated by [`walk_prf_chain`] and
    /// cross-checked against `expected_space_id` via
    /// [`crate::ucan::space_id::verify_space_id_binding`].
    pub root_did: String,
}

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

#[derive(Debug, thiserror::Error)]
pub enum UcanVerifyError {
    #[error("Malformed token: {0}")]
    MalformedToken(String),
    #[error("Invalid signature")]
    Signature,
    #[error("Token expired")]
    Expired,
    #[error("Audience mismatch: expected {expected}, got {actual}")]
    AudienceMismatch { expected: String, actual: String },
    #[error("require_audience called with an empty expected audience")]
    EmptyExpectedAudience,
    #[error("Missing capability for space {space_id}")]
    MissingCapability { space_id: String },
    #[error("Insufficient capability: need {required:?}, have {actual:?}")]
    InsufficientCapability {
        required: CapabilityLevel,
        actual: CapabilityLevel,
    },
    #[error("Unknown capability: {0}")]
    UnknownCapability(String),
    /// prf chain exceeded `max_chain_depth` edges without reaching a root.
    /// Wrapped value is the depth at which we gave up.
    #[error("prf chain too deep: {0}")]
    ChainTooDeep(usize),
    /// A parent token's `aud` did not match the child's `iss` — the chain
    /// is not a valid delegation graph.
    #[error("prf chain broken (parent.aud != child.iss)")]
    ChainBroken,
    /// A child token requested a strictly higher capability than its parent
    /// (e.g. Write child under a Read parent).
    #[error("child capability exceeds parent capability")]
    CapabilityEscalation,
    /// Chain terminated at a token that is not a proper root — either its
    /// `proofs` list is non-empty but the walk depth was exhausted first,
    /// or it has no proofs but is not self-signed at Admin level.
    #[error("root token is not self-signed")]
    RootNotSelfSigned,
    /// The self-certifying `space_id` did not verify against the resolved
    /// chain root DID.
    #[error("space_id does not bind to the resolved root DID")]
    RootBindingMismatch,
    /// The `space_id` string is structurally malformed (bad base58, wrong
    /// length, oversized DoS-guard) so no binding check is possible.
    #[error("space_id is malformed and cannot be bound")]
    RootBindingMalformed,
    /// A parent token in the chain does not name `expected_space_id` in its
    /// capability map — the chain drifted to a different space.
    #[error("chain references the wrong space")]
    WrongSpace,
}

// ---------------------------------------------------------------------------
// Layer 0: parse token (structure + signature + expiry)
// ---------------------------------------------------------------------------

/// Parse a UCAN token, verify its Ed25519 signature, and enforce the `exp`
/// claim. Audience, capability, and the `prf` chain are **not** checked here.
///
/// This exists so callers that only know the target `space_id` after
/// inspecting the leaf's capability map (multi-space routing in
/// `peer_storage`) can extract the map cheaply before invoking the full
/// [`validate_token`] pipeline.
pub fn parse_ucan(token: &str) -> Result<ParsedUcan, UcanVerifyError> {
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
        .map_err(|_| UcanVerifyError::Signature)?;

    // Check expiry. If the system clock is implausibly skewed (before UNIX
    // epoch), fail closed — otherwise unwrap_or_default() would return 0 and
    // every token with exp > 0 would appear valid.
    let exp = payload["exp"]
        .as_u64()
        .ok_or_else(|| UcanVerifyError::MalformedToken("missing exp".into()))?;
    let iat = payload["iat"].as_u64().unwrap_or(0);
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| UcanVerifyError::Expired)?
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
            let cap_str = capability_value.as_str().ok_or_else(|| {
                UcanVerifyError::MalformedToken("capability must be string".into())
            })?;
            let level = CapabilityLevel::from_capability_string(cap_str)
                .ok_or_else(|| UcanVerifyError::UnknownCapability(cap_str.into()))?;
            capabilities.insert(space_id.to_string(), level);
        }
    }

    // Parse proofs: JSON array of embedded token strings.
    let proofs = match payload.get("prf") {
        None => Vec::new(),
        Some(v) => v
            .as_array()
            .ok_or_else(|| UcanVerifyError::MalformedToken("prf must be an array".into()))?
            .iter()
            .map(|el| {
                el.as_str()
                    .map(|s| s.to_string())
                    .ok_or_else(|| UcanVerifyError::MalformedToken("prf entry not a string".into()))
            })
            .collect::<Result<Vec<_>, _>>()?,
    };

    Ok(ParsedUcan {
        iss: issuer.to_string(),
        aud: audience,
        exp,
        iat,
        capabilities,
        proofs,
    })
}

// ---------------------------------------------------------------------------
// prf chain walker
// ---------------------------------------------------------------------------

/// Walk the `prf` chain from `leaf` to its self-signed root, enforcing every
/// edge invariant, and return the parsed root token.
///
/// Rules enforced on every parent → child edge:
///
/// 1. **Parent signature + expiry** — via [`parse_ucan`] on the proof string.
///    A tampered or expired ancestor short-circuits here with
///    [`UcanVerifyError::Signature`] / [`UcanVerifyError::Expired`].
/// 2. **Chain continuity** — `parent.aud == child.iss`. Without this the
///    "chain" is a set of unrelated tokens; violation is [`ChainBroken`].
/// 3. **Space alignment** — `expected_space_id` must appear in both parent's
///    and child's capability map. Violation is [`WrongSpace`]: the chain
///    drifted to (or through) a different space.
/// 4. **Attenuation lattice** — `parent_cap.allows(child_cap)`. A child can
///    request the same or a strictly weaker capability than its parent;
///    the reverse is [`CapabilityEscalation`].
///
/// Root termination: the current token is a valid self-signed Admin root of
/// `expected_space_id` when `iss == aud`, its cap for `expected_space_id` is
/// `Admin`, and `proofs.is_empty()`. Anything else with `proofs.is_empty()`
/// is [`RootNotSelfSigned`].
///
/// Depth guard: `max_depth` is the maximum number of **tokens** (nodes) the
/// walker will visit before bailing with [`ChainTooDeep`] — so
/// `max_depth = 5` accepts chains of one to five tokens (root only, up to
/// four intermediate delegations). A chain requiring a sixth token is
/// rejected before the parent is parsed.
fn walk_prf_chain(
    leaf: &ParsedUcan,
    expected_space_id: &str,
    max_depth: usize,
) -> Result<ParsedUcan, UcanVerifyError> {
    let mut current = leaf.clone();
    // Number of tokens already inspected — the leaf itself is node 1.
    let mut visited_nodes: usize = 1;

    loop {
        let current_cap = current
            .capabilities
            .get(expected_space_id)
            .copied()
            .ok_or(UcanVerifyError::WrongSpace)?;

        let is_self_signed_admin_root = current.iss == current.aud
            && current_cap == CapabilityLevel::Admin
            && current.proofs.is_empty();
        if is_self_signed_admin_root {
            return Ok(current);
        }

        if current.proofs.is_empty() {
            // No further ancestry but this token is not a proper root.
            return Err(UcanVerifyError::RootNotSelfSigned);
        }

        if visited_nodes >= max_depth {
            // Report the chain length we would have needed to walk — one more
            // token than `max_depth` allows — so the error surfaces the
            // actual chain length we refused, not the configured limit.
            return Err(UcanVerifyError::ChainTooDeep(visited_nodes + 1));
        }
        visited_nodes += 1;

        // UCAN 0.10 allows multiple proofs. Phase 2 uses first-proof only —
        // multi-proof fan-out is a follow-up (Task-4+).
        let parent_token = &current.proofs[0];
        let parent = parse_ucan(parent_token)?;

        // Chain continuity.
        if parent.aud != current.iss {
            return Err(UcanVerifyError::ChainBroken);
        }

        // Space alignment + attenuation.
        let parent_cap = parent
            .capabilities
            .get(expected_space_id)
            .copied()
            .ok_or(UcanVerifyError::WrongSpace)?;
        if !parent_cap.allows(&current_cap) {
            return Err(UcanVerifyError::CapabilityEscalation);
        }

        current = parent;
    }
}

// ---------------------------------------------------------------------------
// Layer 1: full validation pipeline (parse + audience + cap + chain + binding)
// ---------------------------------------------------------------------------

/// Full UCAN authorisation for a single leaf token targeting one space.
///
/// Runs in order:
///
/// 1. [`parse_ucan`] — decodes JWT, verifies signature, enforces `exp`.
/// 2. Audience match — leaf `aud` must equal `expected_audience`.
///    Enforces replay-protection against tokens issued to another peer.
/// 3. Capability floor — leaf capability for `expected_space_id` must be
///    `>=` `capability_needed` under the lattice.
/// 4. [`walk_prf_chain`] — traverse `prf` up to a self-signed Admin root,
///    enforcing signature/expiry/continuity/attenuation on every edge.
/// 5. [`crate::ucan::space_id::verify_space_id_binding`] — the self-certifying
///    `space_id` must bind to the resolved root DID; this closes the
///    "any signed leaf is trustable" loophole.
///
/// This is the single entry point for gate-facing authorisation. Callers that
/// need to route by inspecting the leaf's multi-space capability map before
/// they know `expected_space_id` should use [`parse_ucan`] for the peek and
/// then call this function once the target space is decided.
pub fn validate_token(
    token: &str,
    expected_space_id: &str,
    expected_audience: &str,
    capability_needed: CapabilityLevel,
    max_chain_depth: usize,
) -> Result<ValidatedUcan, UcanVerifyError> {
    let parsed = parse_ucan(token)?;

    // Audience — leaf token was issued to this peer.
    if expected_audience.is_empty() {
        return Err(UcanVerifyError::EmptyExpectedAudience);
    }
    if parsed.aud != expected_audience {
        return Err(UcanVerifyError::AudienceMismatch {
            expected: expected_audience.to_string(),
            actual: parsed.aud.clone(),
        });
    }

    // Capability floor — leaf grants at least what the operation needs.
    let leaf_cap = parsed
        .capabilities
        .get(expected_space_id)
        .copied()
        .ok_or_else(|| UcanVerifyError::MissingCapability {
            space_id: expected_space_id.to_string(),
        })?;
    if !leaf_cap.allows(&capability_needed) {
        return Err(UcanVerifyError::InsufficientCapability {
            required: capability_needed,
            actual: leaf_cap,
        });
    }

    // Chain walk to a self-signed Admin root.
    let root = walk_prf_chain(&parsed, expected_space_id, max_chain_depth)?;

    // Root DID must bind to the self-certifying space_id.
    super::space_id::verify_space_id_binding(expected_space_id, &root.iss).map_err(
        |e| match e {
            super::space_id::VerifyError::Malformed(_) => UcanVerifyError::RootBindingMalformed,
            super::space_id::VerifyError::Mismatch { .. } => UcanVerifyError::RootBindingMismatch,
        },
    )?;

    Ok(ValidatedUcan {
        issuer: parsed.iss,
        audience: parsed.aud,
        capabilities: parsed.capabilities,
        expires_at: parsed.exp,
        root_did: root.iss,
    })
}

// ---------------------------------------------------------------------------
// Layer 1.5: audience check (replay protection) — used by cached-UCAN paths
// ---------------------------------------------------------------------------

/// Check that a cached [`ValidatedUcan`]'s `aud` field matches the expected
/// recipient DID.
///
/// Without this check, a UCAN that was issued for peer X can be replayed
/// against peer Y by anyone who obtains the token: Y validates the signature
/// (issuer is legitimate), finds matching capabilities, and grants access —
/// but Y was never the intended recipient.
///
/// [`validate_token`] enforces this internally at Announce time. This helper
/// is re-used by [`crate::space_delivery::local::auth_gate`] on every
/// subsequent request that pulls the UCAN from the connection cache — the
/// stored `aud` still has to match the connection's verified DID.
pub fn require_audience(
    validated: &ValidatedUcan,
    expected_audience: &str,
) -> Result<(), UcanVerifyError> {
    if expected_audience.is_empty() {
        return Err(UcanVerifyError::EmptyExpectedAudience);
    }
    if validated.audience == expected_audience {
        Ok(())
    } else {
        Err(UcanVerifyError::AudienceMismatch {
            expected: expected_audience.to_string(),
            actual: validated.audience.clone(),
        })
    }
}

// ---------------------------------------------------------------------------
// Layer 1.75: cached-UCAN expiry re-check
// ---------------------------------------------------------------------------

/// Re-check that a previously validated UCAN has not expired since it was
/// cached.
///
/// [`validate_token`] enforces `exp` at decode time, but the cached
/// [`ValidatedUcan`] stays in `ConnectedPeer::validated_ucan` for the lifetime
/// of the QUIC connection. A long-lived session can therefore outlast its
/// own UCAN. Callers that hold a cached `ValidatedUcan` should call this
/// before trusting it.
///
/// Clock-skew semantics match `validate_token`: a system clock implausibly
/// before UNIX epoch fails closed (`Err(Expired)`), never silently treated
/// as `now = 0`.
pub fn require_not_expired(validated: &ValidatedUcan) -> Result<(), UcanVerifyError> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| UcanVerifyError::Expired)?
        .as_secs();
    if now >= validated.expires_at {
        return Err(UcanVerifyError::Expired);
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Layer 2: capability check — used by cached-UCAN paths
// ---------------------------------------------------------------------------

/// Check that a validated UCAN grants at least the required capability for a space.
///
/// [`validate_token`] enforces this internally for the target space at
/// Announce time. This helper is re-used by
/// [`crate::space_delivery::local::auth_gate`] on cached UCANs when the
/// per-request required level may differ from the level checked at Announce.
pub fn require_capability(
    validated: &ValidatedUcan,
    space_id: &str,
    required: CapabilityLevel,
) -> Result<(), UcanVerifyError> {
    let actual =
        validated
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

/// Encode an Ed25519 `VerifyingKey` as a `did:key:z6Mk...` DID.
///
/// Inverse of [`public_key_from_did`]; same `0xed01` multicodec + base58btc
/// encoding the rest of the codebase uses for identity DIDs.
pub fn did_key_from_public_key(verifying_key: &VerifyingKey) -> String {
    let mut bytes = Vec::with_capacity(34);
    bytes.extend_from_slice(&ED25519_MULTICODEC);
    bytes.extend_from_slice(verifying_key.as_bytes());
    format!("did:key:z{}", bs58::encode(bytes).into_string())
}

/// Extract an Ed25519 `VerifyingKey` from a `did:key:z6Mk...` DID.
///
/// Format: `did:key:z` + base58btc( 0xed01 + 32-byte-pubkey )
pub fn public_key_from_did(did: &str) -> Result<VerifyingKey, UcanVerifyError> {
    if did.len() > MAX_DID_KEY_LEN_BYTES {
        return Err(UcanVerifyError::MalformedToken(format!(
            "did:key too long: {} bytes (max {MAX_DID_KEY_LEN_BYTES})",
            did.len()
        )));
    }

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
mod tests;

#[cfg(test)]
mod chain_tests;
