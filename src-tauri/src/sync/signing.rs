//! BIP-340 signatures over holzi's own payloads (spec 024, research R10).
//!
//! Every signature covers `SHA-256(domain tag ‖ canonical bytes)`, where the
//! canonical bytes are the postcard encoding of a fixed struct. The domain tag
//! binds a signature to its purpose, so a device-list signature can never be
//! replayed as, say, a key authorization.

use secp256k1::{schnorr, Keypair, SecretKey, XOnlyPublicKey};
use serde::Serialize;
use sha2::{Digest, Sha256};

/// What a signature is for. The tag strings are part of the wire format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Domain {
    DeviceAuth,
    DeviceList,
    KeyGeneration,
    KeyEnvelope,
    Admission,
    Link,
    LinkResume,
    /// A Visitenkarte of the vault (spec 051, contracts/contact-card.md).
    ContactCard,
    /// A device of another vault proving itself on `haex-vault-ext-peer/1` (spec 057 research R4).
    ExtPeerAuth,
    /// A device proving itself on `haex-vault-files/1` (spec 056 research R4).
    FilesAuth,
}

impl Domain {
    pub fn tag(self) -> &'static [u8] {
        match self {
            Domain::DeviceAuth => b"holzi-device-auth/v1",
            Domain::DeviceList => b"holzi-device-list/v1",
            Domain::KeyGeneration => b"holzi-key-generation/v1",
            Domain::KeyEnvelope => b"holzi-key-envelope/v1",
            Domain::Admission => b"holzi-admission/v1",
            Domain::Link => b"holzi-link/v1",
            Domain::LinkResume => b"holzi-link-resume/v1",
            Domain::ContactCard => b"holzi-contact-card/v1",
            Domain::ExtPeerAuth => b"haex-vault-ext-peer-auth/v1",
            Domain::FilesAuth => b"haex-vault-files-auth/v1",
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SigningError {
    #[error("not a valid secp256k1 secret key")]
    InvalidSecretKey,
    #[error("not a valid x-only public key")]
    InvalidPublicKey,
    #[error("the signature does not verify")]
    BadSignature,
    #[error("canonical encoding failed: {0}")]
    Encoding(#[from] postcard::Error),
}

/// The canonical bytes of `value`: its postcard encoding.
pub fn canonical<T: Serialize>(value: &T) -> Result<Vec<u8>, SigningError> {
    Ok(postcard::to_stdvec(value)?)
}

/// The 32-byte digest a signature in `domain` covers.
pub fn digest(domain: Domain, bytes: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(domain.tag());
    hasher.update(bytes);
    hasher.finalize().into()
}

/// Signs `bytes` in `domain` with the secp256k1 secret `secret`.
pub fn sign(domain: Domain, bytes: &[u8], secret: &[u8; 32]) -> Result<[u8; 64], SigningError> {
    let secret =
        SecretKey::from_secret_bytes(*secret).map_err(|_| SigningError::InvalidSecretKey)?;
    let keypair = Keypair::from_secret_key(&secret);
    let aux = crate::sync::keys::random_bytes::<32>();
    Ok(schnorr::sign_with_aux_rand(&digest(domain, bytes), &keypair, &aux).to_byte_array())
}

/// Verifies a signature made by [`sign`] against the x-only public key `pubkey`.
pub fn verify(
    domain: Domain,
    bytes: &[u8],
    signature: &[u8; 64],
    pubkey: &[u8; 32],
) -> Result<(), SigningError> {
    let pubkey =
        XOnlyPublicKey::from_byte_array(*pubkey).map_err(|_| SigningError::InvalidPublicKey)?;
    let signature = schnorr::Signature::from_byte_array(*signature);
    schnorr::verify(&signature, &digest(domain, bytes), &pubkey)
        .map_err(|_| SigningError::BadSignature)
}

/// The x-only public key of the secp256k1 secret `secret`.
pub fn xonly_public_key(secret: &[u8; 32]) -> Result<[u8; 32], SigningError> {
    let secret =
        SecretKey::from_secret_bytes(*secret).map_err(|_| SigningError::InvalidSecretKey)?;
    Ok(secret.x_only_public_key().0.to_byte_array())
}

/// `u32 BE length ‖ bytes`, the length prefix of contracts/sync-protocol.md.
pub fn lp(bytes: &[u8]) -> Vec<u8> {
    let len = u32::try_from(bytes.len()).expect("a length-prefixed field fits in u32");
    let mut out = Vec::with_capacity(4 + bytes.len());
    out.extend_from_slice(&len.to_be_bytes());
    out.extend_from_slice(bytes);
    out
}

#[cfg(test)]
#[path = "signing_tests.rs"]
mod tests;
