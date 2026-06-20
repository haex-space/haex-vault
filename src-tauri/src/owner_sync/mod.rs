//! Owner-vault P2P sync — serverless sync of the owner's own vault across
//! the owner's own devices over iroh/QUIC.
//!
//! The full vault syncs only to peers proven (via DID-auth) to hold the SAME
//! vault-owner DID. This module holds the small, networking-free decision
//! functions that scope what a given peer is allowed to receive:
//!
//! - [`scope::resolve_vault_owner_did`] — who owns this vault.

pub mod scope;

#[cfg(test)]
mod tests;
