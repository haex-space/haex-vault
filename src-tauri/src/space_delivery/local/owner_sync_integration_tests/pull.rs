//! Owner-device PULL paths over real QUIC (full-vault SyncPull and column
//! SyncPullColumns).

use crate::crdt::commands::apply_remote_changes_to_db_scoped;
use crate::crdt::hlc::HlcService;
use crate::crdt::scanner::LocalColumnChange;
use crate::database::DbConnection;

use super::super::peer::PeerSession;
use super::helpers::{
    build_endpoint, count_passwords, delete_password, has_password, insert_password, poll_until,
    read_password_secret, run_owner_accept_loop, seed_vault_db, update_password_secret, Identity,
};

// ---------------------------------------------------------------------------
// Test 1 — convergence over real QUIC (B pulls A's full vault)
// ---------------------------------------------------------------------------

/// A and B share the same owner DID + vault space. A holds a `haex_passwords`
/// row. B connects via the REAL `connect_owner`, pulls via the REAL
/// `pull_changes`, and applies via the REAL `apply_remote_changes_to_db`.
/// The password row must converge onto B.
#[tokio::test]
async fn owner_device_pulls_full_vault_over_real_quic() {
    let owner = Identity::random();

    // Bring up A first so B can be seeded with A's full addr.
    let a_ep = build_endpoint(&[]).await;
    let a_endpoint_id = a_ep.id().to_string();

    // B's identity IS the owner identity (owner-mesh: B signs DID-auth as the
    // owner). B's address book is seeded with A's full addr so B's
    // connect-by-id resolves A's direct addresses (RelayMode::Disabled, no DNS).
    let a_addr = a_ep.addr();
    let b_ep = build_endpoint(&[a_addr]).await;
    let b_endpoint_id = b_ep.id().to_string();

    let vault_space_id = format!("vault-{}", rand::random::<u64>());

    // A's DB: owner identity, vault space, both devices, and the password row.
    let db_a = seed_vault_db(&owner, &a_endpoint_id, &b_endpoint_id, &vault_space_id);
    let row_id = format!("pw-{}", rand::random::<u64>());
    let secret = format!("s3cr3t-{}", rand::random::<u64>());
    insert_password(&db_a, &row_id, &secret, "2000000000000000000/aabbccdd0011");

    // B's DB: same owner + vault space + both devices, but NO password yet.
    let db_b = seed_vault_db(&owner, &b_endpoint_id, &a_endpoint_id, &vault_space_id);
    assert_eq!(count_passwords(&db_b), 0, "B starts with no passwords");

    // Start A's reconstructed accept loop.
    let accept_task = tokio::spawn(run_owner_accept_loop(a_ep, DbConnection(db_a.0.clone())));

    // B connects as the owner (REAL connect_owner) + pulls (REAL pull_changes).
    let session = PeerSession::connect_owner(
        &b_ep,
        &a_endpoint_id,
        None,
        &owner.did,
        &owner.signing_key,
        &b_endpoint_id,
    )
    .await
    .expect("B → A connect_owner");

    let (changes_json, has_more) = session
        .pull_changes(&vault_space_id, None)
        .await
        .expect("B pull_changes");
    // This fixture's full vault fits in one page; assert that explicitly so
    // any later fixture growth past the page budget makes this test fail
    // loudly instead of silently testing only the first page.
    assert!(
        !has_more,
        "expected single-page pull; streaming-apply tests live in dedicated coverage"
    );

    // Apply on B via the REAL apply path (HlcService built directly — no
    // AppHandle), mirroring sync_loop's pull-apply. Production
    // `run_pull_phase` calls the SCOPED apply with `Some(space_id)`, so this
    // test mirrors that — a scoped apply on the owner-space route must accept
    // the unsigned owner-vault dump (see is_owner_space in owner_sync::scope).
    let remote_locals: Vec<LocalColumnChange> =
        serde_json::from_value(changes_json).expect("deserialize pulled changes");
    let remote_changes: Vec<_> = remote_locals
        .iter()
        .map(super::super::sync_loop::local_to_remote_change)
        .collect();
    let hlc_b = HlcService::new_for_testing("device-b");
    apply_remote_changes_to_db_scoped(
        &db_b,
        remote_changes,
        None,
        Some(&hlc_b),
        Some(&vault_space_id),
    )
    .expect("apply remote changes on B");

    // FINAL-STATE assertion via bounded-retry poll (apply is synchronous, so
    // this converges immediately, but we poll to stay race-free).
    let converged = poll_until(|| has_password(&db_b, &row_id, &secret)).await;
    assert!(
        converged,
        "B must have A's haex_passwords row after owner-vault pull+apply"
    );

    session.close();
    accept_task.abort();
}

