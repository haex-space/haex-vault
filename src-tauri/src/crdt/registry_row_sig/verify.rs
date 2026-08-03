use super::payload::RegistryRowSigPayload;
use ed25519_dalek::{Signature, VerifyingKey};

/// Reasons [`verify_registry_row`] can reject a signature. Mirrors
/// `column_sig::verify::VerifyColumnSigError`'s split between a
/// wire/encoding problem and a crypto failure, so callers (the B.4 puller)
/// can tell "signature bytes were corrupted in transit" apart from
/// "signature does not authenticate this payload" for forensics.
#[derive(Debug, PartialEq, Eq)]
pub enum VerifyRegistryRowSigError {
    /// Signature byte-length was not 64.
    MalformedSignatureBytes,
    /// Signature crypto-verify failed (wrong key, tampered payload, or forged sig).
    InvalidSignature,
}

/// Verify `sig` against the canonical encoding of `payload` under `pk`.
///
/// Uses `verify_strict`, not `verify`: the permissive RFC-8032 path accepts
/// small-order (weak) public keys and small-order signature `R` components,
/// which allow forging a signature that validates against almost any
/// message. This signature IS the authorship binding for a registry row, so
/// a weak-key forgery is a direct impersonation. Strict verification rejects
/// both cases — see `column_sig::verify::verify_column_sig` for the same
/// rationale.
pub fn verify_registry_row(
    payload: &RegistryRowSigPayload,
    sig: &[u8],
    pk: &VerifyingKey,
) -> Result<(), VerifyRegistryRowSigError> {
    let sig_arr: [u8; 64] = sig
        .try_into()
        .map_err(|_| VerifyRegistryRowSigError::MalformedSignatureBytes)?;
    let sig = Signature::from_bytes(&sig_arr);
    pk.verify_strict(&payload.canonical_encoding(), &sig)
        .map_err(|_| VerifyRegistryRowSigError::InvalidSignature)
}
