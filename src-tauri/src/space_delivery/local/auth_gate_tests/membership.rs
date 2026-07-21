use std::collections::HashMap;

use tokio::sync::RwLock;

use super::helpers::{assert_single_audit_row, authorize_default, empty_db, make_peer};
use crate::space_delivery::local::protocol::{Request, Response};
use crate::space_delivery::local::test_support::{insert_identity, make_ucan, setup_membership_db};
use crate::space_delivery::local::types::ConnectedPeer;
use crate::ucan::CapabilityLevel;

#[tokio::test]
async fn rejects_revoked_member() {
    // Stage 6 kill-switch: the UCAN itself is still cryptographically
    // valid (audience matches, capability suffices), but the admin has
    // removed the member from haex_space_members. In the delete-log
    // model "revoked" means the row is absent — `is_active_space_member`
    // joins haex_space_members + haex_identities and returns 0 rows,
    // which the gate must convert into a peer-facing "not an active
    // member" reject. This is the runtime revocation knob: it lets an
    // admin terminate a member's access without re-issuing keys.
    let (db, _hlc, log_sink) = setup_membership_db();
    // Seed an identity but deliberately NOT a haex_space_members row for
    // this (space, identity) pair — equivalent to a tombstoned membership.
    insert_identity(&db, "id-revoked", "did:key:zRevoked");

    let mut peers_map: HashMap<String, ConnectedPeer> = HashMap::new();
    peers_map.insert(
        "endpoint-id".to_string(),
        make_peer(
            "endpoint-id",
            "did:key:zRevoked",
            make_ucan("did:key:zRevoked", "SPACE", CapabilityLevel::Write),
        ),
    );
    let peers = RwLock::new(peers_map);

    let request = Request::MlsSendMessage {
        space_id: "SPACE".into(),
        message: String::new(),
        message_type: "application".into(),
    };

    let result = authorize_default(
        &request,
        "did:key:zRevoked",
        "endpoint-id",
        &peers,
        &db,
        Some(&log_sink),
    )
    .await;

    match result {
        Err(Response::Error { message }) => assert!(
            message.contains("active member"),
            "expected 'not an active member' reject, got: {message}"
        ),
        other => panic!("expected membership reject, got {other:?}"),
    }

    assert_single_audit_row(&db, "warn", "AuthGate", &request, "not an active member");
}

#[tokio::test]
async fn surfaces_db_error_from_membership_check_as_explicit_error() {
    // Stage 6b: the cached UCAN passes stages 2-5 cleanly (not expired,
    // audience matches verified DID, capability suffices), but
    // `is_active_space_member`'s SQL fails because the `haex_space_members`
    // table doesn't exist on this connection. The gate must surface that as
    // a `"Membership check failed: …"` peer-facing message — distinct from
    // the plain "not an active member" reject — so the dispatch site (and
    // any future log triage) can tell a DB outage apart from a revoked
    // member.
    let (db, _hlc, log_sink) = empty_db(); // no haex_space_members table → SQL error
    let mut peers_map: HashMap<String, ConnectedPeer> = HashMap::new();
    peers_map.insert(
        "endpoint-id".to_string(),
        make_peer(
            "endpoint-id",
            "did:key:zPeer",
            make_ucan("did:key:zPeer", "SPACE", CapabilityLevel::Write),
        ),
    );
    let peers = RwLock::new(peers_map);

    let request = Request::MlsUploadKeyPackages {
        space_id: "SPACE".into(),
        packages: vec![],
    };

    let result =
        authorize_default(&request, "did:key:zPeer", "endpoint-id", &peers, &db, Some(&log_sink)).await;

    match result {
        Err(Response::Error { message }) => {
            assert!(
                message.contains("Membership check failed"),
                "expected DB-error reject, got: {message}"
            );
        }
        other => panic!("expected DB error response, got {other:?}"),
    }

    assert_single_audit_row(
        &db,
        "error",
        "AuthGate",
        &request,
        "internal failure: membership check DB error",
    );
}
