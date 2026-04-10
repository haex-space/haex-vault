//! UCAN token utilities — shared across peer_storage, space_delivery, and file_sync.
//!
//! Token creation and two-layer verification:
//! 1. `validate_token` — first line of defense: structure, signature, expiry
//! 2. `require_capability` — source of truth: capability matches the operation

mod create;
mod verify;

pub use create::{create_delegated_ucan, signing_key_from_pkcs8_base64, UcanCreateError};
pub use verify::{
    validate_token, require_capability,
    CapabilityLevel, ValidatedUcan, UcanVerifyError,
};
