//! Self-certifying `space_id` — binds the id to the DID of the Space-Root
//! (issuer of the `space/admin` root UCAN). See ADR 0002 §6.1.
//!
//! Layout: `nonce (16 B) ‖ sha256_16(domain_tag ‖ nonce ‖ root_did_utf8)`.
//! The 32-byte binary is base58btc-encoded (Bitcoin alphabet, ~44 chars).
//!
//! The TS counterpart in `src/utils/auth/spaceId.ts` MUST produce byte-identical
//! output; the shared fixture `src-tauri/tests/fixtures/space_id_vectors.json`
//! guards against drift.

use sha2::{Digest, Sha256};

/// Domain-separation tag prefixed to every hash preimage.
pub const DOMAIN_TAG: &str = "haex/space-id/v1";

/// Length of the random nonce embedded in every `space_id`.
pub const NONCE_LEN: usize = 16;

/// Length of the truncated SHA-256 hash tail.
pub const HASH_LEN: usize = 16;

/// Total decoded length of a `space_id` (nonce ‖ hash).
pub const SPACE_ID_BYTES_LEN: usize = NONCE_LEN + HASH_LEN;

/// Upper bound on the base58-encoded `space_id` character length accepted
/// by [`verify_space_id_binding`].
///
/// A well-formed 32-byte binary encodes to ~44 base58btc chars; 128 is a
/// generous safety margin. Guarding this before calling `bs58::decode` is a
/// DoS mitigation: `bs58::decode` allocates a `Vec` whose size grows with
/// input length, so unbounded input from a network-facing caller (Tauri
/// commands, UCAN chain walkers) could be used to force large allocations.
pub const MAX_SPACE_ID_LEN_CHARS: usize = 128;

/// Distinct failure modes returned by [`verify_space_id_binding`].
///
/// Split from a plain `bool` so callers (notably the UCAN chain walker) can
/// distinguish "the caller gave us garbage" (`Malformed`) from "the input is
/// well-formed but does not bind to this DID" (`Mismatch`) without inspecting
/// error messages.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum VerifyError {
    /// The `space_id` string is not a base58 encoding of the expected
    /// `NONCE_LEN + HASH_LEN` bytes, or exceeded the length guard.
    #[error("space_id malformed: {0}")]
    Malformed(String),
    /// The `space_id` is well-formed but the embedded hash does not match
    /// `H(domain_tag ‖ nonce ‖ root_did)` — i.e. it was not derived from
    /// this `root_did`.
    #[error("space_id does not bind to root_did {root_did}")]
    Mismatch { root_did: String },
}

fn compute_hash_part(nonce: &[u8; NONCE_LEN], root_did: &str) -> [u8; HASH_LEN] {
    let mut hasher = Sha256::new();
    hasher.update(DOMAIN_TAG.as_bytes());
    hasher.update(nonce);
    hasher.update(root_did.as_bytes());
    let full = hasher.finalize();
    let mut out = [0u8; HASH_LEN];
    out.copy_from_slice(&full[..HASH_LEN]);
    out
}

/// Derive a self-certifying `space_id` binding `root_did` with `nonce`. The
/// nonce is embedded verbatim so the binding is later verifiable from
/// `(space_id, root_did)` alone.
pub fn derive_space_id(root_did: &str, nonce: &[u8; NONCE_LEN]) -> String {
    let hash = compute_hash_part(nonce, root_did);
    let mut buf = [0u8; SPACE_ID_BYTES_LEN];
    buf[..NONCE_LEN].copy_from_slice(nonce);
    buf[NONCE_LEN..].copy_from_slice(&hash);
    bs58::encode(buf).into_string()
}

/// Verify that `space_id` is the self-certifying binding of `root_did`.
///
/// Returns `Ok(())` iff the encoded nonce ‖ hash decodes cleanly *and* the
/// hash matches `H(domain_tag ‖ nonce ‖ root_did)`. Never panics.
///
/// Distinguishes:
/// - [`VerifyError::Malformed`] — the input is not a valid encoded
///   `SPACE_ID_BYTES_LEN`-byte payload (bad base58, wrong length, or exceeds
///   the DoS length guard).
/// - [`VerifyError::Mismatch`] — the input is well-formed but was not derived
///   from `root_did`.
pub fn verify_space_id_binding(space_id: &str, root_did: &str) -> Result<(), VerifyError> {
    // DoS guard: `bs58::decode` allocates a Vec sized to the input length.
    // Unbounded input from a network-facing caller could force large
    // allocations; the well-formed encoding is ~44 chars, so 128 is a
    // generous margin that still fits any legitimate space_id.
    if space_id.len() > MAX_SPACE_ID_LEN_CHARS {
        return Err(VerifyError::Malformed(format!(
            "space_id too long: {} chars (max {})",
            space_id.len(),
            MAX_SPACE_ID_LEN_CHARS
        )));
    }

    let bytes = bs58::decode(space_id)
        .into_vec()
        .map_err(|e| VerifyError::Malformed(format!("bs58 decode: {e}")))?;
    if bytes.len() != SPACE_ID_BYTES_LEN {
        return Err(VerifyError::Malformed(format!(
            "decoded length {}, expected {}",
            bytes.len(),
            SPACE_ID_BYTES_LEN
        )));
    }

    let nonce: &[u8; NONCE_LEN] = bytes[..NONCE_LEN].try_into().expect("length checked above");
    let expected = compute_hash_part(nonce, root_did);
    let claimed = &bytes[NONCE_LEN..];
    let mut diff: u8 = 0;
    for i in 0..HASH_LEN {
        diff |= claimed[i] ^ expected[i];
    }
    if diff == 0 {
        Ok(())
    } else {
        Err(VerifyError::Mismatch {
            root_did: root_did.to_string(),
        })
    }
}

#[cfg(test)]
mod tests;
