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
        // Scoped to `request_once`, which is where the wire round-trip (and
        // therefore the timeout) lives; `request` is the retry wrapper around
        // it and must not be mistaken for the same body.
        let request_fn = source
            .split_once("async fn request_once")
            .map(|(_, rest)| rest)
            .and_then(|rest| rest.split_once("\n    /// "))
            .map(|(body, _)| body)
            .expect("PeerSession::request_once must exist in peer.rs");

        assert!(
            request_fn.contains("tokio::time::timeout"),
            "PeerSession::request_once must wrap protocol::read_response in \
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

    /// Owner-mesh pending-column recovery builds `SyncPullColumns` with
    /// `ucan_token: None` (owner sessions carry no UCAN), the requested
    /// `columns` verbatim, and `after_row_pks: None` (pagination unused).
    #[test]
    fn owner_session_builds_pull_columns_request_without_ucan() {
        let owner: Option<String> = None;
        let columns = vec![("notes".to_string(), "title".to_string())];
        match PeerSession::pull_columns_request(&owner, "space-1", &columns, None) {
            Request::SyncPullColumns {
                space_id,
                columns: req_columns,
                after_row_pks,
                ucan_token,
            } => {
                assert_eq!(
                    ucan_token, None,
                    "owner SyncPullColumns must not carry a UCAN"
                );
                assert_eq!(space_id, "space-1");
                assert_eq!(req_columns, columns);
                assert_eq!(after_row_pks, None, "pagination cursor must be None");
            }
            other => panic!("expected SyncPullColumns, got {other:?}"),
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

mod rate_limit_retry_tests {
    //! A leader that rate-limits (L5) must slow the peer down, not tear the
    //! session down.
    //!
    //! `run_pull_phase` pages and `run_push_phase` chunks back-to-back with no
    //! pacing, both propagating with `?`. Before the retry, a `rate_limited:`
    //! reply became a fatal `ProtocolError`, aborted the sync cycle and sent
    //! the driver into its 5-60 s reconnect backoff — so a first join needing
    //! more pages than `l5_handler_limits.sync_pull` advanced only a handful
    //! of pages per reconnect.

    use crate::space_delivery::local::peer::is_rate_limited;
    use crate::space_delivery::local::protocol::{Response, RATE_LIMITED_PREFIX};

    /// The exact message `leader::dispatch` builds on an L5 reject. Kept as a
    /// literal so a change to either side's format has to be made twice.
    fn leader_reject_message(op: &str) -> String {
        format!("{RATE_LIMITED_PREFIX} handler {op} exceeded per-DID cap")
    }

    #[test]
    fn recognises_the_leaders_rate_limit_message() {
        for op in ["SyncPull", "SyncPush", "MlsFetchMessages", "RequestRejoin"] {
            let resp = Response::Error {
                message: leader_reject_message(op),
            };
            assert!(
                is_rate_limited(&resp),
                "the {op} rate-limit reply must be recognised as retryable"
            );
        }
    }

    #[test]
    fn leader_message_format_matches_the_dispatcher_source() {
        // The two sides only agree through `RATE_LIMITED_PREFIX`; this pins
        // that the dispatcher really emits it, so the fixture above is not
        // testing itself.
        let dispatch = include_str!("leader/dispatch.rs");
        assert!(
            dispatch.contains("protocol::RATE_LIMITED_PREFIX"),
            "leader::dispatch must build its L5 reject message from \
             protocol::RATE_LIMITED_PREFIX, not a bare literal"
        );
    }

    #[test]
    fn real_failures_are_not_retried() {
        // Anything that is not the marker must propagate immediately —
        // retrying an AccessDenied or a malformed-request error would just
        // add latency to a failure that cannot resolve itself.
        for message in [
            "Access denied: not a member",
            "Unrecognized capability space/bogus",
            "SyncPullColumns is not served on the space path",
            // Near-misses: the prefix must be matched at the start.
            "handler SyncPull exceeded per-DID cap",
            "warning rate_limited: handler SyncPull exceeded per-DID cap",
        ] {
            let resp = Response::Error {
                message: message.to_string(),
            };
            assert!(
                !is_rate_limited(&resp),
                "{message:?} must not be treated as a rate limit"
            );
        }
    }

    #[test]
    fn non_error_responses_are_never_retried() {
        assert!(!is_rate_limited(&Response::Ok));
        assert!(!is_rate_limited(&Response::SyncChanges {
            changes: serde_json::Value::Array(vec![]),
            has_more: false,
        }));
    }

    #[test]
    fn retry_delay_outlasts_the_l5_window() {
        // The bucket must have slid past the rejected burst before the retry
        // lands, otherwise the retry is rejected too and the bound is spent
        // for nothing. The L5 window is the shared tracker's 1 s, set in
        // `commands::lifecycle`.
        use crate::space_delivery::local::peer::{RATE_LIMIT_MAX_RETRIES, RATE_LIMIT_RETRY_DELAY};
        assert!(
            RATE_LIMIT_RETRY_DELAY > std::time::Duration::from_secs(1),
            "retry delay must exceed the 1 s L5 window"
        );
        assert!(
            RATE_LIMIT_MAX_RETRIES >= 1,
            "a zero retry budget restores the teardown behaviour"
        );
    }
}