// ---------------------------------------------------------------------------
// Test 2 — delete propagates after the initial sync
// ---------------------------------------------------------------------------

/// Regression for the P2P owner-sync delete convergence e2e failure
/// (`tests/sync/owner-sync-delete-convergence.spec.ts`): A and B initially
/// hold the same `haex_passwords` row; A then deletes it (writes a
/// `haex_deleted_rows` entry and removes the row). B pulls A's vault via
/// real QUIC and applies via `apply_remote_changes_to_db_scoped` (mirroring
/// `run_pull_phase`). The row must be gone on B after apply.
///
/// The delete-log entry is a normal CRDT-synced row, so under the pre-fix
/// Phase-1 gate the scoped apply dropped its unsigned columns (owner-vault
/// sync leaves `sig` absent by design — see
/// `sign_column_for_spaces`/`RegisterLookup`). Nothing landed in B's
/// `haex_deleted_rows`, `propagate_deleted_rows_to_target_tables` silently
/// skipped, and the password stayed on B forever.
#[tokio::test]
async fn owner_sync_propagates_delete_after_initial_sync() {
    let owner = Identity::random();

    let a_ep = build_endpoint(&[]).await;
    let a_endpoint_id = a_ep.id().to_string();
    let a_addr = a_ep.addr();
    let b_ep = build_endpoint(&[a_addr]).await;
    let b_endpoint_id = b_ep.id().to_string();

    let vault_space_id = format!("vault-{}", rand::random::<u64>());

    // Both A and B start with the SAME password (already-converged initial
    // state).
    let db_a = seed_vault_db(&owner, &a_endpoint_id, &b_endpoint_id, &vault_space_id);
    let db_b = seed_vault_db(&owner, &b_endpoint_id, &a_endpoint_id, &vault_space_id);
    let row_id = format!("pw-{}", rand::random::<u64>());
    let secret = format!("s3cr3t-{}", rand::random::<u64>());
    let initial_hlc = "1000000000000000000/aabbccdd0011";
    let delete_hlc = "2000000000000000000/aabbccdd0011";
    insert_password(&db_a, &row_id, &secret, initial_hlc);
    insert_password(&db_b, &row_id, &secret, initial_hlc);
    assert!(
        has_password(&db_b, &row_id, &secret),
        "B must start with the password (already-synced state)"
    );

    // A deletes the row.
    let delete_log_id = format!("del-{}", rand::random::<u64>());
    delete_password(&db_a, &row_id, &delete_log_id, delete_hlc);
    assert_eq!(
        count_passwords(&db_a),
        0,
        "A must have removed the password locally"
    );

    // B pulls A's vault (real QUIC) and applies scoped, mirroring production.
    let accept_task = tokio::spawn(run_owner_accept_loop(a_ep, DbConnection(db_a.0.clone())));
    let session = PeerSession::connect_owner(
        &b_ep,
        &a_endpoint_id,
        None,
        &owner.did,
        &owner.signing_key,
        &b_endpoint_id,
    )
    .await
    .expect("B → A connect_owner");

    let (changes_json, has_more) = session
        .pull_changes(&vault_space_id, None)
        .await
        .expect("B pull_changes");
    assert!(
        !has_more,
        "expected single-page pull; streaming-apply tests live in dedicated coverage"
    );

    let remote_locals: Vec<LocalColumnChange> =
        serde_json::from_value(changes_json).expect("deserialize pulled changes");
    let remote_changes: Vec<_> = remote_locals
        .iter()
        .map(super::super::sync_loop::local_to_remote_change)
        .collect();
    let hlc_b = HlcService::new_for_testing("device-b");
    apply_remote_changes_to_db_scoped(
        &db_b,
        remote_changes,
        None,
        Some(&hlc_b),
        Some(&vault_space_id),
    )
    .expect("apply remote changes on B");

    // FINAL-STATE: B must have converged — the password is gone.
    let converged = poll_until(|| count_passwords(&db_b) == 0).await;
    assert!(
        converged,
        "B must have removed the password after applying A's delete-log entry"
    );

    session.close();
    accept_task.abort();
}

