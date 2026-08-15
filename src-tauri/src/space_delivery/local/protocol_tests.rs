//! Tests for `Request` metadata helpers used by the unified AuthGate.

use super::Request;
use crate::ucan::CapabilityLevel;

/// Build one instance of every `Request` variant with its `space_id` set to
/// `expected`, then assert `space_id_of` returns exactly that.
#[test]
fn space_id_of_returns_space_id_field_for_every_variant() {
    let expected = "space-under-test";

    let variants: Vec<Request> = vec![
        Request::Announce {
            endpoint_id: "endpoint-1".into(),
            space_id: expected.into(),
            label: None,
            claims: None,
            ucan_token: Some("ucan".into()),
        },
        Request::MlsUploadKeyPackages {
            space_id: expected.into(),
            packages: vec![],
            pops: vec![],
        },
        Request::MlsFetchKeyPackage {
            space_id: expected.into(),
            target_did: "did:key:abc".into(),
        },
        Request::MlsSendMessage {
            space_id: expected.into(),
            message: "msg".into(),
            message_type: "application".into(),
            committer_ucan: None,
            committer_commit_bind_sig: None,
        },
        Request::MlsFetchMessages {
            space_id: expected.into(),
            after_id: None,
        },
        Request::MlsSendWelcome {
            space_id: expected.into(),
            recipient_did: "did:key:abc".into(),
            welcome: "welcome".into(),
        },
        Request::MlsFetchWelcomes {
            space_id: expected.into(),
        },
        Request::MlsAckCommit {
            space_id: expected.into(),
            message_ids: vec![],
        },
        Request::MlsKeyPackageCount {
            space_id: expected.into(),
        },
        Request::RequestRejoin {
            space_id: expected.into(),
            ucan_token: Some("ucan".into()),
        },
        Request::SubmitExternalCommit {
            space_id: expected.into(),
            commit: "commit".into(),
            ucan_token: Some("ucan".into()),
        },
        Request::SyncPush {
            space_id: expected.into(),
            changes: serde_json::json!({}),
            ucan_token: Some("ucan".into()),
        },
        Request::SyncPull {
            space_id: expected.into(),
            after_timestamp: None,
            ucan_token: Some("ucan".into()),
        },
        Request::SyncPullColumns {
            space_id: expected.into(),
            columns: vec![],
            after_row_pks: None,
            ucan_token: None,
        },
        Request::ClaimInvite {
            space_id: expected.into(),
            token: "token".into(),
            endpoint_id: "endpoint-1".into(),
            key_packages: vec![],
            pops: vec![],
            label: None,
            public_key: None,
        },
        Request::PushInvite {
            space_id: expected.into(),
            space_name: "Space".into(),
            space_type: "personal".into(),
            token_id: "token".into(),
            capabilities: vec![],
            include_history: false,
            inviter_did: "did:key:abc".into(),
            inviter_label: None,
            inviter_avatar: None,
            inviter_avatar_options: None,
            space_endpoints: vec![],
            origin_url: None,
            expires_at: "2099-01-01T00:00:00Z".into(),
            inviter_relay_url: None,
        },
    ];

    assert_eq!(variants.len(), 16, "test must cover every Request variant");

    for req in &variants {
        assert_eq!(
            req.space_id_of(),
            expected,
            "space_id_of returned the wrong space_id for {req:?}"
        );
    }
}

