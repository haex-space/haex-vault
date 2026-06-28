//! Full-stack P2P Storage integration tests.
//!
//! Tests the complete protocol: LIST, STAT, READ with proper length-prefixed encoding.
//! Covers: nested directories, file downloads, chunked transfers, path traversal prevention,
//! cross-space isolation, multi-share browsing, concurrent connections.
//!
//! Run: cargo test --test peer_storage_fullstack

// Integration-test file: every fn is a test, `.unwrap()` is idiomatic for
// failing assertions. The crate-level `#![cfg_attr(test, allow(...))]` in
// `src/lib.rs` does not reach here because integration tests are their own
// compilation unit — opt out explicitly.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;
mod concurrent;
mod dos_defence;
mod edge_cases;
mod endpoint_security;
mod list;
mod multi_share;
mod path_traversal;
mod race_conditions;
mod range;
mod read;
mod robustness;
mod security;
mod stat;
mod stress;
