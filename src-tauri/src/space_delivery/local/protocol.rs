//! Space delivery protocol types over QUIC streams.
//!
//! Request/response protocol for MLS delivery and CRDT sync in local spaces.

/// ALPN protocol identifier for space delivery
pub const ALPN: &[u8] = b"haex-delivery/1";
