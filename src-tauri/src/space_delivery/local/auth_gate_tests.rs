//! Tests for [`super::auth_gate::authorize_request`] — the unified
//! pre-dispatch authorisation gate.
//!
//! Covers every stage of the pipeline:
//! - Stage 2a (no peer entry):     `rejects_request_without_prior_announce`
//! - Stage 2b (peer w/o UCAN):     `rejects_request_when_peer_announced_without_ucan`
//! - Stage 3 (expired UCAN):       `rejects_request_with_expired_cached_ucan`
//! - Stage 4 (audience):           `rejects_audience_mismatch`
//! - Stage 5 (capability):         `rejects_missing_capability_for_requested_space`
//! - Stage 5 (SyncPush floor):     `accepts_read_member_sync_push_at_gate_level`
//! - Stage 5 (MLS orthog., upload): `accepts_read_member_mls_upload_key_packages_at_gate_level`
//! - Stage 5 (MLS orthog., ack):    `accepts_read_member_mls_ack_commit_at_gate_level`
//! - Stage 5 (MLS orthog., msg):    `accepts_read_member_mls_send_message_at_gate_level`
//! - Stage 5 (MLS orthog., welc):   `accepts_read_member_mls_send_welcome_at_gate_level`
//! - Stage 6a (revoked):           `rejects_revoked_member`
//! - Stage 6b (DB error):          `surfaces_db_error_from_membership_check_as_explicit_error`
//! - Stage 1 (bypass):             `bypasses_claim_invite_cleanly`
//! - Happy path:                   `accepts_valid_request_from_active_member`
//!
//! Each reject test additionally verifies that the gate writes a `warn` row
//! to `haex_logs` (via `log_to_db`) with `source = Request::op_name`, so the
//! in-app log viewer keeps showing rejected requests; happy-path and bypass
//! tests verify that the gate writes no audit row when nothing is rejected.

#![cfg(test)]

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use tokio::sync::RwLock;

use super::authorize_request;
use crate::crdt::hlc::HlcService;
use crate::database::DbConnection;
use crate::space_delivery::local::protocol::{Request, Response};
use crate::space_delivery::local::test_support::{
    init_logs_db_inner, insert_identity, insert_member, make_ucan, setup_membership_db,
};
use crate::space_delivery::local::types::{ConnectedPeer, PeerClaim};
use crate::ucan::{CapabilityLevel, ValidatedUcan};