/// Each `Request` variant maps to a fixed required capability level (or
/// `None` to bypass the AuthGate). Locks the mapping the gate will rely on
/// in Phase 4 of the unified-authgate refactor.
#[test]
fn required_capability_matches_documented_mapping() {
    let space = "space-x";

    // Read-level operations. RequestRejoin + SubmitExternalCommit live here
    // (not under Write) because the inline UCAN checks they replace in
    // `leader.rs` enforce `CapabilityLevel::Read` — this refactor must not
    // tighten the floor. SyncPush lives here because per-batch Write
    // refinement happens in `inbound_sync::authorize_inbound_sync_push`,
    // not at the gate; the gate only enforces "must be a member to push
    // at all" so read-only members can push their own
    // membership / device / KeyPackage rows.
    //
    // All MLS-protocol variants live here too. UCAN capability classifies
    // who can write *space content* (haex_peer_shares + file bytes); MLS
    // membership is a separate domain and the MLS state machine itself
    // enforces what each member may do inside the group. See
    // `Request::required_capability` doc-comment for the full rationale.
    let read_variants: Vec<Request> = vec![
        Request::MlsFetchKeyPackage {
            space_id: space.into(),
            target_did: "did:key:abc".into(),
        },
        Request::MlsFetchMessages {
            space_id: space.into(),
            after_id: None,
        },
        Request::MlsFetchWelcomes {
            space_id: space.into(),
        },
        Request::MlsKeyPackageCount {
            space_id: space.into(),
        },
        Request::MlsUploadKeyPackages {
            space_id: space.into(),
            packages: vec![],
            pops: vec![],
        },
        Request::MlsSendMessage {
            space_id: space.into(),
            message: "msg".into(),
            message_type: "application".into(),
            committer_ucan: None,
            committer_commit_bind_sig: None,
        },
        Request::MlsSendWelcome {
            space_id: space.into(),
            recipient_did: "did:key:abc".into(),
            welcome: "welcome".into(),
        },
        Request::MlsAckCommit {
            space_id: space.into(),
            message_ids: vec![],
        },
        Request::SyncPull {
            space_id: space.into(),
            after_timestamp: None,
            ucan_token: Some("ucan".into()),
        },
        Request::SyncPullColumns {
            space_id: space.into(),
            columns: vec![],
            after_row_pks: None,
            ucan_token: None,
        },
        Request::SyncPush {
            space_id: space.into(),
            changes: serde_json::json!({}),
            ucan_token: Some("ucan".into()),
        },
        Request::RequestRejoin {
            space_id: space.into(),
            ucan_token: Some("ucan".into()),
        },
        Request::SubmitExternalCommit {
            space_id: space.into(),
            commit: "commit".into(),
            ucan_token: Some("ucan".into()),
        },
    ];
    for req in &read_variants {
        assert_eq!(
            req.required_capability(),
            Some(CapabilityLevel::Read),
            "expected Read for {req:?}",
        );
    }

    // No request variant currently floors at Write at the gate. Per-batch
    // Write refinement for SyncPush happens downstream in
    // `inbound_sync::authorize_inbound_sync_push`.
    let write_variants: Vec<Request> = vec![];
    for req in &write_variants {
        assert_eq!(
            req.required_capability(),
            Some(CapabilityLevel::Write),
            "expected Write for {req:?}",
        );
    }

    // Bypass the gate: Announce bootstraps the cache, ClaimInvite is
    // invite-token-authenticated, PushInvite is leader-internal delivery.
    let bypass_variants: Vec<Request> = vec![
        Request::Announce {
            endpoint_id: "endpoint-1".into(),
            space_id: space.into(),
            label: None,
            claims: None,
            ucan_token: Some("ucan".into()),
        },
        Request::ClaimInvite {
            space_id: space.into(),
            token: "token".into(),
            endpoint_id: "endpoint-1".into(),
            key_packages: vec![],
            pops: vec![],
            label: None,
            public_key: None,
        },
        Request::PushInvite {
            space_id: space.into(),
            space_name: "Space".into(),
            space_type: "personal".into(),
            token_id: "token".into(),
            capabilities: vec![],
            include_history: false,
            inviter_did: "did:key:abc".into(),
            inviter_label: None,
            inviter_avatar: None,
            inviter_avatar_options: None,
            space_endpoints: vec![],
            origin_url: None,
            expires_at: "2099-01-01T00:00:00Z".into(),
            inviter_relay_url: None,
        },
    ];
    for req in &bypass_variants {
        assert_eq!(
            req.required_capability(),
            None,
            "expected None (bypass) for {req:?}",
        );
    }

    // Sanity: total variants covered = total variants in enum.
    assert_eq!(
        read_variants.len() + write_variants.len() + bypass_variants.len(),
        16,
        "test must cover every Request variant",
    );
}

/// Forward-compat guard for the deprecation of the wire-level `ucan_token`
/// field. The AuthGate consumes the connection-cached UCAN, so the field on
/// the wire is dead weight for the four non-bypass arms. Step 1 of the
/// removal makes the field `#[serde(default)] Option<String>` so future
/// senders can omit it entirely. This test pins that contract: a payload
/// without `ucan_token` must still deserialize.
///
/// Step 2 will stop the sender (`peer.rs`) from writing the field at all.
/// Step 3 will remove the field entirely once every device has parsed at
/// least one Step-1 payload.
#[test]
fn non_bypass_requests_deserialize_without_ucan_token_field() {
    // Request is serde-tagged with `op` in SCREAMING_SNAKE_CASE (see
    // protocol.rs:34) — not an outer-keyed object. Match the on-the-wire
    // shape exactly.
    let cases = [
        (
            "SyncPush",
            serde_json::json!({
                "op": "SYNC_PUSH",
                "space_id": "SPACE",
                "changes": []
            }),
        ),
        (
            "SyncPull",
            serde_json::json!({
                "op": "SYNC_PULL",
                "space_id": "SPACE",
                "after_timestamp": null
            }),
        ),
        (
            "RequestRejoin",
            serde_json::json!({
                "op": "REQUEST_REJOIN",
                "space_id": "SPACE"
            }),
        ),
        (
            "SubmitExternalCommit",
            serde_json::json!({
                "op": "SUBMIT_EXTERNAL_COMMIT",
                "space_id": "SPACE",
                "commit": "AAAA"
            }),
        ),
    ];

    for (name, payload) in &cases {
        let req: Request = serde_json::from_value(payload.clone()).unwrap_or_else(|e| {
            panic!("{name} payload without ucan_token must deserialize, got error: {e}")
        });
        // Drilled-down check: the missing field becomes `None`, never a
        // serde default like an empty string.
        let token_is_none = match &req {
            Request::SyncPush { ucan_token, .. } => ucan_token.is_none(),
            Request::SyncPull { ucan_token, .. } => ucan_token.is_none(),
            Request::RequestRejoin { ucan_token, .. } => ucan_token.is_none(),
            Request::SubmitExternalCommit { ucan_token, .. } => ucan_token.is_none(),
            _ => panic!("{name} deserialized into wrong variant: {req:?}"),
        };
        assert!(
            token_is_none,
            "{name} ucan_token must be None when omitted on the wire, got: {req:?}",
        );
    }
}

