use std::collections::HashMap;

use tokio::sync::RwLock;

use super::helpers::{
    assert_single_audit_row, authorize_default, empty_db, make_peer, select_audit_logs,
};
use crate::space_delivery::local::protocol::{Request, Response};
use crate::space_delivery::local::test_support::{
    insert_identity, insert_member, make_ucan, setup_membership_db,
};
use crate::space_delivery::local::types::ConnectedPeer;
use crate::ucan::Cap;

#[tokio::test]
async fn rejects_missing_capability_for_requested_space() {
    // Stage 5: the cached UCAN holds capabilities, but **not for the space
    // this request targets**. `require_capability` short-circuits with
    // `MissingCapability` before `is_active_space_member` runs.
    //
    // (After the MLS-orthogonality refactor, no `Request` variant floors at
    // `Write` at the gate — so the historical "Read-only member sends an
    // MlsSendMessage that needs Write" scenario no longer exists. The
    // `wrong-space-in-UCAN` shape below is now the canonical Stage-5
    // trigger.)
    let (db, _hlc, log_sink) = empty_db();
    let mut peers_map: HashMap<String, ConnectedPeer> = HashMap::new();
    peers_map.insert(
        "endpoint-id".to_string(),
        make_peer(
            "endpoint-id",
            "did:key:zPeer",
            make_ucan("did:key:zPeer", "OTHER-SPACE", Cap::Read),
        ),
    );
    let peers = RwLock::new(peers_map);

    let request = Request::SyncPull {
        space_id: "SPACE".into(),
        after_timestamp: None,
        ucan_token: Some("irrelevant — gate uses cached UCAN".into()),
    };

    let result = authorize_default(
        &request,
        "did:key:zPeer",
        "endpoint-id",
        &peers,
        &db,
        Some(&log_sink),
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

    assert_single_audit_row(&db, "warn", "AuthGate", &request, "capability check failed");
}

#[tokio::test]
async fn accepts_read_member_sync_push_at_gate_level() {
    // SyncPush requires only Read at the gate level — per-batch capability
    // refinement (Write for non-membership tables) lives in
    // `inbound_sync::authorize_inbound_sync_push`, not here. This test
    // guards against a future "tighten SyncPush to Write" that would
    // silently break read-only members trying to push their own
    // membership / device / KeyPackage rows.
    let (db, _hlc, log_sink) = setup_membership_db();
    insert_identity(&db, "id-read-member", "did:key:zReadMember");
    insert_member(&db, "mem-read", "SPACE", "id-read-member", "read");

    let mut peers_map: HashMap<String, ConnectedPeer> = HashMap::new();
    peers_map.insert(
        "endpoint-id".to_string(),
        make_peer(
            "endpoint-id",
            "did:key:zReadMember",
            make_ucan("did:key:zReadMember", "SPACE", Cap::Read),
        ),
    );
    let peers = RwLock::new(peers_map);

    let request = Request::SyncPush {
        space_id: "SPACE".into(),
        changes: serde_json::json!([]),
        ucan_token: Some("irrelevant — gate uses cached UCAN".into()),
    };

    let result = authorize_default(
        &request,
        "did:key:zReadMember",
        "endpoint-id",
        &peers,
        &db,
        Some(&log_sink),
    )
    .await;

    assert!(
        matches!(result, Ok(Some(_))),
        "Read member must pass the gate for SyncPush — per-batch Write refinement happens downstream, got {result:?}"
    );

    assert!(
        select_audit_logs(&db).is_empty(),
        "happy-path gate pass must not write any audit row"
    );
}

#[tokio::test]
async fn accepts_read_member_mls_upload_key_packages_at_gate_level() {
    // `MlsUploadKeyPackages` is MLS-membership maintenance, not a space-content
    // write: the peer uploads its OWN KeyPackages into the leader-local
    // `haex_local_delivery_key_packages_no_sync` buffer so future Welcome/Add
    // commits can pull one. The leader tags each entry with `verified_did`
    // (see `leader.rs::handle_request` `MlsUploadKeyPackages` arm), so a
    // read-only member cannot inject KeyPackages for any other member.
    //
    // Without this, read-only members exhaust the initial ClaimInvite batch
    // and can never refill — every later MLS Add to a sub-group that needs
    // their KeyPackage fails, and the member silently falls out of the
    // encrypted group while still being listed as an active space member.
    // This test guards against a future re-tightening to `Write`.
    let (db, _hlc, log_sink) = setup_membership_db();
    insert_identity(&db, "id-read-member", "did:key:zReadMember");
    insert_member(&db, "mem-read", "SPACE", "id-read-member", "read");

    let mut peers_map: HashMap<String, ConnectedPeer> = HashMap::new();
    peers_map.insert(
        "endpoint-id".to_string(),
        make_peer(
            "endpoint-id",
            "did:key:zReadMember",
            make_ucan("did:key:zReadMember", "SPACE", Cap::Read),
        ),
    );
    let peers = RwLock::new(peers_map);

    let request = Request::MlsUploadKeyPackages {
        space_id: "SPACE".into(),
        packages: vec![],
        pops: vec![],
    };

    let result = authorize_default(
        &request,
        "did:key:zReadMember",
        "endpoint-id",
        &peers,
        &db,
        Some(&log_sink),
    )
    .await;

    assert!(
        matches!(result, Ok(Some(_))),
        "Read member must pass the gate for MlsUploadKeyPackages — uploading own KeyPackages is MLS-membership maintenance, not a space-content write, got {result:?}"
    );

    assert!(
        select_audit_logs(&db).is_empty(),
        "happy-path gate pass must not write any audit row"
    );
}

#[tokio::test]
async fn accepts_read_member_mls_ack_commit_at_gate_level() {
    // `MlsAckCommit` is pure per-DID bookkeeping: the caller marks its own
    // pending-commit entries as acked so the leader can clean them up.
    // Every active MLS member — read-only included — must ack, otherwise
    // commits stay pending forever and cleanup stalls for the whole group.
    // UCAN-Read at this gate is correct: ACK is not a space-content write.
    let (db, _hlc, log_sink) = setup_membership_db();
    insert_identity(&db, "id-read-member", "did:key:zReadMember");
    insert_member(&db, "mem-read", "SPACE", "id-read-member", "read");

    let mut peers_map: HashMap<String, ConnectedPeer> = HashMap::new();
    peers_map.insert(
        "endpoint-id".to_string(),
        make_peer(
            "endpoint-id",
            "did:key:zReadMember",
            make_ucan("did:key:zReadMember", "SPACE", Cap::Read),
        ),
    );
    let peers = RwLock::new(peers_map);

    let request = Request::MlsAckCommit {
        space_id: "SPACE".into(),
        message_ids: vec![],
    };

    let result = authorize_default(
        &request,
        "did:key:zReadMember",
        "endpoint-id",
        &peers,
        &db,
        Some(&log_sink),
    )
    .await;

    assert!(
        matches!(result, Ok(Some(_))),
        "Read member must pass the gate for MlsAckCommit — acking own commits is MLS bookkeeping, not a space-content write, got {result:?}"
    );

    assert!(
        select_audit_logs(&db).is_empty(),
        "happy-path gate pass must not write any audit row"
    );
}

#[tokio::test]
async fn accepts_read_member_mls_send_message_at_gate_level() {
    // `MlsSendMessage` relays an MLS-protocol message (application, commit,
    // proposal). The leader is a pure relay; the MLS state machine at the
    // recipient validates signatures, epoch, and sender membership. UCAN
    // capability is the wrong layer to enforce "who may write": that gate
    // lives on `haex_peer_shares` via `authorize_inbound_sync_push`, not
    // on MLS-message classification.
    let (db, _hlc, log_sink) = setup_membership_db();
    insert_identity(&db, "id-read-member", "did:key:zReadMember");
    insert_member(&db, "mem-read", "SPACE", "id-read-member", "read");

    let mut peers_map: HashMap<String, ConnectedPeer> = HashMap::new();
    peers_map.insert(
        "endpoint-id".to_string(),
        make_peer(
            "endpoint-id",
            "did:key:zReadMember",
            make_ucan("did:key:zReadMember", "SPACE", Cap::Read),
        ),
    );
    let peers = RwLock::new(peers_map);

    let request = Request::MlsSendMessage {
        space_id: "SPACE".into(),
        message: String::new(),
        message_type: "application".into(),
        committer_ucan: None,
        committer_commit_bind_sig: None,
    };

    let result = authorize_default(
        &request,
        "did:key:zReadMember",
        "endpoint-id",
        &peers,
        &db,
        Some(&log_sink),
    )
    .await;

    assert!(
        matches!(result, Ok(Some(_))),
        "Read member must pass the gate for MlsSendMessage — MLS-message relay is orthogonal to UCAN write capability, got {result:?}"
    );

    assert!(
        select_audit_logs(&db).is_empty(),
        "happy-path gate pass must not write any audit row"
    );
}

#[tokio::test]
async fn accepts_read_member_mls_send_welcome_at_gate_level() {
    // `MlsSendWelcome` relays a Welcome blob to a recipient. Whether the
    // sender is *entitled* to invite a new member is application policy
    // enforced at invite-token creation, not at Welcome forwarding — the
    // leader cannot meaningfully distinguish "legitimate add by an
    // authorised inviter" from "Welcome for a malicious add" at this layer,
    // because the MLS payload is opaque here. Treating Welcome forwarding
    // as a UCAN-Write op therefore protects nothing while breaking the
    // case where a read-only member legitimately forwards a Welcome
    // generated by the leader.
    let (db, _hlc, log_sink) = setup_membership_db();
    insert_identity(&db, "id-read-member", "did:key:zReadMember");
    insert_member(&db, "mem-read", "SPACE", "id-read-member", "read");

    let mut peers_map: HashMap<String, ConnectedPeer> = HashMap::new();
    peers_map.insert(
        "endpoint-id".to_string(),
        make_peer(
            "endpoint-id",
            "did:key:zReadMember",
            make_ucan("did:key:zReadMember", "SPACE", Cap::Read),
        ),
    );
    let peers = RwLock::new(peers_map);

    let request = Request::MlsSendWelcome {
        space_id: "SPACE".into(),
        recipient_did: "did:key:zRecipient".into(),
        welcome: String::new(),
    };

    let result = authorize_default(
        &request,
        "did:key:zReadMember",
        "endpoint-id",
        &peers,
        &db,
        Some(&log_sink),
    )
    .await;

    assert!(
        matches!(result, Ok(Some(_))),
        "Read member must pass the gate for MlsSendWelcome — Welcome relay is orthogonal to UCAN write capability, got {result:?}"
    );

    assert!(
        select_audit_logs(&db).is_empty(),
        "happy-path gate pass must not write any audit row"
    );
}