// ---------------------------------------------------------------------------
// Test 3 — column update propagates after the initial sync
// ---------------------------------------------------------------------------

/// Sibling of the delete test: A and B initially hold the same
/// `haex_passwords` row; A then rotates the secret. B pulls A's vault via
/// real QUIC and applies via `apply_remote_changes_to_db_scoped`. The new
/// secret must land on B.
///
/// Same failure mode as the delete case under the pre-fix gate: the column
/// change carries `sig = None` (owner-vault sync writes are unsigned) and
/// `expected_space_id = Some(vault_space_id)`, so the strict Phase-1 gate
/// dropped it and B never saw the new secret.
#[tokio::test]
async fn owner_sync_propagates_update_after_initial_sync() {
    let owner = Identity::random();

    let a_ep = build_endpoint(&[]).await;
    let a_endpoint_id = a_ep.id().to_string();
    let a_addr = a_ep.addr();
    let b_ep = build_endpoint(&[a_addr]).await;
    let b_endpoint_id = b_ep.id().to_string();

    let vault_space_id = format!("vault-{}", rand::random::<u64>());

    // Same initial state on A and B.
    let db_a = seed_vault_db(&owner, &a_endpoint_id, &b_endpoint_id, &vault_space_id);
    let db_b = seed_vault_db(&owner, &b_endpoint_id, &a_endpoint_id, &vault_space_id);
    let row_id = format!("pw-{}", rand::random::<u64>());
    let old_secret = format!("old-{}", rand::random::<u64>());
    let new_secret = format!("new-{}", rand::random::<u64>());
    let initial_hlc = "1000000000000000000/aabbccdd0011";
    let update_hlc = "2000000000000000000/aabbccdd0011";
    insert_password(&db_a, &row_id, &old_secret, initial_hlc);
    insert_password(&db_b, &row_id, &old_secret, initial_hlc);

    // A rotates the secret.
    update_password_secret(&db_a, &row_id, &new_secret, update_hlc);
    assert_eq!(
        read_password_secret(&db_a, &row_id).as_deref(),
        Some(new_secret.as_str()),
        "A must have the new secret locally"
    );

    let accept_task = tokio::spawn(run_owner_accept_loop(a_ep, DbConnection(db_a.0.clone())));
    let session = PeerSession::connect_owner(
        &b_ep,
        &a_endpoint_id,
        None,
        &owner.did,
        &owner.signing_key,
        &b_endpoint_id,
    )
    .await
    .expect("B → A connect_owner");

    let (changes_json, has_more) = session
        .pull_changes(&vault_space_id, None)
        .await
        .expect("B pull_changes");
    assert!(
        !has_more,
        "expected single-page pull; streaming-apply tests live in dedicated coverage"
    );

    let remote_locals: Vec<LocalColumnChange> =
        serde_json::from_value(changes_json).expect("deserialize pulled changes");
    let remote_changes: Vec<_> = remote_locals
        .iter()
        .map(super::super::sync_loop::local_to_remote_change)
        .collect();
    let hlc_b = HlcService::new_for_testing("device-b");
    apply_remote_changes_to_db_scoped(
        &db_b,
        remote_changes,
        None,
        Some(&hlc_b),
        Some(&vault_space_id),
    )
    .expect("apply remote changes on B");

    // FINAL-STATE: B must have converged on the new secret.
    let converged =
        poll_until(|| read_password_secret(&db_b, &row_id).as_deref() == Some(new_secret.as_str()))
            .await;
    assert!(
        converged,
        "B must have the new secret after applying A's UPDATE (got {:?})",
        read_password_secret(&db_b, &row_id)
    );

    session.close();
    accept_task.abort();
}

