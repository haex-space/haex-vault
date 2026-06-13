//! Tests for [`super::auth_gate::authorize_request`] — the unified
//! pre-dispatch authorisation gate.
//!
//! Covers every stage of the pipeline:
//! - Stage 2a (no peer entry):     `rejects_request_without_prior_announce`
//! - Stage 2b (peer w/o UCAN):     `rejects_request_when_peer_announced_without_ucan`
//! - Stage 3 (expired UCAN):       `rejects_request_with_expired_cached_ucan`
//! - Stage 4 (audience):           `rejects_audience_mismatch`
//! - Stage 5 (capability):         `rejects_insufficient_capability`
//! - Stage 5 (SyncPush floor):     `accepts_read_member_sync_push_at_gate_level`
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

use rusqlite::Connection;
use tokio::sync::RwLock;

use super::authorize_request;
use crate::crdt::hlc::HlcService;
use crate::crdt::trigger::ensure_crdt_columns;
use crate::database::connection_context::ConnectionContext;
use crate::database::core::{install_tx_hlc_hooks, register_current_hlc_udf};
use crate::database::DbConnection;
use crate::space_delivery::local::protocol::{Request, Response};
use crate::space_delivery::local::test_support::{
    insert_identity, insert_member, make_ucan, setup_membership_db,
};
use crate::space_delivery::local::types::{ConnectedPeer, PeerClaim};
use crate::table_names::{TABLE_CRDT_CONFIGS, TABLE_CRDT_DIRTY_TABLES};
use crate::ucan::{CapabilityLevel, ValidatedUcan};

/// In-memory DB without the membership tables, but with `haex_logs` +
/// the HLC UDF + CRDT bookkeeping so `log_to_db` works for audit-row
/// assertions. Used by tests that short-circuit before the membership
/// check (stage 2 no-peer, stage 3 audience, stage 4 capability) and by
/// the DB-error test that wants `is_active_space_member` to fail on the
/// missing `haex_space_members` table.
fn empty_db() -> (DbConnection, Arc<Mutex<HlcService>>) {
    let conn = Connection::open_in_memory().expect("in-memory DB");
    let hlc_service = HlcService::new_for_testing("test-device");
    let ctx = ConnectionContext::new();
    register_current_hlc_udf(&conn, hlc_service.clone(), ctx.clone()).expect("register hlc udf");
    install_tx_hlc_hooks(&conn, ctx).expect("install hlc hooks");

    // CRDT bookkeeping table names are constants (the schema renamed them
    // to `..._no_sync` at some point) — use the same constants
    // `setup_membership_db` does so the in-memory schema mirrors production.
    conn.execute_batch(&format!(
        "CREATE TABLE {TABLE_CRDT_CONFIGS} (key TEXT PRIMARY KEY, type TEXT NOT NULL, value TEXT NOT NULL);
         CREATE TABLE {TABLE_CRDT_DIRTY_TABLES} (table_name TEXT PRIMARY KEY, last_modified TEXT);
         CREATE TABLE haex_logs (
             id TEXT PRIMARY KEY,
             timestamp TEXT NOT NULL,
             level TEXT NOT NULL,
             source TEXT NOT NULL,
             extension_id TEXT,
             message TEXT NOT NULL,
             metadata TEXT,
             device_id TEXT NOT NULL
         );",
    ))
    .expect("create logs schema");

    // Mirror `setup_membership_db`: `log_to_db` writes through
    // `execute_with_crdt` which needs the `haex_hlc` / `haex_column_hlcs`
    // columns. We deliberately use `ensure_crdt_columns` (column-only) and
    // not `ensure_crdt_columns_and_triggers` — see `setup_membership_db`
    // for the rationale (no `haex_deleted_rows` table or UDFs seeded; a
    // DELETE-path test would crash with `no such table`).
    {
        let tx = conn.unchecked_transaction().expect("begin crdt-columns tx");
        ensure_crdt_columns(&tx, "haex_logs").expect("ensure crdt columns on haex_logs");
        tx.commit().expect("commit crdt-columns tx");
    }

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

/// Read all `(level, source, message, metadata)` rows from `haex_logs`,
/// oldest first. Used by reject-path tests to assert that the gate wrote
/// exactly one row tagged with the right op name and `subsystem = "AuthGate"`.
fn select_audit_logs(db: &DbConnection) -> Vec<(String, String, String, Option<String>)> {
    let conn_guard = db.0.lock().expect("db lock");
    let conn = conn_guard.as_ref().expect("db connection");
    let mut stmt = conn
        .prepare("SELECT level, source, message, metadata FROM haex_logs ORDER BY timestamp")
        .expect("prepare select haex_logs");
    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<String>>(3)?,
            ))
        })
        .expect("query haex_logs");
    rows.collect::<Result<Vec<_>, _>>().expect("collect haex_logs rows")
}

