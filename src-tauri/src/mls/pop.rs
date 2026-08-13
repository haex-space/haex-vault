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
use openmls::extensions::{Extension, UnknownExtension};

/// Domain separator so a signature made here cannot be confused with a
/// signature made over any other payload signed by the same identity key.
const DOMAIN_TAG: &[u8] = b"haex-mls-pop-v1";

/// Length of an Ed25519 public key. The MLS ciphersuite used here is
/// Ed25519-based, so the MLS signature public key is always 32 bytes.
const ED25519_PUBLIC_KEY_LEN: usize = 32;

/// Extension type carrying the PoP inside a KeyPackage's leaf node.
///
/// Chosen from the IANA "MLS Extension Types" registry's private-use range
/// (`0xF000-0xFFFF`) — no coordination required with the IETF-managed
/// short values, no clash with GREASE (which uses the `0xNANA` pattern,
/// never `0xF001`). Every KeyPackage the vault produces carries this
/// extension; every receiver checking an Add proposal or an external-commit
/// leaf demands its presence and verifies the signature against the
/// credential DID.
///
/// This is the wire-level identifier — changing it is a breaking
/// KeyPackage-format change. There are no production users today so a
/// clean bump is fine when needed, but do not shift the value under
/// mixed-fleet code.
pub const HAEX_POP_EXTENSION_TYPE: u16 = 0xF001;

/// Build the exact byte sequence the identity key signs. Concatenation is
/// unambiguous here because DOMAIN_TAG is fixed-length and `mls_sig_pub` is
/// a fixed-width 32-byte array (enforced by the type). Length-prefixing is
/// unnecessary.
fn pop_message(mls_sig_pub: &[u8; ED25519_PUBLIC_KEY_LEN], did: &str) -> Vec<u8> {
    let mut m = Vec::with_capacity(DOMAIN_TAG.len() + mls_sig_pub.len() + did.len());
    m.extend_from_slice(DOMAIN_TAG);
    m.extend_from_slice(mls_sig_pub);
    m.extend_from_slice(did.as_bytes());
    m
}

/// Produce a proof-of-possession: the identity signing key attests that the
/// MLS signature public key belongs to the DID's owner. `mls_sig_pub` is the
/// 32-byte Ed25519 signature public key; callers holding a variable-length
/// slice must convert (e.g. `.try_into()`) so a wrong length is caught before
/// it reaches the signature payload.
pub fn sign_pop(
    identity: &SigningKey,
    mls_sig_pub: &[u8; ED25519_PUBLIC_KEY_LEN],
    did: &str,
) -> Signature {
    identity.sign(&pop_message(mls_sig_pub, did))
}

/// Verify a proof-of-possession against an identity verifying key resolved
/// from the DID (e.g. via did:key decode). See [`sign_pop`] for the
/// `mls_sig_pub` length contract.
pub fn verify_pop(
    identity_pub: &VerifyingKey,
    mls_sig_pub: &[u8; ED25519_PUBLIC_KEY_LEN],
    did: &str,
    sig: &Signature,
) -> Result<(), ed25519_dalek::SignatureError> {
    identity_pub.verify(&pop_message(mls_sig_pub, did), sig)
}

/// Build the leaf-node extension carrying `sig` under
/// [`HAEX_POP_EXTENSION_TYPE`]. The payload is the raw 64-byte Ed25519
/// signature — no length prefix or envelope; the extension type id AND the
/// domain tag inside [`sign_pop`] already scope the bytes.
pub fn pop_leaf_extension(sig: &Signature) -> Extension {
    Extension::Unknown(
        HAEX_POP_EXTENSION_TYPE,
        UnknownExtension(sig.to_bytes().to_vec()),
    )
}

/// Extract the PoP signature bytes from a leaf's extension list. Returns
/// `None` if the extension is absent OR if the payload is not the expected
/// 64-byte Ed25519 signature. Callers treat `None` as reject.
pub fn extract_pop_from_leaf(leaf: &openmls::prelude::LeafNode) -> Option<Signature> {
    for ext in leaf.extensions().iter() {
        if let Extension::Unknown(HAEX_POP_EXTENSION_TYPE, UnknownExtension(bytes)) = ext {
            return Signature::try_from(bytes.as_slice()).ok();
        }
    }
    None
}

#[cfg(test)]
#[path = "pop_tests.rs"]
mod tests;
