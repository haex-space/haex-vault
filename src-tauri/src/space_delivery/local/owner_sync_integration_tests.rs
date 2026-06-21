//! Real-QUIC capstone for serverless owner-vault P2P sync.
//!
//! Drives the security-critical owner-sync path end to end over a REAL iroh
//! QUIC connection — no mocks, no in-memory channels:
//!
//! - A (accept side) runs the genuine
//!   [`quic_did_auth::challenge_and_verify`] handshake, the genuine
//!   [`owner_sync::scope::owner_route_decision`] gate (fed by the genuine
//!   [`resolve_vault_owner_did`] / [`resolve_vault_space_id`] resolvers), and
//!   for an owner peer the genuine [`owner_serve::handle_owner_pull`].
//! - B / C (client side) use the genuine [`PeerSession::connect_owner`] +
//!   [`PeerSession::pull_changes`], and B applies the pulled changes through
//!   the genuine [`apply_remote_changes_to_db`].
//!
//! Only the *thin* accept-loop glue is reconstructed here. It mirrors the
//! owner branch of [`multi_leader::handle_stream`] (the owner pre-check +
//! `send_response(&handle_owner_sync_request(...))`). That production glue is
//! itself unit-tested (`owner_serve_tests.rs`, `scope.rs` tests) and
//! end-to-end covered by haex-e2e-tests; reconstructing it is the only way to
//! exercise the real pull/auth/gate trio without a `tauri::AppHandle<Wry>`,
//! which cannot be built in a headless `cargo test`.
//!
//! ## AppHandle boundary (intentionally OUT of scope here)
//!
//! `start_peer_sync_loop` (the orchestration) and
//! `owner_serve::handle_owner_push` are `AppHandle`-bound — the push path
//! advances the HLC clock via `AppState::lock_or_fail`, which needs a live
//! Tauri app. Those are deliberately NOT exercised by this test; they are
//! covered by the e2e tests in haex-e2e-tests. This capstone covers the PULL
//! direction (the one that takes `&DbConnection` and no `AppHandle`) plus the
//! full-vault-vs-foreign routing decision, which is the load-bearing security
//! assertion for serverless owner-vault sync.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use ed25519_dalek::SigningKey;
use iroh::address_lookup::memory::MemoryLookup;
use iroh::endpoint::presets;
use iroh::{Endpoint, EndpointAddr, RelayMode, SecretKey};

use crate::crdt::commands::apply_remote_changes_to_db;
use crate::crdt::hlc::HlcService;
use crate::crdt::scanner::LocalColumnChange;
use crate::crdt::trigger::ensure_crdt_columns;
use crate::database::DbConnection;
use crate::owner_sync::scope::{
    owner_route_decision, resolve_vault_owner_did, resolve_vault_space_id,
};
use crate::ucan::did_key_from_public_key;

use super::owner_serve::{handle_owner_pull, handle_owner_pull_columns};
use super::peer::PeerSession;
use super::protocol::{self, Request, Response};

// ---------------------------------------------------------------------------
// Fixture builders
// ---------------------------------------------------------------------------

/// An ed25519 identity: a signing key plus the `did:key:z…` it encodes.
struct Identity {
    signing_key: SigningKey,
    did: String,
}

impl Identity {
    fn random() -> Self {
        let seed: [u8; 32] = rand::random();
        let signing_key = SigningKey::from_bytes(&seed);
        let did = did_key_from_public_key(&signing_key.verifying_key());
        Self { signing_key, did }
    }
}

