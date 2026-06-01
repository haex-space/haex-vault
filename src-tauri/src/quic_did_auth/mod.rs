//! QUIC DID-auth primitive.
//!
//! Provides a reusable challenge–response handshake that cryptographically
//! binds a `did:key:z6Mk…` identity to a QUIC connection. Without this step,
//! UCAN audience checks are theatrical: a peer can present a UCAN issued to a
//! foreign DID over its own iroh transport key. After a successful handshake,
//! the verified DID can be trusted as the peer identity for the lifetime of
//! the connection.
//!
//! Design notes: see `docs/plans/2026-06-01-quic-did-auth-primitiv.md`.

mod server;
mod wire;

pub use server::{challenge_and_verify, ChallengeError, CHALLENGE_TIMEOUT};
pub use wire::{build_sig_input, Challenge, Response, DOMAIN_TAG, NONCE_LEN, PROTOCOL_VERSION};
