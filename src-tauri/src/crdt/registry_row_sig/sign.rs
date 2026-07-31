//! Sign a canonical registry-row payload with Ed25519.
//!
//! Returns typed [`Signature`], not raw bytes — no allocation beyond the
//! canonical encoding itself, and no ambiguity about the byte length at the
//! call site. Serialize to bytes (`.to_bytes()`) at the DB/wire boundary
//! only. See `verify.rs` for the corresponding verification path.

use super::payload::RegistryRowSigPayload;
use ed25519_dalek::{Signature, Signer, SigningKey};

/// Sign the canonical encoding of `payload` with `sk`.
pub fn sign_registry_row(payload: &RegistryRowSigPayload, sk: &SigningKey) -> Signature {
    sk.sign(&payload.canonical_encoding())
}