/// Seed an owner-vault DB with the schema the owner-sync path reads, built on
/// top of [`super::test_support::init_logs_db_inner`] so the CRDT bookkeeping
/// the apply path requires (`haex_crdt_configs_no_sync`, the HLC UDF + tx
/// hooks) is present:
///
/// - `haex_identities(id, did, private_key)` — the owner identity row.
/// - `haex_spaces(id, type, owner_identity_id)` — the `type='vault'` row whose
///   `owner_identity_id` points at that identity, so `resolve_vault_owner_did`
///   returns the owner DID and `resolve_vault_space_id` returns the vault id.
/// - `haex_devices(owner_did, endpoint_id)` — one row per device under the
///   owner DID. The driven code paths (`owner_route_decision`,
///   `handle_owner_pull`, `connect_owner`, `pull_changes`) never consult this
///   table, but a faithful owner-vault DB has it, and seeding it both sides
///   keeps the fixture honest (matches the anti-flake "seed haex_devices both
///   sides" guidance).
/// - `haex_passwords` — a CRDT table (`ensure_crdt_columns` adds `haex_hlc` /
///   `haex_column_hlcs`). `discover_crdt_tables` picks it up via `haex_hlc`,
///   and the apply path can write rows into it.
///
/// `vault_space_id` and the owner identity id are unique per call.
fn seed_vault_db(
    owner: &Identity,
    own_endpoint_id: &str,
    peer_endpoint_id: &str,
    vault_space_id: &str,
) -> DbConnection {
    let (conn, _hlc) = super::test_support::init_logs_db_inner();

    conn.execute_batch(
        "CREATE TABLE haex_identities (
            id TEXT PRIMARY KEY,
            did TEXT NOT NULL,
            private_key TEXT
         );
         CREATE TABLE haex_spaces (
            id TEXT PRIMARY KEY,
            type TEXT NOT NULL,
            owner_identity_id TEXT
         );
         CREATE TABLE haex_devices (
            endpoint_id TEXT PRIMARY KEY,
            owner_did TEXT NOT NULL
         );
         CREATE TABLE haex_passwords (
            id TEXT PRIMARY KEY,
            secret TEXT
         );",
    )
    .unwrap();

    // Make haex_passwords a CRDT table (adds haex_hlc + haex_column_hlcs) so it
    // is discovered by `discover_crdt_tables` and writable by the apply path.
    {
        let tx = conn.unchecked_transaction().unwrap();
        ensure_crdt_columns(&tx, "haex_passwords").unwrap();
        tx.commit().unwrap();
    }

    let identity_id = format!("identity-{}", rand::random::<u64>());
    conn.execute(
        "INSERT INTO haex_identities (id, did, private_key) VALUES (?1, ?2, NULL)",
        rusqlite::params![identity_id, owner.did],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO haex_spaces (id, type, owner_identity_id) VALUES (?1, 'vault', ?2)",
        rusqlite::params![vault_space_id, identity_id],
    )
    .unwrap();
    // Both devices live under the same owner DID.
    conn.execute(
        "INSERT INTO haex_devices (endpoint_id, owner_did) VALUES (?1, ?2)",
        rusqlite::params![own_endpoint_id, owner.did],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO haex_devices (endpoint_id, owner_did) VALUES (?1, ?2)",
        rusqlite::params![peer_endpoint_id, owner.did],
    )
    .unwrap();

    DbConnection(Arc::new(Mutex::new(Some(conn))))
}

/// Insert a CRDT-tracked `haex_passwords` row, writing `haex_hlc` /
/// `haex_column_hlcs` directly (the columns the scanner reads) at a fixed HLC
/// so the row is deterministically scannable.
fn insert_password(db: &DbConnection, id: &str, secret: &str, hlc: &str) {
    let hlcs = format!("{{\"secret\":\"{hlc}\"}}");
    let guard = db.0.lock().unwrap();
    let conn = guard.as_ref().unwrap();
    conn.execute(
        "INSERT INTO haex_passwords (id, secret, haex_hlc, haex_column_hlcs)
             VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params![id, secret, hlc, hlcs],
    )
    .unwrap();
}

/// Count `haex_passwords` rows currently in `db`.
fn count_passwords(db: &DbConnection) -> i64 {
    let guard = db.0.lock().unwrap();
    let conn = guard.as_ref().unwrap();
    conn.query_row("SELECT COUNT(*) FROM haex_passwords", [], |r| r.get(0))
        .unwrap()
}

/// Whether a `haex_passwords` row with `id == row_id` and `secret == secret`
/// is present in `db`.
fn has_password(db: &DbConnection, row_id: &str, secret: &str) -> bool {
    let guard = db.0.lock().unwrap();
    let conn = guard.as_ref().unwrap();
    conn.query_row(
        "SELECT COUNT(*) FROM haex_passwords WHERE id = ?1 AND secret = ?2",
        rusqlite::params![row_id, secret],
        |r| r.get::<_, i64>(0),
    )
    .unwrap()
        > 0
}

/// Build a local-only iroh endpoint (RelayMode::Disabled, `haex-delivery/2`
/// ALPN) whose address book is pre-seeded with `known` peers' full
/// `EndpointAddr`s (direct addrs included, since RelayMode::Disabled), so a
/// later connect-by-id resolves them without relay/DNS.
async fn build_endpoint(known: &[EndpointAddr]) -> Endpoint {
    let lookup = MemoryLookup::new();
    for addr in known {
        lookup.add_endpoint_info(addr.clone());
    }
    let secret = SecretKey::generate();
    Endpoint::builder(presets::Minimal)
        .secret_key(secret)
        .alpns(vec![protocol::ALPN.to_vec()])
        .relay_mode(RelayMode::Disabled)
        .address_lookup(lookup)
        .bind()
        .await
        .expect("bind test endpoint")
}

