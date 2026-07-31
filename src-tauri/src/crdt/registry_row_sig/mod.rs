//! Canonical signing and verification of `haex_shared_space_sync` registry
//! rows.
//!
//! A registry row is an atomic, immutable claim ("who owns this share
//! entry") — see migration `0014_registry_authorization_schema.sql`. See
//! [`payload`] for the canonical preimage builder.

pub mod payload;

pub use payload::{RegistryRowSigPayload, DOMAIN_TAG};

#[cfg(test)]
mod tests;
