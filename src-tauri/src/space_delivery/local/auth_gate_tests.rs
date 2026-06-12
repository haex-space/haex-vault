//! Tests for [`super::auth_gate::authorize_request`] — the unified
//! pre-dispatch authorisation gate.
//!
//! Covers every stage of the pipeline:
//! - Stage 2 (cache lookup): `rejects_request_without_prior_announce`
//! - Stage 3 (audience):     `rejects_audience_mismatch`
//! - Stage 4 (capability):   `rejects_insufficient_capability`
//! - Stage 5 (membership):   `rejects_revoked_member`
//! - Stage 1 (bypass):       `bypasses_claim_invite_cleanly`
//! - Happy path:             `accepts_valid_request_from_active_member`

#![cfg(test)]

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use rusqlite::Connection;
use tokio::sync::RwLock;

use super::authorize_request;
use crate::database::DbConnection;
use crate::space_delivery::local::protocol::{Request, Response};
use crate::space_delivery::local::test_support::{
    insert_identity, insert_member, make_ucan, setup_membership_db,
};
use crate::space_delivery::local::types::{ConnectedPeer, PeerClaim};
use crate::ucan::{CapabilityLevel, ValidatedUcan};

/// Bare in-memory `DbConnection`. The no-Announce reject path short-circuits
/// at the cache-lookup step before any SQL runs, so we deliberately do **not**
/// reach for the heavier membership helper here.
fn empty_db() -> DbConnection {
    let conn = Connection::open_in_memory().expect("in-memory DB");
    DbConnection(Arc::new(Mutex::new(Some(conn))))
}

/// Build a `ConnectedPeer` whose cached `validated_ucan` is the one the
/// AuthGate's stage-2 lookup will resolve. The endpoint-id/audience-DID
/// pair is what stages 3-5 then check.
fn make_peer(endpoint_id: &str, did: &str, validated_ucan: ValidatedUcan) -> ConnectedPeer {
    ConnectedPeer {
        endpoint_id: endpoint_id.to_string(),
        did: did.to_string(),
        label: None,
        claims: Vec::<PeerClaim>::new(),
        connected_at: "1970-01-01T00:00:00Z".to_string(),
        validated_ucan: Some(validated_ucan),
    }
}

#[tokio::test]
async fn rejects_request_without_prior_announce() {
    let db = empty_db();
    let peers: RwLock<HashMap<String, ConnectedPeer>> = RwLock::new(HashMap::new());

    let request = Request::MlsUploadKeyPackages {
        space_id: "SPACE".into(),
        packages: vec![],
    };

    let result = authorize_request(
        &request,
        "did:key:zPeer",
        "endpoint-id",
        &peers,
        &db,
    )
    .await;

    match result {
        Err(Response::Error { message }) => {
            assert!(message.contains("Announce"), "got: {message}")
        }
        other => panic!("expected reject, got {other:?}"),
    }
}

#[tokio::test]
async fn rejects_audience_mismatch() {
    // Stage 3: a peer announces with a UCAN issued *to* someone else's DID
    // (e.g. a stolen-and-replayed token). The connection-authenticated DID
    // is `did:key:zPeer`, but the cached UCAN's audience is
    // `did:key:zSomeoneElse` — require_audience must reject.
    let db = empty_db();
    let mut peers_map: HashMap<String, ConnectedPeer> = HashMap::new();
    peers_map.insert(
        "endpoint-id".to_string(),
        make_peer(
            "endpoint-id",
            "did:key:zPeer",
            make_ucan("did:key:zSomeoneElse", "SPACE", CapabilityLevel::Write),
        ),
    );
    let peers = RwLock::new(peers_map);

    let request = Request::MlsUploadKeyPackages {
        space_id: "SPACE".into(),
        packages: vec![],
    };

    let result = authorize_request(
        &request,
        "did:key:zPeer",
        "endpoint-id",
        &peers,
        &db,
    )
    .await;

    match result {
        Err(Response::Error { message }) => assert!(
            message.to_lowercase().contains("audience"),
            "expected peer-facing audience-mismatch message, got: {message}"
        ),
        other => panic!("expected audience-mismatch reject, got {other:?}"),
    }
}

#[tokio::test]
async fn rejects_insufficient_capability() {
    // Stage 4: the UCAN audience matches the connection DID and is for the
    // right space, but only grants `Read`. A SyncPush requires `Write` —
    // require_capability must reject before is_active_space_member runs.
    let db = empty_db();
    let mut peers_map: HashMap<String, ConnectedPeer> = HashMap::new();
    peers_map.insert(
        "endpoint-id".to_string(),
        make_peer(
            "endpoint-id",
            "did:key:zPeer",
            make_ucan("did:key:zPeer", "SPACE", CapabilityLevel::Read),
        ),
    );
    let peers = RwLock::new(peers_map);

    let request = Request::SyncPush {
        space_id: "SPACE".into(),
        changes: serde_json::json!({}),
        ucan_token: "ignored".into(),
    };

    let result = authorize_request(
        &request,
        "did:key:zPeer",
        "endpoint-id",
        &peers,
        &db,
    )
    .await;

    match result {
        Err(Response::Error { message }) => {
            let lower = message.to_lowercase();
            assert!(
                lower.contains("denied") || lower.contains("capability"),
                "expected capability-rejection message, got: {message}"
            );
        }
        other => panic!("expected capability reject, got {other:?}"),
    }
}

#[tokio::test]
async fn bypasses_claim_invite_cleanly() {
    // Stage 1 bypass: ClaimInvite has its own invite-token authentication
    // mechanism (the claimer is not yet a member, so neither cache nor
    // membership lookups can authorise them). `Request::required_capability`
    // returns `None`, so authorize_request must short-circuit with
    // `Ok(None)` — even with an empty connected_peers map and an empty DB.
    let db = empty_db();
    let peers: RwLock<HashMap<String, ConnectedPeer>> = RwLock::new(HashMap::new());

    let request = Request::ClaimInvite {
        space_id: "SPACE".into(),
        token: "invite-token".into(),
        endpoint_id: "endpoint-id".into(),
        key_packages: vec![],
        label: None,
        public_key: None,
    };

    let result = authorize_request(
        &request,
        "did:key:zNewcomer",
        "endpoint-id",
        &peers,
        &db,
    )
    .await;

    match result {
        Ok(None) => {}
        other => panic!("expected Ok(None) bypass, got {other:?}"),
    }
}

#[tokio::test]
async fn rejects_revoked_member() {
    // Stage 5 kill-switch: the UCAN itself is still cryptographically
    // valid (audience matches, capability suffices), but the admin has
    // removed the member from haex_space_members. In the delete-log
    // model "revoked" means the row is absent — `is_active_space_member`
    // joins haex_space_members + haex_identities and returns 0 rows,
    // which the gate must convert into a peer-facing "not an active
    // member" reject. This is the runtime revocation knob: it lets an
    // admin terminate a member's access without re-issuing keys.
    let db = setup_membership_db();
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

    let result = authorize_request(
        &request,
        "did:key:zRevoked",
        "endpoint-id",
        &peers,
        &db,
    )
    .await;

    match result {
        Err(Response::Error { message }) => assert!(
            message.contains("active member"),
            "expected 'not an active member' reject, got: {message}"
        ),
        other => panic!("expected membership reject, got {other:?}"),
    }
}
