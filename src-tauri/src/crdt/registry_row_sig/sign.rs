use super::payload::RegistryRowSigPayload;
use ed25519_dalek::{Signer, SigningKey};

/// Sign the canonical encoding of `payload` with `sk`, returning the raw
/// 64-byte Ed25519 signature.
pub fn sign_registry_row(payload: &RegistryRowSigPayload, sk: &SigningKey) -> Vec<u8> {
    let msg = payload.canonical_encoding();
    sk.sign(&msg).to_bytes().to_vec()
}
