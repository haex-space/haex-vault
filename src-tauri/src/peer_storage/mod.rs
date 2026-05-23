//! Peer Storage — P2P file sharing via iroh/QUIC
//!
//! Enables users to share local folders with authorized peers over the internet.
//! Uses iroh for NAT traversal and QUIC transport with E2E encryption.

mod android;
pub(crate) mod client;
pub mod commands;
pub mod endpoint;
pub mod error;
mod handlers;
mod helpers;
pub mod protocol;
pub(crate) mod streaming;
#[cfg(test)]
mod tests;

pub use commands::*;
