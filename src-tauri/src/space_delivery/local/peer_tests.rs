//! Tests for `PeerSession`.
//!
//! Behavioural verification of `connect` / `connect_owner` requires a live
//! `iroh::Endpoint` pair (the leader writes a `Challenge` on connection-accept),
//! which exists only in the Task-8 capstone integration test. The unit tests
//! here are source-level guards plus a pure-construction check of the
//! owner-mode `ucan_token` wiring.

mod read_timeout_tests {
    //! Regression guard: PeerSession::request must bound the response wait.
    //!
    //! Behavioural verification requires a live iroh::Endpoint pair and a
    //! controllable path-degradation, which doesn't exist as a test fixture.
    //! A source-level guard catches accidental removal of the timeout.

    #[test]
    fn request_must_apply_read_timeout() {
        let source = include_str!("peer.rs");

        // Scope the guard to PeerSession::request — a file-wide `contains`
        // would still pass if the timeout appeared elsewhere in peer.rs while
        // `request` itself lost its bound. Same scoping pattern as the
        // `connect_owner_skips_ucan_load_and_announce` test below.
        let request_fn = source
            .split_once("async fn request")
            .map(|(_, rest)| rest)
            .and_then(|rest| rest.split_once("\n    /// "))
            .map(|(body, _)| body)
            .expect("PeerSession::request must exist in peer.rs");

        assert!(
            request_fn.contains("tokio::time::timeout"),
            "PeerSession::request must wrap protocol::read_response in \
             tokio::time::timeout; otherwise a degraded QUIC path blocks \
             read for ~150s until the connection's idle timer fires"
        );
        assert!(
            request_fn.contains("READ_TIMEOUT_SECS"),
            "the bound should reuse READ_TIMEOUT_SECS from quic_retry for \
             consistency with the invite-flow timeout"
        );
    }
}

mod owner_mode_tests {
    //! Owner-mode connect (`connect_owner`) drops UCAN: the owner mesh is
    //! gated by same-owner DID-auth, not UCAN, so owner sessions carry no
    //! UCAN token and must emit `ucan_token: None` on every request.
    //!
    //! Full behavioural coverage (DID-auth still runs, `Announce` skipped,
    //! end-to-end push/pull) is the Task-8 capstone integration test against
    //! a live QUIC endpoint pair. Here we cover the wiring that does not need
    //! a live connection.

    use super::super::PeerSession;
    use crate::space_delivery::local::protocol::Request;

    /// An owner-mode session holds `ucan_token: None`, so the requests it
    /// builds must carry `ucan_token: None` — never an empty `Some("")` that a
    /// receiver would try (and fail) to validate. The regular space path
    /// (`Some(token)`) must keep flowing through unchanged.
    #[test]
    fn owner_session_builds_sync_requests_without_ucan() {
        // Owner mesh: no UCAN.
        let owner: Option<String> = None;
        match PeerSession::sync_push_request(&owner, "space-1", serde_json::json!({"k": "v"})) {
            Request::SyncPush { ucan_token, .. } => {
                assert_eq!(ucan_token, None, "owner SyncPush must not carry a UCAN");
            }
            other => panic!("expected SyncPush, got {other:?}"),
        }
        match PeerSession::sync_pull_request(&owner, "space-1", Some("2026-01-01T00:00:00Z")) {
            Request::SyncPull { ucan_token, .. } => {
                assert_eq!(ucan_token, None, "owner SyncPull must not carry a UCAN");
            }
            other => panic!("expected SyncPull, got {other:?}"),
        }

        // Regular space session: token flows through unchanged.
        let space: Option<String> = Some("ucan-abc".to_string());
        match PeerSession::sync_push_request(&space, "space-1", serde_json::json!({})) {
            Request::SyncPush { ucan_token, .. } => {
                assert_eq!(ucan_token.as_deref(), Some("ucan-abc"));
            }
            other => panic!("expected SyncPush, got {other:?}"),
        }
    }

    /// `connect_owner` must NOT load a UCAN, and the source must NOT send an
    /// `Announce` (the owner mesh skips the announce round-trip entirely).
    /// A source-level guard keeps both invariants from silently regressing —
    /// behavioural verification is the Task-8 integration test.
    #[test]
    fn connect_owner_skips_ucan_load_and_announce() {
        let source = include_str!("peer.rs");
        let owner_fn = source
            .split_once("pub async fn connect_owner")
            .map(|(_, rest)| rest)
            .and_then(|rest| rest.split_once("\n    /// "))
            .map(|(body, _)| body)
            .expect("connect_owner must exist in peer.rs");

        assert!(
            !owner_fn.contains("load_active_ucan_for_audience"),
            "connect_owner must NOT load a UCAN — owner devices have no UCAN \
             for themselves; the owner mesh is gated by same-owner DID-auth"
        );
        assert!(
            !owner_fn.contains("Request::Announce"),
            "connect_owner must NOT send an Announce — the owner mesh skips \
             the announce round-trip entirely"
        );
        assert!(
            owner_fn.contains("complete_client_did_auth"),
            "connect_owner MUST still run DID-auth — it is the whole security \
             model for the owner mesh"
        );
    }
}
