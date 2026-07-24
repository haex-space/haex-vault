//! Unit tests for the Task-C4 SpaceKeyCache lifecycle hook.
//!
//! Covers the two pure, AppHandle-free helpers used by all four
//! space-mutation sites. Full integration with `handle_claim_invite` /
//! `local_delivery_start` etc. would need a live app harness — that path is
//! implicitly covered by the DB-round-trip in `warm_from_db_populates_cache`
//! here plus the existing `key_cache_tests` contract tests.

use super::{drop_column_sig_cache, warm_column_sig_cache};
use crate::crdt::column_sig::key_cache::SpaceKeyCache;
use crate::database::DbConnection;
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use ed25519_dalek::SigningKey;
use rusqlite::Connection;
use std::sync::{Arc, Mutex};

fn random_key() -> SigningKey {
    let seed: [u8; 32] = rand::random();
    SigningKey::from_bytes(&seed)
}

/// Mirror of `crdt::column_sig::key_cache_tests::pkcs8_b64` so the reload
/// path reads a valid Base64-PKCS8-Ed25519 blob out of the seeded DB.
fn pkcs8_b64(key: &SigningKey) -> String {
    let pkcs8_prefix: [u8; 16] = [
        0x30, 0x2e, 0x02, 0x01, 0x00, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x04, 0x22, 0x04,
        0x20,
    ];
    let mut der = Vec::with_capacity(48);
    der.extend_from_slice(&pkcs8_prefix);
    der.extend_from_slice(&key.to_bytes());
    BASE64.encode(&der)
}

/// Wrap an in-memory `Connection` in a `DbConnection` so the helper can be
/// exercised through the production `with_connection` code path.
fn db_with_owned_space(space_id: &str, key: &SigningKey) -> DbConnection {
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
    .unwrap();
    conn.execute(
        "INSERT INTO haex_identities (id, did, private_key) VALUES (?1, ?2, ?3)",
        ["id-own", "did:key:zOwn", &pkcs8_b64(key)],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO haex_space_members (id, space_id, identity_id) VALUES (?1, ?2, ?3)",
        ["mem-own", space_id, "id-own"],
    )
    .unwrap();
    DbConnection(Arc::new(Mutex::new(Some(conn))))
}

/// An empty in-memory DB shaped like production: no rows for `space_id`.
fn db_with_no_owned_space() -> DbConnection {
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
    .unwrap();
    DbConnection(Arc::new(Mutex::new(Some(conn))))
}

#[test]
fn warm_from_db_populates_cache() {
    let cache = SpaceKeyCache::new();
    let key = random_key();
    let db = db_with_owned_space("space_A", &key);

    assert!(!cache.contains("space_A"));
    warm_column_sig_cache(&cache, &db, "space_A");
    assert!(cache.contains("space_A"));
    assert_eq!(
        cache.get("space_A").expect("present").to_bytes(),
        key.to_bytes()
    );
}

#[test]
fn warm_is_a_noop_when_row_missing() {
    let cache = SpaceKeyCache::new();
    let db = db_with_no_owned_space();

    // No row for the space, no panic, no cache pollution.
    warm_column_sig_cache(&cache, &db, "space_not_present");
    assert!(!cache.contains("space_not_present"));
}

#[test]
fn drop_removes_only_the_target_space() {
    let cache = SpaceKeyCache::new();
    let key_a = random_key();
    let key_b = random_key();
    cache.insert("space_A", key_a);
    cache.insert("space_B", key_b);

    drop_column_sig_cache(&cache, "space_A");

    assert!(!cache.contains("space_A"));
    assert!(cache.contains("space_B"));
}

#[test]
fn drop_is_a_noop_when_entry_missing() {
    let cache = SpaceKeyCache::new();
    // Must not panic — leader_delivery_stop may run against a cache that
    // never held the space (e.g. cache was never warmed because the
    // owner's `haex_space_members` row hadn't synced yet).
    drop_column_sig_cache(&cache, "space_never_seen");
    assert!(!cache.contains("space_never_seen"));
}
