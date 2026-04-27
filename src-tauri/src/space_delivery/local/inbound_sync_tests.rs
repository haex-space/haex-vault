//! Tests for [`super::inbound_sync`] — the single authorisation choke
//! point for inbound CRDT pushes from space peers.
//!
//! The pipeline is tested at three levels:
//!
//! - **`validate_and_attribute`** (pure transform): table whitelist,
//!   `space_id` column scope, `authored_by_did` strip + re-injection.
//! - **`authorize_inbound_sync_push`** (pipeline): capability gate,
//!   active-membership gate, per-row space scope, per-row ownership.
//! - **`enforce_row_space_scope`** (cross-space attack surface): a member
//!   of two spaces cannot rewrite a foreign-space row by omitting the
//!   `space_id` column from the change set.
//!
//! Test DBs are built with `setup_authz_db()` — schemas mirror production
//! but **skip CRDT triggers**: the authorisation pipeline reads only via
//! `read_existing_column` (a plain `SELECT`), so HLC tracking and
//! trigger-driven column-HLC bookkeeping are orthogonal to what these
//! tests assert. Seeding helpers (`insert_identity`, `insert_member` …)
//! go through `database::core::execute`, the same trigger-bypass path
//! production uses for system inserts, so the seed data is shaped exactly
//! like a row applied via a CRDT push would be.

#![cfg(test)]

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use rusqlite::Connection;
use serde_json::{json, Value as JsonValue};

use crate::crdt::hlc::HlcService;
use crate::crdt::scanner::LocalColumnChange;
use crate::database::connection_context::ConnectionContext;
use crate::database::core::{self, install_tx_hlc_hooks, register_current_hlc_udf};
use crate::database::DbConnection;
use crate::space_delivery::local::inbound_sync::{
    authorize_inbound_sync_push, validate_and_attribute, InboundSyncPushOutcome,
};
use crate::table_names::{TABLE_CRDT_CONFIGS, TABLE_CRDT_DIRTY_TABLES};
use crate::ucan::{CapabilityLevel, ValidatedUcan};

// =========================================================================
// Test fixtures
// =========================================================================