/// Twin of `non_bypass_requests_deserialize_without_ucan_token_field` for the
/// explicit-`null` wire shape. `#[serde(default)] Option<String>` accepts
/// both "field absent" and `"field": null`, so today both yield `None`.
/// Pin that contract: a Step-2 sender (or a foreign interop client) emitting
/// `null` instead of dropping the field must still produce `None` on the
/// receiver — not an empty-string sentinel or a deser error.
#[test]
fn sync_push_deserializes_with_explicit_null_ucan_token() {
    let payload = serde_json::json!({
        "op": "SYNC_PUSH",
        "space_id": "SPACE",
        "changes": [],
        "ucan_token": null,
    });
    let req: Request = serde_json::from_value(payload)
        .expect("SyncPush with explicit null ucan_token must deserialize");
    match req {
        Request::SyncPush { ucan_token, .. } => assert!(
            ucan_token.is_none(),
            "explicit null must deserialize to None, got {ucan_token:?}",
        ),
        other => panic!("expected SyncPush, got {other:?}"),
    }
}

/// `SyncPullColumns` is the owner-mesh-only pending-column recovery request
/// (Phase 3). It must carry the same metadata as `SyncPull` so it routes
/// through the identical owner gate: a `space_id` for the route, a `Read`
/// capability floor, and a stable PascalCase `op_name`. This test pins that
/// contract plus the wire shape: a serde round-trip must be lossless, and the
/// `#[serde(default)]` `ucan_token` must deserialize when omitted (owner
/// sessions send `None`).
#[test]
fn sync_pull_columns_metadata_and_roundtrip() {
    let space = "space-under-test";

    let req = Request::SyncPullColumns {
        space_id: space.into(),
        columns: vec![
            ("haex_peer_shares".into(), "encrypted_key".into()),
            ("haex_files".into(), "metadata".into()),
        ],
        after_row_pks: Some("cursor-42".into()),
        ucan_token: None,
    };

    // Metadata mirrors SyncPull exactly.
    assert_eq!(req.space_id_of(), space, "space_id_of must return space_id");
    let sync_pull = Request::SyncPull {
        space_id: space.into(),
        after_timestamp: None,
        ucan_token: None,
    };
    assert_eq!(
        req.required_capability(),
        sync_pull.required_capability(),
        "SyncPullColumns must require the same capability as SyncPull",
    );
    assert_eq!(
        req.required_capability(),
        Some(CapabilityLevel::Read),
        "SyncPullColumns must floor at Read",
    );
    assert_eq!(req.op_name(), "SyncPullColumns");

    // Serde round-trip is lossless.
    let bytes = serde_json::to_vec(&req).expect("encode SyncPullColumns");
    let decoded: Request = serde_json::from_slice(&bytes).expect("decode SyncPullColumns");
    match decoded {
        Request::SyncPullColumns {
            ref space_id,
            ref columns,
            ref after_row_pks,
            ref ucan_token,
        } => {
            assert_eq!(space_id, space);
            assert_eq!(
                columns,
                &vec![
                    ("haex_peer_shares".to_string(), "encrypted_key".to_string()),
                    ("haex_files".to_string(), "metadata".to_string()),
                ],
            );
            assert_eq!(after_row_pks.as_deref(), Some("cursor-42"));
            assert!(ucan_token.is_none());
        }
        other => panic!("round-trip produced wrong variant: {other:?}"),
    }

    // `ucan_token` is `#[serde(default)]`: a payload omitting it deserializes
    // to `None` (owner sessions send no token).
    let payload = serde_json::json!({
        "op": "SYNC_PULL_COLUMNS",
        "space_id": "SPACE",
        "columns": [["haex_files", "metadata"]],
        "after_row_pks": null,
    });
    let req: Request = serde_json::from_value(payload)
        .expect("SyncPullColumns without ucan_token must deserialize");
    match req {
        Request::SyncPullColumns { ucan_token, .. } => assert!(
            ucan_token.is_none(),
            "omitted ucan_token must deserialize to None, got {ucan_token:?}",
        ),
        other => panic!("expected SyncPullColumns, got {other:?}"),
    }
}