/// Reconstructed owner-sync accept loop for endpoint A. Mirrors the owner
/// branch of `multi_leader::handle_stream`:
///
/// 1. server-initiated `quic_did_auth` handshake on the first bi-stream,
/// 2. for each subsequent request stream, resolve the REAL owner-route gate
///    (`resolve_vault_owner_did` + `resolve_vault_space_id` + the genuine
///    `owner_route_decision`), and
/// 3. if the peer is an owner device targeting the vault space, serve via the
///    REAL `handle_owner_pull`; otherwise return `Response::Error` (the gate's
///    fall-through — never serve the full vault to a foreign peer).
///
/// Runs until the connection closes; spawned by the test and left to run.
async fn run_owner_accept_loop(endpoint: Endpoint, db: DbConnection) {
    let own_endpoint_id = endpoint.id().to_string();
    while let Some(incoming) = endpoint.accept().await {
        let conn = match incoming.await {
            Ok(c) => c,
            Err(_) => continue,
        };
        let db = DbConnection(db.0.clone());
        let own_endpoint_id = own_endpoint_id.clone();
        tokio::spawn(async move {
            let remote_str = conn.remote_id().to_string();

            // -- Phase 1: server-initiated DID-auth handshake (REAL). --
            let verified_did = match conn.open_bi().await {
                Ok((mut send, mut recv)) => {
                    match crate::quic_did_auth::challenge_and_verify(
                        &mut send,
                        &mut recv,
                        &own_endpoint_id,
                        &remote_str,
                    )
                    .await
                    {
                        Ok(did) => did,
                        Err(_) => {
                            conn.close(2u32.into(), b"did-auth failed");
                            return;
                        }
                    }
                }
                Err(_) => {
                    conn.close(2u32.into(), b"auth stream open failed");
                    return;
                }
            };

            // -- Phase 2: request loop. --
            loop {
                let (mut send, mut recv) = match conn.accept_bi().await {
                    Ok(s) => s,
                    Err(_) => break, // connection closed
                };

                let request = match protocol::read_request(&mut recv).await {
                    Ok(r) => r,
                    Err(_) => break,
                };

                // REAL owner-route gate: resolve the vault owner DID + space id
                // from this DB, then run the genuine decision function. This is
                // the exact gate `multi_leader::is_owner_vault_route` applies.
                let target_space_id = request.space_id_of().to_string();
                let resolved = crate::database::core::with_connection(&db, |conn| {
                    let owner_did = resolve_vault_owner_did(conn)?;
                    let space_id = resolve_vault_space_id(conn)?;
                    Ok((owner_did, space_id))
                });

                let is_owner_route = match resolved {
                    Ok((Some(owner_did), Some(vault_space_id))) => owner_route_decision(
                        &verified_did,
                        &target_space_id,
                        &owner_did,
                        &vault_space_id,
                    ),
                    _ => false,
                };

                let response = if is_owner_route {
                    // Owner device targeting the vault space → serve full vault
                    // via the REAL pull handler (PULL path takes `&DbConnection`,
                    // no AppHandle).
                    match request {
                        Request::SyncPull {
                            after_timestamp, ..
                        } => handle_owner_pull(after_timestamp.as_deref(), &db),
                        Request::SyncPullColumns { columns, .. } => {
                            handle_owner_pull_columns(&columns, &db)
                        }
                        // Push is AppHandle-bound and out of scope here.
                        _ => Response::Error {
                            message:
                                "owner-sync accept loop: only SyncPull/SyncPullColumns is driven \
                                 in this test"
                                    .to_string(),
                        },
                    }
                } else {
                    // Foreign peer: mirror the gate's fall-through. We do NOT
                    // call handle_owner_pull — the full vault must never reach a
                    // non-owner peer. (No space leader is registered in this
                    // test, so the production path would also error here.)
                    Response::Error {
                        message: format!("No leader active for space {target_space_id}"),
                    }
                };

                if super::leader::send_response(&mut send, &response)
                    .await
                    .is_err()
                {
                    break;
                }
            }
        });
    }
}

/// Bounded-retry poll: re-run `check` in small steps up to ~5s, returning as
/// soon as it holds. Asserts on the FINAL state, never on a transient count.
async fn poll_until<F>(mut check: F) -> bool
where
    F: FnMut() -> bool,
{
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        if check() {
            return true;
        }
        if std::time::Instant::now() >= deadline {
            return false;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

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

    let (changes_json, _has_more) = session
        .pull_changes(&vault_space_id, None)
        .await
        .expect("B pull_changes");

    // Apply on B via the REAL apply path (HlcService built directly — no
    // AppHandle), mirroring sync_loop's pull-apply.
    let remote_locals: Vec<LocalColumnChange> =
        serde_json::from_value(changes_json).expect("deserialize pulled changes");
    let remote_changes: Vec<_> = remote_locals
        .iter()
        .map(super::sync_loop::local_to_remote_change)
        .collect();
    let hlc_b = HlcService::new_for_testing("device-b");
    apply_remote_changes_to_db(&db_b, remote_changes, None, Some(&hlc_b))
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
