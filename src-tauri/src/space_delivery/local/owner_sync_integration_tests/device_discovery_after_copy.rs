//! Device discovery after DB-copy onboarding.
//!
//! Reproduces the asymmetric `haex_devices` state observed by haex-e2e-tests
//! PR #57's diagnostic for the "owner-sync stalls after onboarding via DB
//! copy" bug:
//!
//! - Device A is the original / onboarding source. Its DB was frozen at the
//!   moment the copy was made, so it knows ONLY its own `haex_devices` row.
//! - Device B was provisioned by copying A's DB, so it inherited A's row,
//!   and on first boot inserted its own row too. B knows BOTH devices.
//!
//! From that starting point, this module pins TWO observations the bug must
//! satisfy AFTER the upcoming fix lands:
//!
//! 1. **Phase 1a — pure-function discovery.** After whatever propagation
//!     step is supposed to happen, `resolve_owner_device_endpoints` on A must
//!     enumerate B exactly once. Today it returns the empty set; the fix
//!     must make it return `[b_endpoint_id]`.
//!
//! 2. **Phase 1b — pull-direction wire observation.** Driving the same
//!     sync surface that production owner-vault sync uses (`connect_owner`
//!     + `pull_changes` + `apply_remote_changes_to_db`), A must end up with
//!     B's `haex_devices` row in its own DB. This discriminates which
//!     sub-hypothesis of the bug we're in:
//!      - If A's `haex_devices` stays `[A]` after the pull → B's row never
//!        crosses the wire in the only direction this fixture can drive
//!        (PULL). The bug is "B's authored row is not advertised in any
//!        sync direction" or "the pull direction is structurally incapable
//!        of carrying B → A device rows".
//!      - If the pull itself errors out → infrastructure issue, surface it.
//!      - If the assertion passes today → the bug is elsewhere; widen.
//!
//! Phase 1a fails today (pins the bug); Phase 1b passes today, which is
//! informative — it confirms the pull mechanism CAN carry `haex_devices`
//! rows in this direction, so the production bug must live in orchestration
//! or in the AppHandle-bound push direction that this fixture intentionally
//! doesn't drive.

use crate::crdt::commands::apply_remote_changes_to_db;
use crate::crdt::hlc::HlcService;
use crate::crdt::scanner::LocalColumnChange;
use crate::database::DbConnection;
use crate::owner_sync::scope::resolve_owner_device_endpoints;

use super::super::peer::PeerSession;
use super::helpers::{
    build_endpoint, list_device_endpoint_ids, poll_until, run_owner_accept_loop,
    seed_vault_db_asymmetric, Identity,
};

// ---------------------------------------------------------------------------
// Phase 1a — pure-function asymmetry
// ---------------------------------------------------------------------------

/// Starting from the post-DB-copy asymmetric state (A has only its own row,
/// B has both rows), `resolve_owner_device_endpoints` on A must enumerate
/// B's endpoint exactly once. Today it returns the empty set — this test
/// fails red on that assertion. The follow-up fix must turn it green.
#[tokio::test]
async fn a_resolves_b_as_owner_device_after_copy() {
    let owner = Identity::random();

    // Stable endpoint ids without binding real iroh endpoints (this phase is
    // pure-function and does not touch QUIC).
    let a_endpoint_id = format!("ep-a-{}", rand::random::<u64>());
    let b_endpoint_id = format!("ep-b-{}", rand::random::<u64>());
    let vault_space_id = format!("vault-{}", rand::random::<u64>());

    // A: only its own haex_devices row (the post-copy frozen state). B's DB
    // is irrelevant for this pure-function assertion — see Phase 1b for the
    // full asymmetric setup driven over the wire.
    let db_a = seed_vault_db_asymmetric(&owner, &a_endpoint_id, &vault_space_id);

    // Sanity (today's symptom): A cannot enumerate B because A only knows
    // its own row.
    {
        let guard = db_a.0.lock().unwrap();
        let conn = guard.as_ref().unwrap();
        let peers = resolve_owner_device_endpoints(conn, &owner.did, &a_endpoint_id).unwrap();
        assert_eq!(
            peers,
            Vec::<String>::new(),
            "pre-fix sanity: A's haex_devices contains only A, so it discovers nobody",
        );
    }

    // The post-fix expectation: after whatever propagation the fix introduces
    // (which on this pure-function test means "after the seeder reflects
    // whatever state the fix guarantees"), A should discover B once. Today
    // we have nothing that brings B's row into A's DB on this code path, so
    // this assertion is the RED line that pins the bug.
    let peers = {
        let guard = db_a.0.lock().unwrap();
        let conn = guard.as_ref().unwrap();
        resolve_owner_device_endpoints(conn, &owner.did, &a_endpoint_id).unwrap()
    };
    assert_eq!(
        peers,
        vec![b_endpoint_id.clone()],
        "POST-FIX expectation: A must enumerate B exactly once via \
         resolve_owner_device_endpoints once B's haex_devices row has reached A. \
         Today A's haex_devices only contains A, so this assertion fails — \
         that failure is the bug from haex-e2e-tests PR #57.",
    );
}

