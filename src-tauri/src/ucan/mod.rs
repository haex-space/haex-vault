//! UCAN token utilities — shared across peer_storage, space_delivery, and file_sync.
//!
//! Two-stage verification (see [`verify`] mod docs for full rationale):
//! 1. [`parse_ucan`] — structure + Ed25519 signature + `exp`. Callers use this
//!    when the target `space_id` is only known after inspecting the leaf's
//!    capability map (multi-space routing in `peer_storage::handlers::dispatch`).
//! 2. [`validate_token`] — full pipeline: parse + audience + capability +
//!    prf-chain walk to a self-signed root + self-certifying `space_id` binding.

pub mod capability_set;
pub mod commands;
pub mod config;
mod create;
pub mod predicate;
pub mod row_capability;
pub mod space_id;
pub mod verify;

pub use commands::{verify_ucan_chain_batch, VerifyChainRequest, VerifyChainResult, VerifyOutcome};
pub use config::{
    read_max_ucan_chain_depth, MAX_UCAN_CHAIN_DEPTH_DEFAULT, MAX_UCAN_CHAIN_DEPTH_KEY,
    MAX_UCAN_CHAIN_DEPTH_MAX, MAX_UCAN_CHAIN_DEPTH_MIN,
};
pub use create::{create_delegated_ucan, signing_key_from_pkcs8_base64, UcanCreateError};
pub use space_id::VerifyError as SpaceIdVerifyError;
pub use verify::{
    did_key_from_public_key, parse_ucan, public_key_from_did, require_audience, require_capability,
    require_not_expired, validate_token, CapabilityLevel, ParsedUcan, UcanVerifyError,
    ValidatedUcan,
};
