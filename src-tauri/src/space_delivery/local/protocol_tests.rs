//! Tests for `Request` metadata helpers used by the unified AuthGate.

use super::{IdentityClaim, Request};

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
            claims: None::<Vec<IdentityClaim>>,
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
