//! Proof-of-Possession binding the MLS signature key to the identity key.
//!
//! A KeyPackage carries only a self-asserted DID string in its BasicCredential
//! (`manager.rs::init_identity`). The MLS signature key is generated
//! independently, so anyone can mint a KeyPackage whose credential names any
//! DID they like. To defeat that, the *identity* signing key signs the newly
//! generated MLS signature public key together with the DID; verifiers resolve
//! the identity key from the DID (`did:key`) and check the signature. Without
//! this proof, the credential-DID string check in `manager::add_member` can
//! still be bypassed by minting a KeyPackage with someone else's DID string.

use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};

/// Domain separator so a signature made here cannot be confused with a
/// signature made over any other payload signed by the same identity key.
const DOMAIN_TAG: &[u8] = b"haex-mls-pop-v1";

/// Build the exact byte sequence the identity key signs. Concatenation is
/// unambiguous here because DOMAIN_TAG is fixed-length and `mls_sig_pub`
/// is fixed-width (32 bytes for Ed25519). Length-prefixing is unnecessary.
fn pop_message(mls_sig_pub: &[u8], did: &str) -> Vec<u8> {
    let mut m = Vec::with_capacity(DOMAIN_TAG.len() + mls_sig_pub.len() + did.len());
    m.extend_from_slice(DOMAIN_TAG);
    m.extend_from_slice(mls_sig_pub);
    m.extend_from_slice(did.as_bytes());
    m
}

/// Produce a proof-of-possession: the identity signing key attests that the
/// MLS signature public key belongs to the DID's owner.
pub fn sign_pop(identity: &SigningKey, mls_sig_pub: &[u8], did: &str) -> Signature {
    identity.sign(&pop_message(mls_sig_pub, did))
}

/// Verify a proof-of-possession against an identity verifying key resolved
/// from the DID (e.g. via did:key decode).
pub fn verify_pop(
    identity_pub: &VerifyingKey,
    mls_sig_pub: &[u8],
    did: &str,
    sig: &Signature,
) -> Result<(), ed25519_dalek::SignatureError> {
    identity_pub.verify(&pop_message(mls_sig_pub, did), sig)
}

#[cfg(test)]
#[path = "pop_tests.rs"]
mod tests;
