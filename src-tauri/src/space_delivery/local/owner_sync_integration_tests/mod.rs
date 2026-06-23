//! Real-QUIC capstone for serverless owner-vault P2P sync.
//!
//! Drives the security-critical owner-sync path end to end over a REAL iroh
//! QUIC connection — no mocks, no in-memory channels:
//!
//! - A (accept side) runs the genuine
//!   [`quic_did_auth::challenge_and_verify`] handshake, the genuine
//!   [`owner_sync::scope::owner_route_decision`] gate (fed by the genuine
//!   [`resolve_vault_owner_did`] / [`resolve_vault_space_id`] resolvers), and
//!   for an owner peer the genuine [`owner_serve::handle_owner_pull`].
//! - B / C (client side) use the genuine [`PeerSession::connect_owner`] +
//!   [`PeerSession::pull_changes`], and B applies the pulled changes through
//!   the genuine [`apply_remote_changes_to_db`].
//!
//! Only the *thin* accept-loop glue is reconstructed here. It mirrors the
//! owner branch of [`multi_leader::handle_stream`] (the owner pre-check +
//! `send_response(&handle_owner_sync_request(...))`). That production glue is
//! itself unit-tested (`owner_serve_tests.rs`, `scope.rs` tests) and
//! end-to-end covered by haex-e2e-tests; reconstructing it is the only way to
//! exercise the real pull/auth/gate trio without a `tauri::AppHandle<Wry>`,
//! which cannot be built in a headless `cargo test`.
//!
//! ## AppHandle boundary (intentionally OUT of scope here)
//!
//! `start_peer_sync_loop` (the orchestration) and
//! `owner_serve::handle_owner_push` are `AppHandle`-bound — the push path
//! advances the HLC clock via `AppState::lock_or_fail`, which needs a live
//! Tauri app. Those are deliberately NOT exercised by this test; they are
//! covered by the e2e tests in haex-e2e-tests. This capstone covers the PULL
//! direction (the one that takes `&DbConnection` and no `AppHandle`) plus the
//! full-vault-vs-foreign routing decision, which is the load-bearing security
//! assertion for serverless owner-vault sync.

#![cfg(test)]

mod foreign_peer;
mod helpers;
mod pull;