fn make_change(
    table: &str,
    row_id: &str,
    column: &str,
    hlc: &str,
    value: JsonValue,
) -> LocalColumnChange {
    LocalColumnChange {
        table_name: table.to_string(),
        row_pks: format!(r#"{{"id":"{row_id}"}}"#),
        column_name: column.to_string(),
        hlc_timestamp: hlc.to_string(),
        value,
        device_id: "device-under-test".to_string(),
    }
}

fn change(
    table: &str,
    row_id: &str,
    column: &str,
    hlc: &str,
    value: JsonValue,
) -> LocalColumnChange {
    LocalColumnChange {
        table_name: table.to_string(),
        row_pks: format!(r#"{{"id":"{row_id}"}}"#),
        column_name: column.to_string(),
        hlc_timestamp: hlc.to_string(),
        value,
        device_id: "wire-device-id".to_string(),
    }
}

fn expect_accepted(outcome: InboundSyncPushOutcome) -> Vec<LocalColumnChange> {
    match outcome {
        InboundSyncPushOutcome::Accepted { changes } => changes,
        InboundSyncPushOutcome::Rejected { reason } => {
            panic!("expected Accepted, got Rejected: {reason}")
        }
    }
}

fn expect_rejected(outcome: InboundSyncPushOutcome) -> String {
    match outcome {
        InboundSyncPushOutcome::Rejected { reason } => reason,
        InboundSyncPushOutcome::Accepted { .. } => panic!("expected Rejected, got Accepted"),
    }
}

/// In-memory DB with all schemas the authorisation pipeline reads from.
/// Schemas mirror production but skip CRDT triggers — these tests do not
/// exercise the CRDT merge layer, only authorisation decisions.
fn setup_authz_db() -> DbConnection {
    let conn = Connection::open_in_memory().unwrap();
    let hlc = HlcService::new_for_testing("test-device");
    let ctx = ConnectionContext::new();
    register_current_hlc_udf(&conn, hlc, ctx.clone()).unwrap();
    install_tx_hlc_hooks(&conn, ctx).unwrap();

    conn.execute_batch(&format!(
        "CREATE TABLE {} (key TEXT PRIMARY KEY, type TEXT NOT NULL, value TEXT NOT NULL);",
        TABLE_CRDT_CONFIGS
    ))
    .unwrap();
    conn.execute_batch(&format!(
        "CREATE TABLE {} (table_name TEXT PRIMARY KEY, last_modified TEXT);",
        TABLE_CRDT_DIRTY_TABLES
    ))
    .unwrap();

    conn.execute_batch(
        "CREATE TABLE haex_identities (
            id TEXT PRIMARY KEY,
            did TEXT NOT NULL UNIQUE,
            public_key TEXT,
            created_at TEXT
        );

        CREATE TABLE haex_spaces (
            id TEXT PRIMARY KEY,
            type TEXT NOT NULL DEFAULT 'local',
            status TEXT NOT NULL DEFAULT 'active',
            name TEXT NOT NULL
        );

        CREATE TABLE haex_space_members (
            id TEXT PRIMARY KEY,
            space_id TEXT NOT NULL,
            identity_id TEXT NOT NULL,
            role TEXT NOT NULL DEFAULT 'read',
            authored_by_did TEXT,
            joined_at TEXT
        );

        CREATE TABLE haex_space_devices (
            id TEXT PRIMARY KEY,
            space_id TEXT NOT NULL,
            identity_id TEXT,
            device_endpoint_id TEXT NOT NULL,
            device_name TEXT NOT NULL,
            relay_url TEXT,
            authored_by_did TEXT
        );

        CREATE TABLE haex_device_mls_enrollments (
            id TEXT PRIMARY KEY,
            space_id TEXT NOT NULL,
            device_id TEXT NOT NULL,
            key_package TEXT NOT NULL,
            welcome TEXT,
            status TEXT NOT NULL DEFAULT 'pending',
            authored_by_did TEXT
        );

        CREATE TABLE haex_mls_sync_keys (
            id TEXT PRIMARY KEY,
            space_id TEXT NOT NULL,
            epoch INTEGER NOT NULL,
            key_data TEXT NOT NULL,
            authored_by_did TEXT
        );

        CREATE TABLE haex_peer_shares (
            id TEXT PRIMARY KEY,
            space_id TEXT NOT NULL,
            device_endpoint_id TEXT NOT NULL,
            name TEXT NOT NULL,
            local_path TEXT NOT NULL,
            authored_by_did TEXT
        );",
    )
    .unwrap();

    DbConnection(Arc::new(Mutex::new(Some(conn))))
}

fn insert_identity(db: &DbConnection, identity_id: &str, did: &str) {
    core::execute(
        "INSERT INTO haex_identities (id, did) VALUES (?1, ?2)".to_string(),
        vec![json!(identity_id), json!(did)],
        db,
    )
    .unwrap();
}

fn insert_member(
    db: &DbConnection,
    member_row_id: &str,
    space_id: &str,
    identity_id: &str,
    role: &str,
) {
    core::execute(
        "INSERT INTO haex_space_members (id, space_id, identity_id, role) \
         VALUES (?1, ?2, ?3, ?4)"
            .to_string(),
        vec![
            json!(member_row_id),
            json!(space_id),
            json!(identity_id),
            json!(role),
        ],
        db,
    )
    .unwrap();
}

fn insert_device(
    db: &DbConnection,
    device_row_id: &str,
    space_id: &str,
    identity_id: Option<&str>,
    endpoint_id: &str,
    name: &str,
) {
    core::execute(
        "INSERT INTO haex_space_devices \
         (id, space_id, identity_id, device_endpoint_id, device_name) \
         VALUES (?1, ?2, ?3, ?4, ?5)"
            .to_string(),
        vec![
            json!(device_row_id),
            json!(space_id),
            identity_id.map(JsonValue::from).unwrap_or(JsonValue::Null),
            json!(endpoint_id),
            json!(name),
        ],
        db,
    )
    .unwrap();
}

fn insert_share(
    db: &DbConnection,
    share_row_id: &str,
    space_id: &str,
    endpoint_id: &str,
    name: &str,
) {
    core::execute(
        "INSERT INTO haex_peer_shares \
         (id, space_id, device_endpoint_id, name, local_path) \
         VALUES (?1, ?2, ?3, ?4, ?5)"
            .to_string(),
        vec![
            json!(share_row_id),
            json!(space_id),
            json!(endpoint_id),
            json!(name),
            json!("/x"),
        ],
        db,
    )
    .unwrap();
}

fn make_ucan(audience: &str, space_id: &str, level: CapabilityLevel) -> ValidatedUcan {
    let mut capabilities = HashMap::new();
    capabilities.insert(space_id.to_string(), level);
    ValidatedUcan {
        issuer: "did:key:zIssuer".to_string(),
        audience: audience.to_string(),
        capabilities,
        expires_at: u64::MAX,
    }
}

// =========================================================================
// validate_and_attribute — pure transform
// =========================================================================

#[test]
fn rejects_non_whitelisted_table() {
    let changes = vec![make_change(
        "haex_identities",
        "row-1",
        "private_key",
        "1000/abcd",
        json!("leaked-key"),
    )];
    let reason = expect_rejected(validate_and_attribute(
        "space-A",
        "did:key:zAlice",
        changes,
    ));
    assert!(
        reason.contains("haex_identities"),
        "reason should name the bad table: {reason}"
    );
}

#[test]
fn rejects_foreign_space_id_column_value() {
    let changes = vec![make_change(
        "haex_peer_shares",
        "row-1",
        "space_id",
        "1000/abcd",
        json!("space-B"),
    )];
    let reason = expect_rejected(validate_and_attribute(
        "space-A",
        "did:key:zAlice",
        changes,
    ));
    assert!(
        reason.contains("space-A") || reason.contains("space-B"),
        "reason should mention the space_id mismatch: {reason}"
    );
}

#[test]
fn accepts_matching_space_id_column_value() {
    let changes = vec![make_change(
        "haex_peer_shares",
        "row-1",
        "space_id",
        "1000/abcd",
        json!("space-A"),
    )];
    let out = expect_accepted(validate_and_attribute(
        "space-A",
        "did:key:zAlice",
        changes,
    ));
    assert!(out.iter().any(|c| c.column_name == "space_id"));
}

#[test]
fn strips_client_supplied_authored_by_did() {
    // Attacker pushes a row and tries to claim Bob wrote it.
    let changes = vec![
        make_change(
            "haex_peer_shares",
            "row-1",
            "space_id",
            "1000/abcd",
            json!("space-A"),
        ),
        make_change(
            "haex_peer_shares",
            "row-1",
            "name",
            "2000/abcd",
            json!("evil-share"),
        ),
        make_change(
            "haex_peer_shares",
            "row-1",
            "authored_by_did",
            "3000/abcd",
            json!("did:key:zBob"),
        ),
    ];
    let out = expect_accepted(validate_and_attribute(
        "space-A",
        "did:key:zMallory",
        changes,
    ));

    let author_changes: Vec<&LocalColumnChange> = out
        .iter()
        .filter(|c| c.column_name == "authored_by_did")
        .collect();
    assert_eq!(
        author_changes.len(),
        1,
        "exactly one authored_by_did change expected, got {author_changes:?}"
    );
    let author_value = author_changes[0].value.as_str().unwrap();
    assert_eq!(
        author_value, "did:key:zMallory",
        "origin must be the UCAN audience (Mallory), not the client claim (Bob)",
    );
}

#[test]
fn injects_one_authored_by_did_per_unique_row() {
    let changes = vec![
        make_change(
            "haex_peer_shares",
            "row-1",
            "name",
            "1000/abcd",
            json!("share-one"),
        ),
        make_change(
            "haex_peer_shares",
            "row-1",
            "local_path",
            "2000/abcd",
            json!("/path/one"),
        ),
        make_change(
            "haex_peer_shares",
            "row-2",
            "name",
            "3000/abcd",
            json!("share-two"),
        ),
    ];
    let out = expect_accepted(validate_and_attribute(
        "space-A",
        "did:key:zAlice",
        changes,
    ));

    let mut author_rows: Vec<&str> = out
        .iter()
        .filter(|c| c.column_name == "authored_by_did")
        .map(|c| c.row_pks.as_str())
        .collect();
    author_rows.sort();
    assert_eq!(
        author_rows,
        vec![r#"{"id":"row-1"}"#, r#"{"id":"row-2"}"#],
        "exactly one authored_by_did per unique row expected",
    );
}

#[test]
fn authored_by_did_uses_max_hlc_within_row_group() {
    // HLC string format is "<ntp_nanos>/<node_id_hex>" — compared
    // numerically by the time component. Pass them out of order to
    // prove the transform picks the real maximum, not the first-seen.
    let changes = vec![
        make_change("haex_peer_shares", "row-1", "name", "1000/abcd", json!("a")),
        make_change(
            "haex_peer_shares",
            "row-1",
            "local_path",
            "9000/abcd",
            json!("z"),
        ),
        make_change(
            "haex_peer_shares",
            "row-1",
            "device_endpoint_id",
            "5000/abcd",
            json!("m"),
        ),
    ];
    let out = expect_accepted(validate_and_attribute(
        "space-A",
        "did:key:zAlice",
        changes,
    ));

    let author = out
        .iter()
        .find(|c| c.column_name == "authored_by_did")
        .expect("authored_by_did should be injected");
    assert_eq!(
        author.hlc_timestamp, "9000/abcd",
        "authored_by_did HLC should be the max HLC of the row-group",
    );
}

#[test]
fn origin_always_comes_from_audience_never_from_payload() {
    // Even with no client-supplied authored_by_did, the leader sets one
    // from the audience.
    let changes = vec![make_change(
        "haex_space_members",
        "row-1",
        "role",
        "1000/abcd",
        json!("write"),
    )];
    let out = expect_accepted(validate_and_attribute(
        "space-A",
        "did:key:zAlice",
        changes,
    ));

    let author = out
        .iter()
        .find(|c| c.column_name == "authored_by_did")
        .expect("authored_by_did must be injected even without client input");
    assert_eq!(author.value.as_str(), Some("did:key:zAlice"));
}

#[test]
fn empty_batch_stays_empty() {
    let out = expect_accepted(validate_and_attribute(
        "space-A",
        "did:key:zAlice",
        vec![],
    ));
    assert!(out.is_empty());
}

#[test]
fn preserves_non_attribution_changes() {
    // Sanity: the transform must not swallow legitimate changes.
    let changes = vec![make_change(
        "haex_peer_shares",
        "row-1",
        "name",
        "1000/abcd",
        json!("my-share"),
    )];
    let out = expect_accepted(validate_and_attribute(
        "space-A",
        "did:key:zAlice",
        changes,
    ));
    assert!(
        out.iter()
            .any(|c| c.column_name == "name" && c.value.as_str() == Some("my-share")),
        "original 'name' change must be preserved",
    );
}

// =========================================================================
// authorize_inbound_sync_push — capability + membership gates
// =========================================================================

#[test]
fn authz_read_only_member_can_push_own_membership_update() {
    let db = setup_authz_db();
    insert_identity(&db, "id-mallory", "did:key:zMallory");
    insert_member(&db, "mem-mallory", "space-A", "id-mallory", "read");

    let ucan = make_ucan("did:key:zMallory", "space-A", CapabilityLevel::Read);
    let changes = vec![change(
        "haex_space_members",
        "mem-mallory",
        "joined_at",
        "100/abcd",
        json!("2026-01-01"),
    )];

    let outcome = authorize_inbound_sync_push(&db, "space-A", "endpoint-mallory", &ucan, changes);
    assert!(
        matches!(outcome, InboundSyncPushOutcome::Accepted { .. }),
        "read-only member must be able to update her own membership row, got: {outcome:?}",
    );
}

#[test]
fn authz_read_only_member_cannot_push_peer_shares() {
    let db = setup_authz_db();
    insert_identity(&db, "id-mallory", "did:key:zMallory");
    insert_member(&db, "mem-mallory", "space-A", "id-mallory", "read");

    let ucan = make_ucan("did:key:zMallory", "space-A", CapabilityLevel::Read);
    let changes = vec![change(
        "haex_peer_shares",
        "share-1",
        "name",
        "100/abcd",
        json!("malicious-share"),
    )];

    let reason = expect_rejected(authorize_inbound_sync_push(
        &db,
        "space-A",
        "endpoint-mallory",
        &ucan,
        changes,
    ));
    assert!(
        reason.to_lowercase().contains("access denied") || reason.contains("Insufficient"),
        "expected capability rejection, got: {reason}",
    );
}

#[test]
fn authz_write_member_can_push_peer_shares() {
    let db = setup_authz_db();
    insert_identity(&db, "id-alice", "did:key:zAlice");
    insert_member(&db, "mem-alice", "space-A", "id-alice", "write");

    let ucan = make_ucan("did:key:zAlice", "space-A", CapabilityLevel::Write);
    let changes = vec![
        change(
            "haex_peer_shares",
            "share-1",
            "space_id",
            "100/abcd",
            json!("space-A"),
        ),
        change(
            "haex_peer_shares",
            "share-1",
            "device_endpoint_id",
            "100/abcd",
            json!("endpoint-alice"),
        ),
        change("haex_peer_shares", "share-1", "name", "100/abcd", json!("docs")),
        change(
            "haex_peer_shares",
            "share-1",
            "local_path",
            "100/abcd",
            json!("/home/alice/docs"),
        ),
    ];

    let outcome = authorize_inbound_sync_push(&db, "space-A", "endpoint-alice", &ucan, changes);
    assert!(
        matches!(outcome, InboundSyncPushOutcome::Accepted { .. }),
        "write member must be able to push peer_shares, got: {outcome:?}",
    );
}

#[test]
fn authz_mixed_push_with_user_table_requires_write() {
    // Membership row + peer_shares row in the same push: the mixed batch
    // escalates to Write because of peer_shares.
    let db = setup_authz_db();
    insert_identity(&db, "id-mallory", "did:key:zMallory");
    insert_member(&db, "mem-mallory", "space-A", "id-mallory", "read");

    let ucan = make_ucan("did:key:zMallory", "space-A", CapabilityLevel::Read);
    let changes = vec![
        change(
            "haex_space_members",
            "mem-mallory",
            "joined_at",
            "100/abcd",
            json!("2026-01-01"),
        ),
        change(
            "haex_peer_shares",
            "share-1",
            "name",
            "100/abcd",
            json!("evil-share"),
        ),
    ];

    let reason = expect_rejected(authorize_inbound_sync_push(
        &db,
        "space-A",
        "endpoint-mallory",
        &ucan,
        changes,
    ));
    assert!(
        reason.to_lowercase().contains("access denied"),
        "mixed push with peer_shares should fail capability check for read member: {reason}",
    );
}

#[test]
fn authz_member_for_other_space_rejected() {
    let db = setup_authz_db();
    insert_identity(&db, "id-mallory", "did:key:zMallory");
    insert_member(&db, "mem-mallory", "space-B", "id-mallory", "write");

    let ucan = make_ucan("did:key:zMallory", "space-A", CapabilityLevel::Write);
    let changes = vec![change(
        "haex_space_members",
        "mem-mallory",
        "role",
        "100/abcd",
        json!("admin"),
    )];

    let reason = expect_rejected(authorize_inbound_sync_push(
        &db,
        "space-A",
        "endpoint-mallory",
        &ucan,
        changes,
    ));
    assert!(
        reason.contains("not an active member"),
        "non-member must be rejected, got: {reason}",
    );
}

// =========================================================================
// authorize_inbound_sync_push — per-row ownership
// =========================================================================

#[test]
fn authz_read_only_cannot_overwrite_admin_membership_row() {
    // Classic privilege escalation: Mallory tries to set Bob's membership
    // identity_id to herself.
    let db = setup_authz_db();
    insert_identity(&db, "id-mallory", "did:key:zMallory");
    insert_identity(&db, "id-bob", "did:key:zBob");
    insert_member(&db, "mem-mallory", "space-A", "id-mallory", "read");
    insert_member(&db, "mem-bob", "space-A", "id-bob", "admin");

    let ucan = make_ucan("did:key:zMallory", "space-A", CapabilityLevel::Read);
    let changes = vec![change(
        "haex_space_members",
        "mem-bob",
        "identity_id",
        "100/abcd",
        json!("id-mallory"),
    )];

    let reason = expect_rejected(authorize_inbound_sync_push(
        &db,
        "space-A",
        "endpoint-mallory",
        &ucan,
        changes,
    ));
    assert!(
        reason.contains("ownership") || reason.contains("does not match caller"),
        "Mallory must not be able to overwrite Bob's row, got: {reason}",
    );
}

#[test]
fn authz_read_only_cannot_modify_foreign_member_role() {
    // role=admin on Bob's row without changing identity_id — ownership
    // check must pull identity_id from the existing DB row.
    let db = setup_authz_db();
    insert_identity(&db, "id-mallory", "did:key:zMallory");
    insert_identity(&db, "id-bob", "did:key:zBob");
    insert_member(&db, "mem-mallory", "space-A", "id-mallory", "read");
    insert_member(&db, "mem-bob", "space-A", "id-bob", "read");

    let ucan = make_ucan("did:key:zMallory", "space-A", CapabilityLevel::Read);
    let changes = vec![change(
        "haex_space_members",
        "mem-bob",
        "role",
        "100/abcd",
        json!("admin"),
    )];

    let reason = expect_rejected(authorize_inbound_sync_push(
        &db,
        "space-A",
        "endpoint-mallory",
        &ucan,
        changes,
    ));
    assert!(
        reason.contains("ownership") || reason.contains("does not match caller"),
        "Mallory must not silently modify Bob's row, got: {reason}",
    );
}

#[test]
fn authz_member_can_insert_own_new_membership_row() {
    let db = setup_authz_db();
    insert_identity(&db, "id-mallory", "did:key:zMallory");
    insert_member(&db, "mem-mallory-old", "space-A", "id-mallory", "read");

    let ucan = make_ucan("did:key:zMallory", "space-A", CapabilityLevel::Read);
    let changes = vec![
        change(
            "haex_space_members",
            "mem-mallory-new",
            "space_id",
            "100/abcd",
            json!("space-A"),
        ),
        change(
            "haex_space_members",
            "mem-mallory-new",
            "identity_id",
            "100/abcd",
            json!("id-mallory"),
        ),
        change(
            "haex_space_members",
            "mem-mallory-new",
            "role",
            "100/abcd",
            json!("read"),
        ),
    ];

    let outcome = authorize_inbound_sync_push(&db, "space-A", "endpoint-mallory", &ucan, changes);
    assert!(
        matches!(outcome, InboundSyncPushOutcome::Accepted { .. }),
        "self-insert must succeed, got: {outcome:?}",
    );
}

#[test]
fn authz_member_cannot_insert_membership_with_others_identity() {
    let db = setup_authz_db();
    insert_identity(&db, "id-mallory", "did:key:zMallory");
    insert_identity(&db, "id-bob", "did:key:zBob");
    insert_member(&db, "mem-mallory", "space-A", "id-mallory", "read");

    let ucan = make_ucan("did:key:zMallory", "space-A", CapabilityLevel::Read);
    let changes = vec![
        change(
            "haex_space_members",
            "mem-fake",
            "space_id",
            "100/abcd",
            json!("space-A"),
        ),
        change(
            "haex_space_members",
            "mem-fake",
            "identity_id",
            "100/abcd",
            json!("id-bob"),
        ),
        change(
            "haex_space_members",
            "mem-fake",
            "role",
            "100/abcd",
            json!("admin"),
        ),
    ];

    let reason = expect_rejected(authorize_inbound_sync_push(
        &db,
        "space-A",
        "endpoint-mallory",
        &ucan,
        changes,
    ));
    assert!(
        reason.contains("ownership") || reason.contains("does not match caller"),
        "must not allow forging row for foreign identity, got: {reason}",
    );
}

#[test]
fn authz_member_can_register_own_device() {
    let db = setup_authz_db();
    insert_identity(&db, "id-alice", "did:key:zAlice");
    insert_member(&db, "mem-alice", "space-A", "id-alice", "read");

    let ucan = make_ucan("did:key:zAlice", "space-A", CapabilityLevel::Read);
    let changes = vec![
        change(
            "haex_space_devices",
            "dev-alice",
            "space_id",
            "100/abcd",
            json!("space-A"),
        ),
        change(
            "haex_space_devices",
            "dev-alice",
            "device_endpoint_id",
            "100/abcd",
            json!("endpoint-alice"),
        ),
        change(
            "haex_space_devices",
            "dev-alice",
            "device_name",
            "100/abcd",
            json!("Alice's Laptop"),
        ),
    ];

    let outcome = authorize_inbound_sync_push(&db, "space-A", "endpoint-alice", &ucan, changes);
    assert!(
        matches!(outcome, InboundSyncPushOutcome::Accepted { .. }),
        "Alice must be able to register her own device, got: {outcome:?}",
    );
}

#[test]
fn authz_member_cannot_hijack_foreign_device_endpoint() {
    let db = setup_authz_db();
    insert_identity(&db, "id-mallory", "did:key:zMallory");
    insert_identity(&db, "id-bob", "did:key:zBob");
    insert_member(&db, "mem-mallory", "space-A", "id-mallory", "read");

    let ucan = make_ucan("did:key:zMallory", "space-A", CapabilityLevel::Read);
    let changes = vec![
        change(
            "haex_space_devices",
            "dev-fake",
            "space_id",
            "100/abcd",
            json!("space-A"),
        ),
        change(
            "haex_space_devices",
            "dev-fake",
            "device_endpoint_id",
            "100/abcd",
            json!("endpoint-bob"),
        ),
        change(
            "haex_space_devices",
            "dev-fake",
            "device_name",
            "100/abcd",
            json!("Pretending to be Bob"),
        ),
    ];

    let reason = expect_rejected(authorize_inbound_sync_push(
        &db,
        "space-A",
        "endpoint-mallory",
        &ucan,
        changes,
    ));
    assert!(
        reason.contains("ownership") || reason.contains("does not match caller"),
        "device endpoint hijack must be rejected, got: {reason}",
    );
}

#[test]
fn authz_member_cannot_modify_foreign_device_row() {
    // Existing device row belongs to Bob; Mallory tries to update its
    // name without changing endpoint_id (so ownership comes from DB).
    let db = setup_authz_db();
    insert_identity(&db, "id-mallory", "did:key:zMallory");
    insert_identity(&db, "id-bob", "did:key:zBob");
    insert_member(&db, "mem-mallory", "space-A", "id-mallory", "read");
    insert_device(
        &db,
        "dev-bob",
        "space-A",
        Some("id-bob"),
        "endpoint-bob",
        "Bob's Phone",
    );

    let ucan = make_ucan("did:key:zMallory", "space-A", CapabilityLevel::Read);
    let changes = vec![change(
        "haex_space_devices",
        "dev-bob",
        "device_name",
        "100/abcd",
        json!("Hacked"),
    )];

    let reason = expect_rejected(authorize_inbound_sync_push(
        &db,
        "space-A",
        "endpoint-mallory",
        &ucan,
        changes,
    ));
    assert!(
        reason.contains("ownership") || reason.contains("does not match caller"),
        "Mallory must not be able to alter Bob's device row, got: {reason}",
    );
}

#[test]
fn authz_mixed_batch_one_foreign_row_rejects_whole_push() {
    // Whole-batch atomicity: a single bad row taints the whole push.
    let db = setup_authz_db();
    insert_identity(&db, "id-mallory", "did:key:zMallory");
    insert_identity(&db, "id-bob", "did:key:zBob");
    insert_member(&db, "mem-mallory", "space-A", "id-mallory", "read");
    insert_member(&db, "mem-bob", "space-A", "id-bob", "read");

    let ucan = make_ucan("did:key:zMallory", "space-A", CapabilityLevel::Read);
    let changes = vec![
        change(
            "haex_space_members",
            "mem-mallory",
            "joined_at",
            "100/abcd",
            json!("2026-01-01"),
        ),
        change(
            "haex_space_members",
            "mem-bob",
            "role",
            "100/abcd",
            json!("admin"),
        ),
    ];

    let reason = expect_rejected(authorize_inbound_sync_push(
        &db,
        "space-A",
        "endpoint-mallory",
        &ucan,
        changes,
    ));
    assert!(
        reason.contains("ownership") || reason.contains("does not match caller"),
        "the whole batch must be rejected even if one row is legit, got: {reason}",
    );
}

#[test]
fn authz_cross_space_id_injection_blocked() {
    // Defense-in-depth: even with valid Write capability for space-A,
    // attempting to set space_id=space-B in the payload must fail at the
    // column-level check in validate_and_attribute.
    let db = setup_authz_db();
    insert_identity(&db, "id-alice", "did:key:zAlice");
    insert_member(&db, "mem-alice", "space-A", "id-alice", "write");

    let ucan = make_ucan("did:key:zAlice", "space-A", CapabilityLevel::Write);
    let changes = vec![change(
        "haex_peer_shares",
        "share-1",
        "space_id",
        "100/abcd",
        json!("space-B"),
    )];

    let reason = expect_rejected(authorize_inbound_sync_push(
        &db,
        "space-A",
        "endpoint-alice",
        &ucan,
        changes,
    ));
    assert!(
        reason.contains("space-A") || reason.contains("space-B"),
        "cross-space injection must be blocked, got: {reason}",
    );
}

// =========================================================================
// authorize_inbound_sync_push — per-row space scope (cross-space PK collision)
// =========================================================================

#[test]
fn authz_cross_space_row_mutation_via_missing_space_id_blocked() {
    // Multi-space attack: Mallory is a write member of both space-A and
    // space-B. share-bob lives in space-B (Bob's row). Mallory crafts a
    // SyncPush addressed at the space-A leader, targeting share-bob's PK,
    // and *omits* the `space_id` column from the change set so the
    // column-level check in validate_and_attribute does not fire. Without
    // the row-space-scope gate the leader would happily apply the update
    // to a foreign-space row.
    let db = setup_authz_db();
    insert_identity(&db, "id-mallory", "did:key:zMallory");
    insert_member(&db, "mem-mallory-A", "space-A", "id-mallory", "write");
    insert_member(&db, "mem-mallory-B", "space-B", "id-mallory", "write");
    insert_share(&db, "share-bob", "space-B", "endpoint-bob", "Bob's docs");

    let ucan = make_ucan("did:key:zMallory", "space-A", CapabilityLevel::Write);
    let changes = vec![change(
        "haex_peer_shares",
        "share-bob",
        "name",
        "100/abcd",
        json!("PWND"),
    )];

    let reason = expect_rejected(authorize_inbound_sync_push(
        &db,
        "space-A",
        "endpoint-mallory",
        &ucan,
        changes,
    ));
    assert!(
        reason.to_lowercase().contains("cross-space") || reason.contains("space-B"),
        "cross-space PK-collision attack must be blocked, got: {reason}",
    );
}

#[test]
fn authz_insert_without_space_id_column_rejected() {
    // A real outbound scanner emits all data columns for fresh inserts,
    // including space_id. A push that omits space_id on a fresh row is
    // either malformed or an attempt to leave the row unscoped — reject
    // it before it reaches the apply layer.
    let db = setup_authz_db();
    insert_identity(&db, "id-alice", "did:key:zAlice");
    insert_member(&db, "mem-alice", "space-A", "id-alice", "write");

    let ucan = make_ucan("did:key:zAlice", "space-A", CapabilityLevel::Write);
    let changes = vec![
        change(
            "haex_peer_shares",
            "share-new",
            "device_endpoint_id",
            "100/abcd",
            json!("endpoint-alice"),
        ),
        change(
            "haex_peer_shares",
            "share-new",
            "name",
            "100/abcd",
            json!("docs"),
        ),
        change(
            "haex_peer_shares",
            "share-new",
            "local_path",
            "100/abcd",
            json!("/home/alice"),
        ),
    ];

    let reason = expect_rejected(authorize_inbound_sync_push(
        &db,
        "space-A",
        "endpoint-alice",
        &ucan,
        changes,
    ));
    assert!(
        reason.contains("space_id") || reason.to_lowercase().contains("cross-space"),
        "insert without space_id must be rejected, got: {reason}",
    );
}

#[test]
fn authz_authored_by_did_forge_attempt_is_rewritten() {
    // Confirms validate_and_attribute keeps working through the central
    // function: a client-supplied authored_by_did = Bob is overwritten by
    // the leader to = Mallory.
    let db = setup_authz_db();
    insert_identity(&db, "id-mallory", "did:key:zMallory");
    insert_identity(&db, "id-bob", "did:key:zBob");
    insert_member(&db, "mem-mallory", "space-A", "id-mallory", "write");

    let ucan = make_ucan("did:key:zMallory", "space-A", CapabilityLevel::Write);
    let changes = vec![
        change(
            "haex_peer_shares",
            "share-mallory",
            "space_id",
            "100/abcd",
            json!("space-A"),
        ),
        change(
            "haex_peer_shares",
            "share-mallory",
            "device_endpoint_id",
            "100/abcd",
            json!("endpoint-mallory"),
        ),
        change(
            "haex_peer_shares",
            "share-mallory",
            "name",
            "100/abcd",
            json!("share"),
        ),
        change(
            "haex_peer_shares",
            "share-mallory",
            "local_path",
            "100/abcd",
            json!("/m"),
        ),
        change(
            "haex_peer_shares",
            "share-mallory",
            "authored_by_did",
            "100/abcd",
            json!("did:key:zBob"),
        ),
    ];

    let out = match authorize_inbound_sync_push(&db, "space-A", "endpoint-mallory", &ucan, changes)
    {
        InboundSyncPushOutcome::Accepted { changes } => changes,
        InboundSyncPushOutcome::Rejected { reason } => {
            panic!("expected Accepted, got Rejected: {reason}")
        }
    };

    let author = out
        .iter()
        .find(|c| c.column_name == "authored_by_did")
        .expect("authored_by_did must be present");
    assert_eq!(
        author.value.as_str(),
        Some("did:key:zMallory"),
        "leader must overwrite forged authored_by_did with audience",
    );
}
