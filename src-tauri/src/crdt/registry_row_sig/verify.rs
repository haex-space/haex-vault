use super::payload::RegistryRowSigPayload;
use ed25519_dalek::{Signature, VerifyingKey};

/// Verify `sig` against the canonical encoding of `payload` under `pk`.
///
/// Uses `verify_strict`, not `verify`: the permissive RFC-8032 path accepts
/// small-order (weak) public keys and small-order signature `R` components,
/// which allow forging a signature that validates against almost any
/// message. This signature IS the authorship binding for a registry row, so
/// a weak-key forgery is a direct impersonation. Strict verification rejects
/// both cases — see `column_sig::verify::verify_column_sig` for the same
/// rationale.
pub fn verify_registry_row(payload: &RegistryRowSigPayload, sig: &[u8], pk: &VerifyingKey) -> bool {
    let msg = payload.canonical_encoding();
    let sig_arr: [u8; 64] = match sig.try_into() {
        Ok(a) => a,
        Err(_) => return false,
    };
    let sig = Signature::from_bytes(&sig_arr);
    pk.verify_strict(&msg, &sig).is_ok()
}
