//! `authorize_inbound_sync_push` — per-row space scope (cross-space PK collision).

use serde_json::json;

use crate::space_delivery::local::inbound_sync::{
    authorize_inbound_sync_push, InboundSyncPushOutcome,
};
use crate::space_delivery::local::test_support::{insert_identity, insert_member, make_ucan};
use crate::ucan::Cap;

use super::helpers::{change, expect_rejected, insert_share, setup_authz_db};

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

    let ucan = make_ucan("did:key:zMallory", "space-A", Cap::Write);
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

    let ucan = make_ucan("did:key:zAlice", "space-A", Cap::Write);
    let changes = vec![
        change(
            "haex_peer_shares",
            "share-new",
            "endpoint_id",
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

    let ucan = make_ucan("did:key:zMallory", "space-A", Cap::Write);
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
            "endpoint_id",
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