/// Assert that the gate wrote exactly one audit row at `expected_level`
/// (`"warn"` for peer-side rejects, `"error"` for internal vault failures),
/// tagged with `expected_op` and the structured `subsystem` metadata field
/// `expected_subsystem`, and whose message contains `must_contain`.
///
/// The `subsystem` check pins the metadata convention (always set to
/// `"AuthGate"` for any reject row this module emits) so operators can
/// filter `haex_logs` by subsystem independent of the per-op `source` tag.
fn assert_single_audit_row(
    db: &DbConnection,
    expected_level: &str,
    expected_subsystem: &str,
    expected_op: &str,
    must_contain: &str,
) {
    let rows = select_audit_logs(db);
    assert_eq!(
        rows.len(),
        1,
        "expected exactly one audit row for op={expected_op}, got: {rows:?}"
    );
    let (level, source, message, metadata) = &rows[0];
    assert_eq!(
        level, expected_level,
        "audit row level must be {expected_level}, got {level}"
    );
    assert_eq!(
        source, expected_op,
        "audit row source must be op_name={expected_op}, got {source}"
    );
    assert!(
        message.contains(must_contain),
        "audit row message must mention {must_contain:?}, got: {message}"
    );
    let metadata_str = metadata
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

    assert_single_audit_row(&db, "warn", "AuthGate", "MlsUploadKeyPackages", "no peer entry");
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

    assert_single_audit_row(&db, "warn", "AuthGate", "MlsUploadKeyPackages", "cached UCAN expired");
}

#[tokio::test]
async fn rejects_audience_mismatch() {
    // Stage 3: a peer announces with a UCAN issued *to* someone else's DID
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

    assert_single_audit_row(&db, "warn", "AuthGate", "MlsUploadKeyPackages", "audience");
}

#[tokio::test]
async fn rejects_insufficient_capability() {
    // Stage 4: the UCAN audience matches the connection DID and is for the
    // right space, but only grants `Read`. An `MlsSendMessage` requires
    // `Write` — require_capability must reject before
    // is_active_space_member runs. (SyncPush is intentionally `Read` at the
    // gate; the Write refinement for non-membership tables lives in
    // `inbound_sync::authorize_inbound_sync_push`, not here.)
    let (db, hlc) = empty_db();
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

    let request = Request::MlsSendMessage {
        space_id: "SPACE".into(),
        message: String::new(),
        message_type: "application".into(),
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

    assert_single_audit_row(&db, "warn", "AuthGate", "MlsSendMessage", "capability check failed");
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
        ucan_token: "irrelevant — gate uses cached UCAN".into(),
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
    // Stage 5 kill-switch: the UCAN itself is still cryptographically
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

    assert_single_audit_row(&db, "warn", "AuthGate", "MlsSendMessage", "not an active member");
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

    assert_single_audit_row(&db, "warn", "AuthGate", "MlsUploadKeyPackages", "no cached UCAN");
}

#[tokio::test]
async fn surfaces_db_error_from_membership_check_as_explicit_error() {
    // Stage 5b: the cached UCAN passes stages 2-4 cleanly (audience matches
    // verified DID, capability suffices), but `is_active_space_member`'s
    // SQL fails because the `haex_space_members` table doesn't exist on
    // this connection. The gate must surface that as a
    // `"Membership check failed: …"` peer-facing message — distinct from
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

    assert_single_audit_row(&db, "error", "AuthGate", "MlsUploadKeyPackages", "internal failure: membership check DB error");
}
