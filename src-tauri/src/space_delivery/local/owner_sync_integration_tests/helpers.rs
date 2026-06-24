//! Shared fixture builders for the real-QUIC owner-sync capstone tests.
//!
//! See `mod.rs` for the overall scope/boundaries; this module hosts the
//! identity/DB/endpoint/accept-loop scaffolding used by every scenario file.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use ed25519_dalek::SigningKey;
use iroh::address_lookup::memory::MemoryLookup;
use iroh::endpoint::presets;
use iroh::{Endpoint, EndpointAddr, RelayMode, SecretKey};

use crate::crdt::trigger::ensure_crdt_columns;
use crate::database::DbConnection;
use crate::owner_sync::scope::{
    owner_route_decision, resolve_vault_owner_did, resolve_vault_space_id,
};
use crate::ucan::did_key_from_public_key;

use super::super::owner_serve::{handle_owner_pull, handle_owner_pull_columns};
use super::super::protocol::{self, Request, Response};

// ---------------------------------------------------------------------------
// Fixture builders
// ---------------------------------------------------------------------------

/// An ed25519 identity: a signing key plus the `did:key:z…` it encodes.
pub(super) struct Identity {
    pub(super) signing_key: SigningKey,
    pub(super) did: String,
}

impl Identity {
    pub(super) fn random() -> Self {
        let seed: [u8; 32] = rand::random();
        let signing_key = SigningKey::from_bytes(&seed);
        let did = did_key_from_public_key(&signing_key.verifying_key());
        Self { signing_key, did }
    }
}

/// Seed an owner-vault DB with the schema the owner-sync path reads, built on
/// top of [`super::super::test_support::init_logs_db_inner`] so the CRDT
/// bookkeeping the apply path requires (`haex_crdt_configs_no_sync`, the HLC
/// UDF + tx hooks) is present:
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
pub(super) fn seed_vault_db(
    owner: &Identity,
    own_endpoint_id: &str,
    peer_endpoint_id: &str,
    vault_space_id: &str,
) -> DbConnection {
    let (conn, _hlc) = super::super::test_support::init_logs_db_inner();

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
         );
         CREATE TABLE haex_deleted_rows (
            id TEXT PRIMARY KEY,
            table_name TEXT NOT NULL,
            row_pks TEXT NOT NULL,
            haex_hlc TEXT,
            haex_column_hlcs TEXT NOT NULL DEFAULT '{}'
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

/// Same shape as [`seed_vault_db`] EXCEPT this DB only contains its own
/// `haex_devices` row — the peer device row is intentionally omitted.
///
/// This reproduces the post-DB-copy onboarding state observed by the e2e
/// diagnostic in haex-e2e-tests PR #57: after Device B is provisioned by
/// copying A's vault DB, A's local DB has been frozen since the copy and
/// therefore still contains ONLY its own `haex_devices` row — B's row never
/// appears in A's DB unless something propagates it back. Any "B can be
/// discovered by A" assertion against this fixture targets exactly that
/// propagation step.
///
/// Identical to `seed_vault_db` for `haex_identities`, `haex_spaces`, the
/// CRDT bookkeeping, and `haex_passwords` — the only divergence is the
/// single-row `haex_devices` seed.
pub(super) fn seed_vault_db_asymmetric(
    owner: &Identity,
    own_endpoint_id: &str,
    vault_space_id: &str,
) -> DbConnection {
    let (conn, _hlc) = super::super::test_support::init_logs_db_inner();

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
         );
         CREATE TABLE haex_deleted_rows (
            id TEXT PRIMARY KEY,
            table_name TEXT NOT NULL,
            row_pks TEXT NOT NULL,
            haex_hlc TEXT,
            haex_column_hlcs TEXT NOT NULL DEFAULT '{}'
         );",
    )
    .unwrap();

    // Make haex_passwords AND haex_devices CRDT tables — haex_devices in
    // particular is the table whose rows we want to observe propagating, so
    // it must carry `haex_hlc` / `haex_column_hlcs` for the scanner.
    {
        let tx = conn.unchecked_transaction().unwrap();
        ensure_crdt_columns(&tx, "haex_passwords").unwrap();
        ensure_crdt_columns(&tx, "haex_devices").unwrap();
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
    // Asymmetric seed: ONLY this DB's own device row, with CRDT bookkeeping so
    // the scanner can pick it up if it ever gets authored on this side.
    conn.execute(
        "INSERT INTO haex_devices (endpoint_id, owner_did, haex_hlc, haex_column_hlcs)
             VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params![
            own_endpoint_id,
            owner.did,
            "1000000000000000000/aabbccdd0000",
            "{\"owner_did\":\"1000000000000000000/aabbccdd0000\"}",
        ],
    )
    .unwrap();

    DbConnection(Arc::new(Mutex::new(Some(conn))))
}

/// Insert a CRDT-tracked `haex_passwords` row, writing `haex_hlc` /
/// `haex_column_hlcs` directly (the columns the scanner reads) at a fixed HLC
/// so the row is deterministically scannable.
pub(super) fn insert_password(db: &DbConnection, id: &str, secret: &str, hlc: &str) {
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
pub(super) fn count_passwords(db: &DbConnection) -> i64 {
    let guard = db.0.lock().unwrap();
    let conn = guard.as_ref().unwrap();
    conn.query_row("SELECT COUNT(*) FROM haex_passwords", [], |r| r.get(0))
        .unwrap()
}

/// Whether a `haex_passwords` row with `id == row_id` and `secret == secret`
/// is present in `db`.
pub(super) fn has_password(db: &DbConnection, row_id: &str, secret: &str) -> bool {
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

/// Read every `endpoint_id` currently present in `haex_devices`, sorted.
///
/// Used by device-propagation tests to assert which peer rows are visible
/// post-sync.
pub(super) fn list_device_endpoint_ids(db: &DbConnection) -> Vec<String> {
    let guard = db.0.lock().unwrap();
    let conn = guard.as_ref().unwrap();
    let mut stmt = conn
        .prepare("SELECT endpoint_id FROM haex_devices ORDER BY endpoint_id")
        .unwrap();
    let rows = stmt
        .query_map([], |row| row.get::<_, String>(0))
        .unwrap();
    rows.collect::<Result<Vec<_>, _>>().unwrap()
}

/// Build a local-only iroh endpoint (RelayMode::Disabled, `haex-delivery/2`
/// ALPN) whose address book is pre-seeded with `known` peers' full
/// `EndpointAddr`s (direct addrs included, since RelayMode::Disabled), so a
/// later connect-by-id resolves them without relay/DNS.
pub(super) async fn build_endpoint(known: &[EndpointAddr]) -> Endpoint {
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
pub(super) async fn run_owner_accept_loop(endpoint: Endpoint, db: DbConnection) {
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

                if super::super::leader::send_response(&mut send, &response)
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
pub(super) async fn poll_until<F>(mut check: F) -> bool
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