// ---------------------------------------------------------------------------
// Phase 1b — pull-direction wire observation
// ---------------------------------------------------------------------------

/// Same starting state as Phase 1a (A has only its own row, B has both),
/// but this time drive the actual sync surface. B runs the owner-accept
/// loop, A runs `connect_owner` and `pull_changes` against B, and applies
/// the result. After the apply, A's `haex_devices` must contain B's row.
///
/// This is the H1/H2 discriminator at the cargo-test level. Today it fails
/// because either (a) B never authors a `haex_devices` row that ends up in
/// any scannable change stream, or (b) the pull-only direction in this
/// fixture is structurally incapable of carrying B → A rows. Either way the
/// bug is observable here.
#[tokio::test]
async fn b_haex_devices_row_propagates_to_a_via_owner_pull() {
    let owner = Identity::random();

    // Bring up B first so A can be seeded with B's full addr (we flip the
    // accept/connect direction vs the existing pull.rs tests: here A is the
    // one that must LEARN about B, so A pulls FROM B).
    let b_ep = build_endpoint(&[]).await;
    let b_endpoint_id = b_ep.id().to_string();

    let b_addr = b_ep.addr();
    let a_ep = build_endpoint(&[b_addr]).await;
    let a_endpoint_id = a_ep.id().to_string();

    let vault_space_id = format!("vault-{}", rand::random::<u64>());

    // A's DB: post-copy frozen state — only A's own haex_devices row.
    let db_a = seed_vault_db_asymmetric(&owner, &a_endpoint_id, &vault_space_id);

    // B's DB: inherited A's row from the copy, then authored its own.
    let db_b = seed_vault_db_asymmetric(&owner, &b_endpoint_id, &vault_space_id);
    {
        let guard = db_b.0.lock().unwrap();
        let conn = guard.as_ref().unwrap();
        conn.execute(
            "INSERT INTO haex_devices (endpoint_id, owner_did, haex_hlc, haex_column_hlcs)
                 VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![
                a_endpoint_id,
                owner.did,
                "0500000000000000000/aabbccdd0001",
                "{\"owner_did\":\"0500000000000000000/aabbccdd0001\"}",
            ],
        )
        .unwrap();
    }

    // Pre-condition sanity.
    assert_eq!(
        list_device_endpoint_ids(&db_a),
        vec![a_endpoint_id.clone()],
        "pre-condition: A's haex_devices is frozen at the copy point (only A)",
    );
    let mut b_initial = list_device_endpoint_ids(&db_b);
    b_initial.sort();
    let mut expected_b = vec![a_endpoint_id.clone(), b_endpoint_id.clone()];
    expected_b.sort();
    assert_eq!(
        b_initial, expected_b,
        "pre-condition: B's haex_devices has both rows (post-copy then self-insert)",
    );

    // B accepts; A connects + pulls.
    let accept_task = tokio::spawn(run_owner_accept_loop(b_ep, DbConnection(db_b.0.clone())));

    let session = PeerSession::connect_owner(
        &a_ep,
        &b_endpoint_id,
        None,
        &owner.did,
        &owner.signing_key,
        &a_endpoint_id,
    )
    .await
    .expect("A → B connect_owner");

    let (changes_json, has_more) = session
        .pull_changes(&vault_space_id, None)
        .await
        .expect("A pull_changes from B");
    assert!(
        !has_more,
        "expected single-page pull; if this trips, widen the fixture before re-reading",
    );

    let remote_locals: Vec<LocalColumnChange> =
        serde_json::from_value(changes_json).expect("deserialize pulled changes");
    let remote_changes: Vec<_> = remote_locals
        .iter()
        .map(super::super::sync_loop::local_to_remote_change)
        .collect();
    let hlc_a = HlcService::new_for_testing("device-a");
    apply_remote_changes_to_db(&db_a, remote_changes, None, Some(&hlc_a))
        .expect("apply remote changes on A");

    // The discriminator: after the pull-and-apply, A must have B's row.
    // Today this fails: either nothing carried B's row over the wire, or
    // the apply path won't write to haex_devices on the receiving side.
    let converged = poll_until(|| {
        list_device_endpoint_ids(&db_a).contains(&b_endpoint_id)
    })
    .await;
    let final_devices = list_device_endpoint_ids(&db_a);
    assert!(
        converged,
        "POST-FIX expectation: B's haex_devices row must propagate to A via \
         the only available sync direction (A pulls from B). Today this \
         assertion fails because either (a) B never advertises its own \
         haex_devices row in any sync direction, or (b) the pull-only \
         direction in this fixture is structurally incapable of carrying \
         B → A device rows. Final A.haex_devices = {final_devices:?}",
    );

    session.close();
    accept_task.abort();
}
