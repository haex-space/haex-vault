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
            ucan_token: "ucan".into(),
        },
        Request::MlsUploadKeyPackages {
            space_id: expected.into(),
            packages: vec![],
        },
        Request::MlsFetchKeyPackage {
            space_id: expected.into(),
            target_did: "did:key:abc".into(),
        },
        Request::MlsSendMessage {
            space_id: expected.into(),
            message: "msg".into(),
            message_type: "application".into(),
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
            ucan_token: "ucan".into(),
        },
        Request::SubmitExternalCommit {
            space_id: expected.into(),
            commit: "commit".into(),
            ucan_token: "ucan".into(),
        },
        Request::SyncPush {
            space_id: expected.into(),
            changes: serde_json::json!({}),
            ucan_token: "ucan".into(),
        },
        Request::SyncPull {
            space_id: expected.into(),
            after_timestamp: None,
            ucan_token: "ucan".into(),
        },
        Request::ClaimInvite {
            space_id: expected.into(),
            token: "token".into(),
            endpoint_id: "endpoint-1".into(),
            key_packages: vec![],
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

    assert_eq!(variants.len(), 15, "test must cover every Request variant");

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
    // membership / device / KeyPackage rows. See
    // `Request::required_capability` doc-comment.
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
        Request::SyncPull {
            space_id: space.into(),
            after_timestamp: None,
            ucan_token: "ucan".into(),
        },
        Request::SyncPush {
            space_id: space.into(),
            changes: serde_json::json!({}),
            ucan_token: "ucan".into(),
        },
        Request::RequestRejoin {
            space_id: space.into(),
            ucan_token: "ucan".into(),
        },
        Request::SubmitExternalCommit {
            space_id: space.into(),
            commit: "commit".into(),
            ucan_token: "ucan".into(),
        },
    ];
    for req in &read_variants {
        assert_eq!(
            req.required_capability(),
            Some(CapabilityLevel::Read),
            "expected Read for {req:?}",
        );
    }

    // Write-level operations
    let write_variants: Vec<Request> = vec![
        Request::MlsUploadKeyPackages {
            space_id: space.into(),
            packages: vec![],
        },
        Request::MlsSendMessage {
            space_id: space.into(),
            message: "msg".into(),
            message_type: "application".into(),
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
    ];
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
            ucan_token: "ucan".into(),
        },
        Request::ClaimInvite {
            space_id: space.into(),
            token: "token".into(),
            endpoint_id: "endpoint-1".into(),
            key_packages: vec![],
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
        15,
        "test must cover every Request variant",
    );
}
