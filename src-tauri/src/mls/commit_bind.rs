//! Binds a committer's presented UCAN capability proof to one specific MLS
//! commit, so a captured proof cannot be replayed against a different
//! commit (plan `2026-08-13-mls-receive-gate-ucan-on-commit.md` §1/§4.4).
//!
//! Mirrors the shape of [`crate::mls::pop`]: a domain-separated message is
//! signed with the committer's identity key (the same key `did:key`
//! resolves to), and receivers verify it against the identity key resolved
//! from the presented UCAN's `audience_did`.

use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use sha2::{Digest, Sha256};

/// Domain separator versioning this binding scheme, mirroring
/// [`crate::mls::pop::HAEX_POP_EXTENSION_TYPE`]'s sibling tag
/// `"haex-mls-pop-v1"`.
const DOMAIN_TAG: &[u8] = b"haex-mls-commit-bind-v1";

/// `sha256(DOMAIN_TAG || sha256(commit_bytes))` — the digest actually
/// signed. Hashing the commit first keeps the signed payload fixed-size
/// regardless of commit length; the outer hash-with-domain-tag prevents a
/// signature made here from being confused with a signature made over the
/// raw commit hash for an unrelated purpose.
fn bind_digest(commit_bytes: &[u8]) -> [u8; 32] {
    let commit_hash = Sha256::digest(commit_bytes);
    let mut bind_input = Vec::with_capacity(DOMAIN_TAG.len() + commit_hash.len());
    bind_input.extend_from_slice(DOMAIN_TAG);
    bind_input.extend_from_slice(&commit_hash);
    Sha256::digest(&bind_input).into()
}

/// Sign the commit-bind digest for `commit_bytes` with the committer's
/// identity key.
pub fn sign_commit_bind(identity: &SigningKey, commit_bytes: &[u8]) -> Signature {
    identity.sign(&bind_digest(commit_bytes))
}

/// Verify a commit-bind signature against the committer's identity
/// verifying key and the exact commit bytes it must have been made over.
pub fn verify_commit_bind(
    identity_pub: &VerifyingKey,
    commit_bytes: &[u8],
    sig: &Signature,
) -> Result<(), ed25519_dalek::SignatureError> {
    identity_pub.verify(&bind_digest(commit_bytes), sig)
}

/// Convenience wrapper for callers holding raw wire bytes rather than typed
/// `did:key` / `Signature` values: resolves the identity key from
/// `audience_did` and parses `sig_bytes` before delegating to
/// [`verify_commit_bind`]. Used by [`crate::mls::manager::MlsManager::decrypt`]
/// on the receive path, where the presented capability's `audience_did` and
/// the wire-carried signature bytes are the only inputs available.
pub fn verify_commit_bind_bytes(
    audience_did: &str,
    commit_bytes: &[u8],
    sig_bytes: &[u8],
) -> Result<(), String> {
    let identity_pub = crate::ucan::public_key_from_did(audience_did)
        .map_err(|e| format!("cannot resolve identity key from DID {audience_did}: {e}"))?;
    let sig = Signature::try_from(sig_bytes)
        .map_err(|e| format!("malformed commit-bind signature bytes: {e}"))?;
    verify_commit_bind(&identity_pub, commit_bytes, &sig)
        .map_err(|e| format!("commit-bind signature does not verify: {e}"))
}

#[cfg(test)]
#[path = "commit_bind_tests.rs"]
mod tests;
