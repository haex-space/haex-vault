//! `authorize_inbound_sync_push` — per-row ownership.

use serde_json::json;

use crate::space_delivery::local::inbound_sync::{
    authorize_inbound_sync_push, InboundSyncPushOutcome,
};
use crate::space_delivery::local::test_support::{insert_identity, insert_member, make_ucan};
use crate::ucan::Cap;

use super::helpers::{change, expect_accepted, expect_rejected, insert_device, setup_authz_db};

#[test]
fn authz_read_only_cannot_overwrite_admin_membership_row() {
    // Classic privilege escalation: Mallory tries to set Bob's membership
    // identity_id to herself. The batch is accepted but Bob's row is dropped —
    // the security invariant holds: the foreign row is never applied.
    let db = setup_authz_db();
    insert_identity(&db, "id-mallory", "did:key:zMallory");
    insert_identity(&db, "id-bob", "did:key:zBob");
    insert_member(&db, "mem-mallory", "space-A", "id-mallory", "read");
    insert_member(&db, "mem-bob", "space-A", "id-bob", "admin");

    let ucan = make_ucan("did:key:zMallory", "space-A", Cap::Read);
    let changes = vec![change(
        "haex_space_members",
        "mem-bob",
        "identity_id",
        "100/abcd",
        json!("id-mallory"),
    )];

    let accepted = expect_accepted(authorize_inbound_sync_push(
        &db,
        "space-A",
        "endpoint-mallory",
        &ucan,
        changes,
    ));
    assert!(
        accepted.is_empty(),
        "Bob's row must be filtered out (not applied), but accepted contained: {accepted:?}",
    );
}

#[test]
fn authz_read_only_cannot_modify_foreign_member_role() {
    // role=admin on Bob's row without changing identity_id — ownership is
    // pulled from the existing DB row. Row is filtered out, not applied.
    let db = setup_authz_db();
    insert_identity(&db, "id-mallory", "did:key:zMallory");
    insert_identity(&db, "id-bob", "did:key:zBob");
    insert_member(&db, "mem-mallory", "space-A", "id-mallory", "read");
    insert_member(&db, "mem-bob", "space-A", "id-bob", "read");

    let ucan = make_ucan("did:key:zMallory", "space-A", Cap::Read);
    let changes = vec![change(
        "haex_space_members",
        "mem-bob",
        "role",
        "100/abcd",
        json!("admin"),
    )];

    let accepted = expect_accepted(authorize_inbound_sync_push(
        &db,
        "space-A",
        "endpoint-mallory",
        &ucan,
        changes,
    ));
    assert!(
        accepted.is_empty(),
        "Bob's row must be filtered out, but accepted contained: {accepted:?}",
    );
}

#[test]
fn authz_member_can_insert_own_new_membership_row() {
    let db = setup_authz_db();
    insert_identity(&db, "id-mallory", "did:key:zMallory");
    insert_member(&db, "mem-mallory-old", "space-A", "id-mallory", "read");

    let ucan = make_ucan("did:key:zMallory", "space-A", Cap::Read);
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
    // Mallory tries to forge a membership row for Bob's identity. The row is
    // filtered out so it is never applied — batch returns Accepted with 0 changes.
    let db = setup_authz_db();
    insert_identity(&db, "id-mallory", "did:key:zMallory");
    insert_identity(&db, "id-bob", "did:key:zBob");
    insert_member(&db, "mem-mallory", "space-A", "id-mallory", "read");

    let ucan = make_ucan("did:key:zMallory", "space-A", Cap::Read);
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

    let accepted = expect_accepted(authorize_inbound_sync_push(
        &db,
        "space-A",
        "endpoint-mallory",
        &ucan,
        changes,
    ));
    assert!(
        accepted.is_empty(),
        "forged row for foreign identity must be filtered out, but accepted contained: {accepted:?}",
    );
}

#[test]
fn authz_member_can_register_own_device() {
    let db = setup_authz_db();
    insert_identity(&db, "id-alice", "did:key:zAlice");
    insert_member(&db, "mem-alice", "space-A", "id-alice", "read");

    let ucan = make_ucan("did:key:zAlice", "space-A", Cap::Read);
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
            "endpoint_id",
            "100/abcd",
            json!("endpoint-alice"),
        ),
        change(
            "haex_space_devices",
            "dev-alice",
            "name",
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
    // Mallory registers a device row with Bob's endpoint_id. The row is
    // filtered out so it is never applied — batch returns Accepted with 0 changes.
    let db = setup_authz_db();
    insert_identity(&db, "id-mallory", "did:key:zMallory");
    insert_identity(&db, "id-bob", "did:key:zBob");
    insert_member(&db, "mem-mallory", "space-A", "id-mallory", "read");

    let ucan = make_ucan("did:key:zMallory", "space-A", Cap::Read);
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
            "endpoint_id",
            "100/abcd",
            json!("endpoint-bob"),
        ),
        change(
            "haex_space_devices",
            "dev-fake",
            "name",
            "100/abcd",
            json!("Pretending to be Bob"),
        ),
    ];

    let accepted = expect_accepted(authorize_inbound_sync_push(
        &db,
        "space-A",
        "endpoint-mallory",
        &ucan,
        changes,
    ));
    assert!(
        accepted.is_empty(),
        "hijacked device row must be filtered out, but accepted contained: {accepted:?}",
    );
}

#[test]
fn authz_member_cannot_modify_foreign_device_row() {
    // Existing device row belongs to Bob; Mallory tries to update its name
    // without changing endpoint_id (ownership comes from DB). Row is filtered
    // out so the modification is never applied.
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

    let ucan = make_ucan("did:key:zMallory", "space-A", Cap::Read);
    let changes = vec![change(
        "haex_space_devices",
        "dev-bob",
        "name",
        "100/abcd",
        json!("Hacked"),
    )];

    let accepted = expect_accepted(authorize_inbound_sync_push(
        &db,
        "space-A",
        "endpoint-mallory",
        &ucan,
        changes,
    ));
    assert!(
        accepted.is_empty(),
        "Bob's device row must be filtered out, but accepted contained: {accepted:?}",
    );
}

#[test]
fn authz_mixed_batch_foreign_row_filtered_own_row_accepted() {
    // Mixed batch: Mallory's own row + Bob's row. Bob's row is filtered out;
    // Mallory's row is accepted. This is the ping-pong re-push scenario —
    // the invitee received Bob's row via SyncPull and tries to push it back.
    let db = setup_authz_db();
    insert_identity(&db, "id-mallory", "did:key:zMallory");
    insert_identity(&db, "id-bob", "did:key:zBob");
    insert_member(&db, "mem-mallory", "space-A", "id-mallory", "read");
    insert_member(&db, "mem-bob", "space-A", "id-bob", "read");

    let ucan = make_ucan("did:key:zMallory", "space-A", Cap::Read);
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

    let accepted = expect_accepted(authorize_inbound_sync_push(
        &db,
        "space-A",
        "endpoint-mallory",
        &ucan,
        changes,
    ));
    assert!(
        accepted.iter().all(|c| c.row_pks.contains("mem-mallory")),
        "only Mallory's own row must be accepted, got: {accepted:?}",
    );
    assert!(
        !accepted.iter().any(|c| c.row_pks.contains("mem-bob")),
        "Bob's row must not be in accepted changes, got: {accepted:?}",
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

    let ucan = make_ucan("did:key:zAlice", "space-A", Cap::Write);
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
