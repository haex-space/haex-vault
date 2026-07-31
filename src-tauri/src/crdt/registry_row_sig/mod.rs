//! Canonical signing and verification of `haex_shared_space_sync` registry
//! rows.
//!
//! A registry row is an atomic, immutable claim ("who owns this share
//! entry") — see migration `0014_registry_authorization_schema.sql`. See
//! [`payload`] for the canonical preimage builder and [`sign`] for the
//! Ed25519 signing helper.

pub mod payload;
pub mod sign;

pub use payload::{RegistryRowSigPayload, DOMAIN_TAG};
pub use sign::sign_registry_row;

#[cfg(test)]
mod tests;
