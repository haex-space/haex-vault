//! `authorize_inbound_sync_push` — capability + membership gates.

use serde_json::json;

use crate::space_delivery::local::inbound_sync::{
    authorize_inbound_sync_push, InboundSyncPushOutcome,
};
use crate::space_delivery::local::test_support::{insert_identity, insert_member, make_ucan};
use crate::ucan::CapabilityLevel;

use super::helpers::{change, expect_rejected, setup_authz_db};

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
            "endpoint_id",
            "100/abcd",
            json!("endpoint-alice"),
        ),
        change(
            "haex_peer_shares",
            "share-1",
            "name",
            "100/abcd",
            json!("docs"),
        ),
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