/// In-memory DB without the membership tables, but with `haex_logs` +
/// the HLC UDF + CRDT bookkeeping so `log_to_db` works for audit-row
/// assertions. Used by tests that short-circuit before the membership
/// check (stage 2 no-peer, stage 4 audience, stage 5 capability) and by
/// the DB-error test that wants `is_active_space_member` to fail on the
/// missing `haex_space_members` table.
///
/// Delegates the entire setup to `test_support::init_logs_db_inner` —
/// keeps this fixture byte-identical to `setup_membership_db` on every
/// shared knob (HLC, CRDT bookkeeping, `ensure_crdt_columns` policy).
fn empty_db() -> (DbConnection, Arc<Mutex<HlcService>>) {
    let (conn, hlc_service) = init_logs_db_inner();
    let db = DbConnection(Arc::new(Mutex::new(Some(conn))));
    let hlc = Arc::new(Mutex::new(hlc_service));
    (db, hlc)
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

/// Read all `haex_logs` rows via the same `logging::query_logs` the in-app
/// log viewer uses. Going through the production query (rather than a
/// bespoke `SELECT level, source, message, metadata FROM haex_logs`)
/// means any future change to `query_logs` — added column, JSON
/// normalisation, column-order change — gets exercised by these tests
/// automatically; a SQL drift between production and tests can no longer
/// pass silently.
///
/// (`select_with_crdt` is a no-op for `SELECT` statements in the
/// delete-log model — see `crdt::transformer::transform_query` — so we
/// do *not* get a hardened tombstone filter from this routing. The
/// motivation is purely the schema-drift coverage above.)
///
/// Ordering: `query_logs` returns newest first; today's assertions check
/// "exactly one row", so the order is moot. If a future test wants to
/// inspect multiple rows in temporal order, reverse the slice at the
/// callsite — don't reshape this helper.
fn select_audit_logs(db: &DbConnection) -> Vec<crate::logging::LogEntry> {
    crate::logging::query_logs(db, &crate::logging::LogQueryParams {
        source: None,
        extension_id: None,
        level: None,
        since: None,
        until: None,
        device_id: None,
        limit: None,
        offset: None,
    })
    .expect("query haex_logs")
}

/// Assert that the gate wrote exactly one audit row at `expected_level`
/// (`"warn"` for peer-side rejects, `"error"` for internal vault failures),
/// tagged with the `request`'s [`Request::op_name`] and the structured
/// `subsystem` metadata field `expected_subsystem`, and whose message
/// contains `must_contain`.
///
/// Taking the actual `&Request` (rather than a hardcoded op-name string)
/// means a future rename of any `op_name` variant stays caught here — if
/// the production tag drifts the test fails for the right reason, never
/// "I edited only one of the two strings". The `subsystem` check pins the
/// metadata convention (always set to `"AuthGate"` for any reject row
/// this module emits) so operators can filter `haex_logs` by subsystem
/// independent of the per-op `source` tag.
fn assert_single_audit_row(
    db: &DbConnection,
    expected_level: &str,
    expected_subsystem: &str,
    request: &Request,
    must_contain: &str,
) {
    let expected_op = request.op_name();
    let rows = select_audit_logs(db);
    assert_eq!(
        rows.len(),
        1,
        "expected exactly one audit row for op={expected_op}, got: {rows:?}"
    );
    let row = &rows[0];
    assert_eq!(
        row.level, expected_level,
        "audit row level must be {expected_level}, got {}",
        row.level
    );
    assert_eq!(
        row.source, expected_op,
        "audit row source must be op_name={expected_op}, got {}",
        row.source
    );
    assert!(
        row.message.contains(must_contain),
        "audit row message must mention {must_contain:?}, got: {}",
        row.message
    );
    let metadata_str = row
        .metadata
        .as_deref()
        .expect("audit row must have metadata column populated (with subsystem field)");
    let metadata_json: serde_json::Value = serde_json::from_str(metadata_str)
        .expect("audit row metadata must be valid JSON");
    assert_eq!(
        metadata_json.get("subsystem").and_then(|s| s.as_str()),
        Some(expected_subsystem),
        "audit row metadata.subsystem must be {expected_subsystem}, got: {metadata_str}"
    );
}

#[tokio::test]
async fn rejects_request_without_prior_announce() {
    let (db, hlc) = empty_db();
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
        &hlc,
    )
    .await;

    match result {
        Err(Response::Error { message }) => {
            assert!(message.contains("Announce"), "got: {message}")
        }
        other => panic!("expected reject, got {other:?}"),
    }

    assert_single_audit_row(&db, "warn", "AuthGate", &request, "no peer entry");
}

#[tokio::test]
async fn rejects_request_with_expired_cached_ucan() {
    // Stage 3: `validate_token` enforced `exp` at Announce time, but the
    // cached `ValidatedUcan` rides along for the lifetime of the QUIC
    // connection. A session that started fresh and then outlived its UCAN
    // must be rejected on the next gated request — otherwise an expired
    // capability silently keeps granting access until the peer disconnects.
    //
    // Set `expires_at = 0` so the check is independent of the system clock:
    // any positive `now` will reject. The audience matches the connection
    // DID and the capability is sufficient, so the test isolates the
    // expiry stage — only `require_not_expired` can be the rejecting
    // layer.
    let (db, hlc) = empty_db();
    let expired_ucan = ValidatedUcan {
        issuer: "did:key:zIssuer".to_string(),
        audience: "did:key:zPeer".to_string(),
        capabilities: HashMap::from([("SPACE".to_string(), CapabilityLevel::Write)]),
        expires_at: 0,
    };
    let mut peers_map: HashMap<String, ConnectedPeer> = HashMap::new();
    peers_map.insert(
        "endpoint-id".to_string(),
        make_peer("endpoint-id", "did:key:zPeer", expired_ucan),
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
        &hlc,
    )
    .await;

    match result {
        Err(Response::Error { message }) => assert!(
            message.to_lowercase().contains("expired"),
            "expected peer-facing expired-UCAN message, got: {message}"
        ),
        other => panic!("expected expired-UCAN reject, got {other:?}"),
    }

    assert_single_audit_row(&db, "warn", "AuthGate", &request, "cached UCAN expired");
}

