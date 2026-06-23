//! Tests for the browser bridge authorization layer.
//!
//! These tests verify that:
//! 1. Only authorized clients can access extensions
//! 2. Unauthorized clients are rejected
//! 3. Clients can only access their authorized extension
//! 4. Authorization can be revoked
//! 5. Extension targeting via publicKey + name works correctly
//! 6. Multi-extension authorization per client works

#![cfg(test)]

mod envelope;
mod protocol;
mod ready_signaling;
mod requested_extension;
mod sql_queries;
