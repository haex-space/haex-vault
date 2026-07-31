//! Canonical signing and verification of `haex_shared_space_sync` registry
//! rows.
//!
//! A registry row is an atomic, immutable claim ("who owns this share
//! entry") — see migration `0014_registry_authorization_schema.sql`.
//! [`payload`] builds the canonical preimage; [`sign`] and [`verify`] provide
//! the Ed25519 helpers used by the authoring extension/device and the
//! puller.

pub mod payload;
pub mod sign;
pub mod verify;

#[cfg(test)]
mod tests;