#[tokio::test]
async fn rejects_audience_mismatch() {
    // Stage 4: a peer announces with a UCAN issued *to* someone else's DID
    // (e.g. a stolen-and-replayed token). The connection-authenticated DID
    // is `did:key:zPeer`, but the cached UCAN's audience is
    // `did:key:zSomeoneElse` — require_audience must reject.
    let (db, hlc) = empty_db();
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
        &hlc,
    )
    .await;

    match result {
        Err(Response::Error { message }) => assert!(
            message.to_lowercase().contains("audience"),
            "expected peer-facing audience-mismatch message, got: {message}"
        ),
        other => panic!("expected audience-mismatch reject, got {other:?}"),
    }

    assert_single_audit_row(&db, "warn", "AuthGate", &request, "audience");
}

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
    let (db, hlc) = empty_db();
    let mut peers_map: HashMap<String, ConnectedPeer> = HashMap::new();
    peers_map.insert(
        "endpoint-id".to_string(),
        make_peer(
            "endpoint-id",
            "did:key:zPeer",
            make_ucan("did:key:zPeer", "OTHER-SPACE", CapabilityLevel::Read),
        ),
    );
    let peers = RwLock::new(peers_map);

    let request = Request::SyncPull {
        space_id: "SPACE".into(),
        after_timestamp: None,
        ucan_token: Some("irrelevant — gate uses cached UCAN".into()),
    };

    let result = authorize_request(
        &request,
        "did:key:zPeer",
        "endpoint-id",
        &peers,
        &db,
        &hlc,
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
    let (db, hlc) = setup_membership_db();
    insert_identity(&db, "id-read-member", "did:key:zReadMember");
    insert_member(&db, "mem-read", "SPACE", "id-read-member", "read");

    let mut peers_map: HashMap<String, ConnectedPeer> = HashMap::new();
    peers_map.insert(
        "endpoint-id".to_string(),
        make_peer(
            "endpoint-id",
            "did:key:zReadMember",
            make_ucan("did:key:zReadMember", "SPACE", CapabilityLevel::Read),
        ),
    );
    let peers = RwLock::new(peers_map);

    let request = Request::SyncPush {
        space_id: "SPACE".into(),
        changes: serde_json::json!([]),
        ucan_token: Some("irrelevant — gate uses cached UCAN".into()),
    };

    let result = authorize_request(
        &request,
        "did:key:zReadMember",
        "endpoint-id",
        &peers,
        &db,
        &hlc,
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
    let (db, hlc) = setup_membership_db();
    insert_identity(&db, "id-read-member", "did:key:zReadMember");
    insert_member(&db, "mem-read", "SPACE", "id-read-member", "read");

    let mut peers_map: HashMap<String, ConnectedPeer> = HashMap::new();
    peers_map.insert(
        "endpoint-id".to_string(),
        make_peer(
            "endpoint-id",
            "did:key:zReadMember",
            make_ucan("did:key:zReadMember", "SPACE", CapabilityLevel::Read),
        ),
    );
    let peers = RwLock::new(peers_map);

    let request = Request::MlsUploadKeyPackages {
        space_id: "SPACE".into(),
        packages: vec![],
    };

    let result = authorize_request(
        &request,
        "did:key:zReadMember",
        "endpoint-id",
        &peers,
        &db,
        &hlc,
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
    let (db, hlc) = setup_membership_db();
    insert_identity(&db, "id-read-member", "did:key:zReadMember");
    insert_member(&db, "mem-read", "SPACE", "id-read-member", "read");

    let mut peers_map: HashMap<String, ConnectedPeer> = HashMap::new();
    peers_map.insert(
        "endpoint-id".to_string(),
        make_peer(
            "endpoint-id",
            "did:key:zReadMember",
            make_ucan("did:key:zReadMember", "SPACE", CapabilityLevel::Read),
        ),
    );
    let peers = RwLock::new(peers_map);

    let request = Request::MlsAckCommit {
        space_id: "SPACE".into(),
        message_ids: vec![],
    };

    let result = authorize_request(
        &request,
        "did:key:zReadMember",
        "endpoint-id",
        &peers,
        &db,
        &hlc,
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
    let (db, hlc) = setup_membership_db();
    insert_identity(&db, "id-read-member", "did:key:zReadMember");
    insert_member(&db, "mem-read", "SPACE", "id-read-member", "read");

    let mut peers_map: HashMap<String, ConnectedPeer> = HashMap::new();
    peers_map.insert(
        "endpoint-id".to_string(),
        make_peer(
            "endpoint-id",
            "did:key:zReadMember",
            make_ucan("did:key:zReadMember", "SPACE", CapabilityLevel::Read),
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
        "did:key:zReadMember",
        "endpoint-id",
        &peers,
        &db,
        &hlc,
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
    let (db, hlc) = setup_membership_db();
    insert_identity(&db, "id-read-member", "did:key:zReadMember");
    insert_member(&db, "mem-read", "SPACE", "id-read-member", "read");

    let mut peers_map: HashMap<String, ConnectedPeer> = HashMap::new();
    peers_map.insert(
        "endpoint-id".to_string(),
        make_peer(
            "endpoint-id",
            "did:key:zReadMember",
            make_ucan("did:key:zReadMember", "SPACE", CapabilityLevel::Read),
        ),
    );
    let peers = RwLock::new(peers_map);

    let request = Request::MlsSendWelcome {
        space_id: "SPACE".into(),
        recipient_did: "did:key:zRecipient".into(),
        welcome: String::new(),
    };

    let result = authorize_request(
        &request,
        "did:key:zReadMember",
        "endpoint-id",
        &peers,
        &db,
        &hlc,
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

#[tokio::test]
async fn bypasses_claim_invite_cleanly() {
    // Stage 1 bypass: ClaimInvite has its own invite-token authentication
    // mechanism (the claimer is not yet a member, so neither cache nor
    // membership lookups can authorise them). `Request::required_capability`
    // returns `None`, so authorize_request must short-circuit with
    // `Ok(None)` — even with an empty connected_peers map and an empty DB.
    let (db, hlc) = empty_db();
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
        &hlc,
    )
    .await;

    match result {
        Ok(None) => {}
        other => panic!("expected Ok(None) bypass, got {other:?}"),
    }

    assert!(
        select_audit_logs(&db).is_empty(),
        "bypass path must not write any audit row"
    );
}

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
    let (db, hlc) = setup_membership_db();
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
        &hlc,
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
async fn accepts_valid_request_from_active_member() {
    // Happy path: cached UCAN's audience == connection DID, capability
    // satisfies the request floor, and the member is in
    // haex_space_members for the target space. The gate returns
    // `Ok(Some(validated))` so the dispatch site can use the UCAN for
    // origin attribution (`authored_by_did`).
    let (db, hlc) = setup_membership_db();
    insert_identity(&db, "id-peer", "did:key:zPeer");
    insert_member(&db, "mem-peer", "SPACE", "id-peer", "write");

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

    let result = authorize_request(
        &request,
        "did:key:zPeer",
        "endpoint-id",
        &peers,
        &db,
        &hlc,
    )
    .await;

    match result {
        Ok(Some(validated)) => {
            assert_eq!(validated.audience, "did:key:zPeer");
            assert_eq!(
                validated.capabilities.get("SPACE"),
                Some(&CapabilityLevel::Write),
                "validated UCAN should carry the Write capability for SPACE",
            );
        }
        other => panic!("expected Ok(Some(_)) for active member, got {other:?}"),
    }

    assert!(
        select_audit_logs(&db).is_empty(),
        "happy-path gate pass must not write any audit row"
    );
}

#[tokio::test]
async fn rejects_request_when_peer_announced_without_ucan() {
    // Stage 2b: the peer DOES have an entry in `connected_peers` (so it
    // didn't forge an endpoint-id), but `validated_ucan` is `None`. This
    // is the ClaimInvite-without-follow-up-Announce shape — the whole
    // reason `ConnectedPeer::validated_ucan` is `Option<ValidatedUcan>`
    // and the gate's `None` arm exists (see `auth_gate.rs:31-39`).
    // Silently treating `None` as a pass would defeat the entire gate.
    let (db, hlc) = empty_db();
    let peer = ConnectedPeer {
        endpoint_id: "endpoint-id".to_string(),
        did: "did:key:zPeer".to_string(),
        label: None,
        claims: Vec::<PeerClaim>::new(),
        connected_at: "2026-06-12T00:00:00Z".to_string(),
        validated_ucan: None,
    };
    let mut peers_map: HashMap<String, ConnectedPeer> = HashMap::new();
    peers_map.insert("endpoint-id".to_string(), peer);
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
        &hlc,
    )
    .await;

    match result {
        Err(Response::Error { message }) => {
            assert!(
                message.contains("Announce"),
                "expected reject mentioning Announce, got: {message}"
            );
        }
        other => panic!("expected reject, got {other:?}"),
    }

    assert_single_audit_row(&db, "warn", "AuthGate", &request, "no cached UCAN");
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
    let (db, hlc) = empty_db(); // no haex_space_members table → SQL error
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

    let result = authorize_request(
        &request,
        "did:key:zPeer",
        "endpoint-id",
        &peers,
        &db,
        &hlc,
    )
    .await;

    match result {
        Err(Response::Error { message }) => {
            assert!(
                message.contains("Membership check failed"),
                "expected DB-error reject, got: {message}"
            );
        }
        other => panic!("expected DB error response, got {other:?}"),
    }

    assert_single_audit_row(&db, "error", "AuthGate", &request, "internal failure: membership check DB error");
}
