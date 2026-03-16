//! Peer Storage — P2P file sharing via iroh/QUIC
//!
//! Enables users to share local folders with authorized peers over the internet.
//! Uses iroh for NAT traversal and QUIC transport with E2E encryption.

pub mod commands;
pub mod endpoint;
pub mod error;
pub mod protocol;
#[cfg(test)]
mod tests;

pub use commands::*;