// ---------------------------------------------------------------------------
// Test 1b — column recovery over real QUIC (B pulls A's column dump)
// ---------------------------------------------------------------------------

/// A and B share the same owner DID + vault space. A holds a `haex_passwords`
/// row with a known `secret`. B connects via the REAL `connect_owner`, then
/// pulls the `(haex_passwords, secret)` column via the REAL `pull_columns`.
/// The owner route serves `SyncPullColumns` over real QUIC and B receives a
/// dump containing A's secret value. (Asserting on the served dump proves the
/// owner route served `SyncPullColumns`; applying is not required here.)
#[tokio::test]
async fn owner_device_pulls_columns_over_real_quic() {
    let owner = Identity::random();

    // Bring up A first so B can be seeded with A's full addr.
    let a_ep = build_endpoint(&[]).await;
    let a_endpoint_id = a_ep.id().to_string();

    // B's identity IS the owner identity (owner-mesh). B's address book is
    // seeded with A's full addr so connect-by-id resolves A's direct addresses.
    let a_addr = a_ep.addr();
    let b_ep = build_endpoint(&[a_addr]).await;
    let b_endpoint_id = b_ep.id().to_string();

    let vault_space_id = format!("vault-{}", rand::random::<u64>());

    // A's DB: owner identity, vault space, both devices, and the password row.
    let db_a = seed_vault_db(&owner, &a_endpoint_id, &b_endpoint_id, &vault_space_id);
    let row_id = format!("pw-{}", rand::random::<u64>());
    let secret = format!("s3cr3t-{}", rand::random::<u64>());
    insert_password(&db_a, &row_id, &secret, "2000000000000000000/aabbccdd0011");

    // B's DB: same owner + vault space + both devices, but NO password yet.
    let db_b = seed_vault_db(&owner, &b_endpoint_id, &a_endpoint_id, &vault_space_id);
    assert_eq!(count_passwords(&db_b), 0, "B starts with no passwords");

    // Start A's reconstructed accept loop.
    let accept_task = tokio::spawn(run_owner_accept_loop(a_ep, DbConnection(db_a.0.clone())));

    // B connects as the owner (REAL connect_owner) + pulls the column dump
    // (REAL pull_columns).
    let session = PeerSession::connect_owner(
        &b_ep,
        &a_endpoint_id,
        None,
        &owner.did,
        &owner.signing_key,
        &b_endpoint_id,
    )
    .await
    .expect("B → A connect_owner");

    let changes_json = session
        .pull_columns(
            &vault_space_id,
            &[("haex_passwords".to_string(), "secret".to_string())],
        )
        .await
        .expect("B pull_columns");

    // The owner route served SyncPullColumns over real QUIC: the dump must
    // contain A's seeded secret for (haex_passwords, secret).
    let locals: Vec<LocalColumnChange> =
        serde_json::from_value(changes_json).expect("deserialize pulled column dump");
    let served = locals.iter().any(|c| {
        c.table_name == "haex_passwords"
            && c.column_name == "secret"
            && c.value.as_str() == Some(secret.as_str())
    });
    assert!(
        served,
        "owner device must receive A's (haex_passwords, secret) value over SyncPullColumns; \
         got {locals:?}"
    );

    session.close();
    accept_task.abort();
}
