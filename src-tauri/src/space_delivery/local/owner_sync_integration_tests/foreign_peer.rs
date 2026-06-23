//! Foreign-peer leak-guard scenarios over real QUIC (load-bearing security):
//! a non-owner DID must never receive vault rows, neither via SyncPull nor via
//! SyncPullColumns.

use crate::crdt::scanner::LocalColumnChange;
use crate::database::DbConnection;

use super::super::peer::PeerSession;
use super::helpers::{
    build_endpoint, count_passwords, insert_password, run_owner_accept_loop, seed_vault_db,
    Identity,
};

// ---------------------------------------------------------------------------
// Test 2 — foreign-peer leak guard over real QUIC (load-bearing security)
// ---------------------------------------------------------------------------

/// C has a DIFFERENT owner DID. It connects to A, passes its OWN real DID-auth,
/// and issues `SyncPull` at A's vault space id. A's REAL `owner_route_decision`
/// classifies C as `Foreign`, so A does NOT call `handle_owner_pull` and
/// returns an error. C must receive ZERO `haex_passwords` rows — the full vault
/// must never leak to a non-owner peer.
#[tokio::test]
async fn foreign_peer_gets_zero_vault_rows_over_real_quic() {
    let owner = Identity::random();
    let foreign = Identity::random();
    assert_ne!(
        owner.did, foreign.did,
        "owner and foreign DIDs must differ for this test to mean anything"
    );

    let a_ep = build_endpoint(&[]).await;
    let a_endpoint_id = a_ep.id().to_string();
    let a_addr = a_ep.addr();

    // C is a real peer with its own endpoint; it knows A's addr.
    let c_ep = build_endpoint(&[a_addr]).await;
    let c_endpoint_id = c_ep.id().to_string();

    let vault_space_id = format!("vault-{}", rand::random::<u64>());

    // A's DB: owner-owned vault + a secret password row. (haex_devices lists
    // A and some other owner device; C is NOT an owner device.)
    let some_owner_device = format!("ep-{}", rand::random::<u64>());
    let db_a = seed_vault_db(&owner, &a_endpoint_id, &some_owner_device, &vault_space_id);
    let row_id = format!("pw-{}", rand::random::<u64>());
    let secret = format!("top-secret-{}", rand::random::<u64>());
    insert_password(&db_a, &row_id, &secret, "3000000000000000000/ccddeeff0022");
    assert_eq!(count_passwords(&db_a), 1, "A holds exactly one password");

    let accept_task = tokio::spawn(run_owner_accept_loop(a_ep, DbConnection(db_a.0.clone())));

    // C connects as ITSELF (foreign DID) and passes its own real DID-auth.
    let session = PeerSession::connect_owner(
        &c_ep,
        &a_endpoint_id,
        None,
        &foreign.did,
        &foreign.signing_key,
        &c_endpoint_id,
    )
    .await
    .expect("C → A connect_owner (foreign DID still passes the handshake)");

    // C issues SyncPull at A's vault space id. A's REAL owner_route_decision
    // classifies C as Foreign → NOT served → error, NOT SyncChanges.
    let pull_result = session.pull_changes(&vault_space_id, None).await;

    // The load-bearing negative assertion: C receives ZERO haex_passwords
    // rows. `pull_changes` only returns Ok on a `SyncChanges` response; the
    // foreign fall-through is `Response::Error`, so the call must be Err. We
    // additionally prove that, however the bytes are interpreted, no password
    // row crosses the wire.
    match pull_result {
        Err(_) => { /* expected: foreign peer is rejected, no vault served */ }
        Ok((changes_json, _has_more)) => {
            // Defense-in-depth: even if a future change made the fall-through
            // return an (empty) SyncChanges, assert there are zero password
            // rows in whatever was sent.
            let locals: Vec<LocalColumnChange> =
                serde_json::from_value(changes_json).unwrap_or_default();
            let password_rows = locals
                .iter()
                .filter(|c| c.table_name == "haex_passwords")
                .count();
            assert_eq!(
                password_rows, 0,
                "foreign peer must receive ZERO haex_passwords rows; got {password_rows}"
            );
            assert!(
                locals.is_empty(),
                "foreign peer must receive ZERO vault-private rows at all; got {} rows",
                locals.len()
            );
        }
    }

    session.close();
    accept_task.abort();
}

