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
/// Returns `false` for malformed input (never panics) so callers can treat
/// verification as a pure predicate.
pub fn verify_space_id_binding(space_id: &str, root_did: &str) -> bool {
    let bytes = match bs58::decode(space_id).into_vec() {
        Ok(b) if b.len() == SPACE_ID_BYTES_LEN => b,
        _ => return false,
    };
    let mut nonce = [0u8; NONCE_LEN];
    nonce.copy_from_slice(&bytes[..NONCE_LEN]);
    let expected = compute_hash_part(&nonce, root_did);
    let claimed = &bytes[NONCE_LEN..];
    let mut diff: u8 = 0;
    for i in 0..HASH_LEN {
        diff |= claimed[i] ^ expected[i];
    }
    diff == 0
}

#[cfg(test)]
mod tests;
