//! Wiring tests for the vault-open path.
//!
//! `SpaceKeyCache` unit behavior lives in `crdt::column_sig::key_cache_tests` —
//! this file exercises the helper that plugs the cache into the AppState's
//! mounted `DbConnection` (Task C3 of the shared-space Phase 1 column-sigs
//! plan).

use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use ed25519_dalek::SigningKey;
use rusqlite::Connection;
use std::sync::{Arc, Mutex};

use super::populate_column_sig_key_cache;
use crate::crdt::column_sig::key_cache::SpaceKeyCache;
use crate::database::DbConnection;

fn random_key() -> SigningKey {
    let seed: [u8; 32] = rand::random();
    SigningKey::from_bytes(&seed)
}

fn pkcs8_b64(key: &SigningKey) -> String {
    // Fixed 16-byte PKCS8-Ed25519 DER prefix, then 32 seed bytes.
    let pkcs8_prefix: [u8; 16] = [
        0x30, 0x2e, 0x02, 0x01, 0x00, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x04, 0x22, 0x04,
        0x20,
    ];
    let mut der = Vec::with_capacity(48);
    der.extend_from_slice(&pkcs8_prefix);
    der.extend_from_slice(&key.to_bytes());
    BASE64.encode(&der)
}

/// Minimal DB fixture with one owned space (`space_A`) — schema mirrors the
/// two columns `SQL_SELECT_ALL_OWN_SPACE_KEYS` reads. Wrapped in the
/// `DbConnection` newtype so the helper sees the same shape as production.
fn open_db_with_one_owned_space() -> (DbConnection, SigningKey) {
    let conn = Connection::open_in_memory().expect("open in-memory");
    conn.execute_batch(
        "CREATE TABLE haex_identities (
            id TEXT PRIMARY KEY NOT NULL,
            did TEXT NOT NULL,
            private_key TEXT
         );
         CREATE TABLE haex_space_members (
            id TEXT PRIMARY KEY NOT NULL,
            space_id TEXT NOT NULL,
            identity_id TEXT NOT NULL
         );",
    )
    .expect("create schema");

    let key = random_key();
    conn.execute(
        "INSERT INTO haex_identities (id, did, private_key) VALUES (?1, ?2, ?3)",
        ["id-own-a", "did:key:zOwnA", &pkcs8_b64(&key)],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO haex_space_members (id, space_id, identity_id) VALUES (?1, ?2, ?3)",
        ["mem-a", "space_A", "id-own-a"],
    )
    .unwrap();

    (DbConnection(Arc::new(Mutex::new(Some(conn)))), key)
}

#[test]
fn populate_column_sig_key_cache_warms_cache_from_mounted_db() {
    let (db, key) = open_db_with_one_owned_space();
    let cache = SpaceKeyCache::new();

    let n = populate_column_sig_key_cache(&cache, &db).expect("warm-up");
    assert_eq!(n, 1);
    assert!(cache.contains("space_A"));
    assert_eq!(
        cache.get("space_A").expect("space_A cached").to_bytes(),
        key.to_bytes()
    );
}

#[test]
fn populate_column_sig_key_cache_is_zero_on_empty_vault() {
    // Fresh vault: schema present, but no owned space memberships yet. The
    // create/open callsites treat this as the normal case and must not fail.
    let conn = Connection::open_in_memory().expect("open in-memory");
    conn.execute_batch(
        "CREATE TABLE haex_identities (
            id TEXT PRIMARY KEY NOT NULL,
            did TEXT NOT NULL,
            private_key TEXT
         );
         CREATE TABLE haex_space_members (
            id TEXT PRIMARY KEY NOT NULL,
            space_id TEXT NOT NULL,
            identity_id TEXT NOT NULL
         );",
    )
    .expect("create schema");
    let db = DbConnection(Arc::new(Mutex::new(Some(conn))));
    let cache = SpaceKeyCache::new();

    let n = populate_column_sig_key_cache(&cache, &db).expect("empty warm-up");
    assert_eq!(n, 0);
}