// ---------------------------------------------------------------------------
// Test 2b — foreign-peer column-dump leak guard over real QUIC
// ---------------------------------------------------------------------------

/// Like `foreign_peer_gets_zero_vault_rows_over_real_quic`, but the foreign
/// peer issues `SyncPullColumns` instead of `SyncPull`. The owner route now
/// serves `SyncPullColumns` (so the rejection is the GATE's doing, not the
/// loop's inability to handle columns): C has a DIFFERENT owner DID, passes its
/// OWN real DID-auth, and requests `(haex_passwords, secret)` at A's vault space
/// id. A's REAL `owner_route_decision` classifies C as `Foreign`, so A does NOT
/// call `handle_owner_pull_columns` and returns an error. C must receive ZERO
/// `haex_passwords` rows — the full-vault column dump must never leak to a
/// non-owner peer.
#[tokio::test]
async fn foreign_peer_sync_pull_columns_is_not_served_full_vault() {
    let owner = Identity::random();
    let foreign = Identity::random();
    assert_ne!(
        owner.did, foreign.did,
        "owner and foreign DIDs must differ for this test to mean anything"
    );

    let a_ep = build_endpoint(&[]).await;
    let a_endpoint_id = a_ep.id().to_string();
    let a_addr = a_ep.addr();

    // C is a real peer with its own endpoint; it knows A's addr.
    let c_ep = build_endpoint(&[a_addr]).await;
    let c_endpoint_id = c_ep.id().to_string();

    let vault_space_id = format!("vault-{}", rand::random::<u64>());

    // A's DB: owner-owned vault + a secret password row. (haex_devices lists
    // A and some other owner device; C is NOT an owner device.)
    let some_owner_device = format!("ep-{}", rand::random::<u64>());
    let db_a = seed_vault_db(&owner, &a_endpoint_id, &some_owner_device, &vault_space_id);
    let row_id = format!("pw-{}", rand::random::<u64>());
    let secret = format!("top-secret-{}", rand::random::<u64>());
    insert_password(&db_a, &row_id, &secret, "3000000000000000000/ccddeeff0022");
    assert_eq!(count_passwords(&db_a), 1, "A holds exactly one password");

    let accept_task = tokio::spawn(run_owner_accept_loop(a_ep, DbConnection(db_a.0.clone())));

    // C connects as ITSELF (foreign DID) and passes its own real DID-auth.
    let session = PeerSession::connect_owner(
        &c_ep,
        &a_endpoint_id,
        None,
        &foreign.did,
        &foreign.signing_key,
        &c_endpoint_id,
    )
    .await
    .expect("C → A connect_owner (foreign DID still passes the handshake)");

    // C issues SyncPullColumns at A's vault space id. A's REAL
    // owner_route_decision classifies C as Foreign → NOT served → error, NOT
    // SyncChanges.
    let pull_result = session
        .pull_columns(
            &vault_space_id,
            &[("haex_passwords".to_string(), "secret".to_string())],
        )
        .await;

    // The load-bearing negative assertion: C receives ZERO haex_passwords
    // rows. `pull_columns` only returns Ok on a `SyncChanges` response; the
    // foreign fall-through is `Response::Error`, so the call must be Err. We
    // additionally prove that, however the bytes are interpreted, no password
    // row crosses the wire.
    match pull_result {
        Err(_) => { /* expected: foreign peer is rejected, no column dump served */ }
        Ok(changes_json) => {
            // Defense-in-depth: even if a future change made the fall-through
            // return an (empty) SyncChanges, assert there are zero password
            // rows in whatever was sent.
            let locals: Vec<LocalColumnChange> =
                serde_json::from_value(changes_json).unwrap_or_default();
            let password_rows = locals
                .iter()
                .filter(|c| c.table_name == "haex_passwords")
                .count();
            assert_eq!(
                password_rows, 0,
                "foreign peer must receive ZERO haex_passwords rows; got {password_rows}"
            );
            assert!(
                locals.is_empty(),
                "foreign peer must receive ZERO vault-private rows at all; got {} rows",
                locals.len()
            );
        }
    }

    session.close();
    accept_task.abort();
}
