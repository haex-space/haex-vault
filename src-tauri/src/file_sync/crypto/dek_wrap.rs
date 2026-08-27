//! Round F2 — DEK wrap/unwrap under a grant's KEK.
//!
//! A per-object DEK (Data Encryption Key) is generated fresh on upload
//! and sealed inside every sidecar that grants access to the object.
//! The wrap key (KEK) is the grant scope's key: `vault_key` for own-
//! vault sidecars today, the MLS epoch key for space-scoped sidecars in
//! Round F3/F4. This module keeps the wrap layout tiny (72 bytes on
//! disk) and independent of file-content envelope framing — a sidecar
//! is a small JSON blob that carries the wrapped bytes verbatim.
//!
//! ## Wire format
//!
//! ```text
//! [ nonce (24) ][ ciphertext (32) ][ tag (16) ]
//! ```
//!
//! `WRAPPED_DEK_LEN` is the constant total (72 bytes). No version byte,
//! no magic — the wrapper only ever lives inside a `SidecarPayload`
//! whose enclosing envelope already carries the version discriminator.
//! Wraps use a fresh random 24-byte XChaCha20 nonce so two wraps of the
//! same DEK under the same KEK never collide on the wire.

use chacha20poly1305::{
    aead::{Aead, KeyInit},
    XChaCha20Poly1305, XNonce,
};
use zeroize::Zeroizing;

use super::chunk::{CryptoError, TAG_SIZE};
use super::envelope::NONCE_SIZE;

/// Length of a Data Encryption Key: matches the AEAD key size.
pub const DEK_LEN: usize = 32;

/// Fixed on-wire length of a wrapped DEK: nonce (24) + ciphertext (32) + tag (16).
pub const WRAPPED_DEK_LEN: usize = NONCE_SIZE + DEK_LEN + TAG_SIZE;

/// Errors from DEK wrap/unwrap. Distinguishes malformed-input (caller
/// error) from AEAD failure (wrong KEK or tampered wrapper) so callers
/// can log the two cases separately.
#[derive(Debug, thiserror::Error)]
pub enum DekWrapError {
    #[error(transparent)]
    Crypto(#[from] CryptoError),
    #[error("malformed wrapped DEK: expected {expected} bytes, got {got}")]
    MalformedWrapper { expected: usize, got: usize },
}

/// Seal `dek` under `kek` with a fresh random nonce. Output is exactly
/// [`WRAPPED_DEK_LEN`] bytes. Non-deterministic on purpose — repeated
/// wraps of the same DEK under the same KEK are unlinkable on the wire.
pub fn wrap_dek(kek: &[u8; 32], dek: &[u8; DEK_LEN]) -> Result<Vec<u8>, DekWrapError> {
    let cipher = XChaCha20Poly1305::new(kek.into());
    let mut nonce_bytes = [0u8; NONCE_SIZE];
    rand::fill(&mut nonce_bytes);
    let nonce = XNonce::from(nonce_bytes);
    let ct = cipher
        .encrypt(&nonce, dek.as_ref())
        .map_err(|_| CryptoError::SealFailed)?;
    let mut out = Vec::with_capacity(WRAPPED_DEK_LEN);
    out.extend_from_slice(&nonce_bytes);
    out.extend_from_slice(&ct);
    Ok(out)
}

/// Open a wrapped DEK under `kek`. Returns the DEK inside a
/// [`Zeroizing`] buffer so it scrubs on drop — callers should hand it
/// straight into the AEAD constructor and let the scope end naturally.
pub fn unwrap_dek(
    kek: &[u8; 32],
    wrapped: &[u8],
) -> Result<Zeroizing<[u8; DEK_LEN]>, DekWrapError> {
    if wrapped.len() != WRAPPED_DEK_LEN {
        return Err(DekWrapError::MalformedWrapper {
            expected: WRAPPED_DEK_LEN,
            got: wrapped.len(),
        });
    }
    let (nonce_bytes, ct) = wrapped.split_at(NONCE_SIZE);
    let cipher = XChaCha20Poly1305::new(kek.into());
    let nonce = XNonce::from_slice(nonce_bytes);
    let pt = cipher
        .decrypt(nonce, ct)
        .map_err(|_| CryptoError::OpenFailed)?;
    let mut out = Zeroizing::new([0u8; DEK_LEN]);
    out.copy_from_slice(&pt);
    Ok(out)
}
