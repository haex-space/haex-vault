use super::key_cache::SpaceKeyCache;
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use ed25519_dalek::SigningKey;
use rusqlite::Connection;

fn random_key() -> SigningKey {
    let seed: [u8; 32] = rand::random();
    SigningKey::from_bytes(&seed)
}

/// Encode a `SigningKey` as the Base64-PKCS8 blob that `haex_identities.private_key`
/// stores in production. Mirrors `ucan::create::tests::test_pkcs8_key`.
fn pkcs8_b64(key: &SigningKey) -> String {
    // Fixed 16-byte PKCS8-Ed25519 DER prefix; then 32 seed bytes.
    let pkcs8_prefix: [u8; 16] = [
        0x30, 0x2e, 0x02, 0x01, 0x00, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x04, 0x22, 0x04,
        0x20,
    ];
    let mut der = Vec::with_capacity(48);
    der.extend_from_slice(&pkcs8_prefix);
    der.extend_from_slice(&key.to_bytes());
    BASE64.encode(&der)
}

/// Minimal in-memory DB with two owned spaces (A + B). Only the columns
/// `SQL_SELECT_ALL_OWN_SPACE_KEYS` / `SQL_SELECT_OWN_SPACE_KEY` read are
/// modeled — the production schema has more, but this test cares only about
/// the join.
///
/// Returns the two seeded `SigningKey`s so tests can compare bytes.
fn seed_two_owned_spaces() -> (Connection, SigningKey, SigningKey) {
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

    let key_a = random_key();
    let key_b = random_key();

    // A contact identity (no private_key) that shares space_A. It must NOT
    // be returned by either query — the JOIN filters on i.private_key IS NOT NULL.
    conn.execute(
        "INSERT INTO haex_identities (id, did, private_key) VALUES (?1, ?2, NULL)",
        ["id-contact", "did:key:zContact"],
    )
    .unwrap();

    conn.execute(
        "INSERT INTO haex_identities (id, did, private_key) VALUES (?1, ?2, ?3)",
        ["id-own-a", "did:key:zOwnA", &pkcs8_b64(&key_a)],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO haex_identities (id, did, private_key) VALUES (?1, ?2, ?3)",
        ["id-own-b", "did:key:zOwnB", &pkcs8_b64(&key_b)],
    )
    .unwrap();

    conn.execute(
        "INSERT INTO haex_space_members (id, space_id, identity_id) VALUES (?1, ?2, ?3)",
        ["mem-a", "space_A", "id-own-a"],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO haex_space_members (id, space_id, identity_id) VALUES (?1, ?2, ?3)",
        ["mem-b", "space_B", "id-own-b"],
    )
    .unwrap();
    // Contact is in space_A but must not surface as own-key.
    conn.execute(
        "INSERT INTO haex_space_members (id, space_id, identity_id) VALUES (?1, ?2, ?3)",
        ["mem-a-contact", "space_A", "id-contact"],
    )
    .unwrap();

    (conn, key_a, key_b)
}

#[test]
fn empty_cache_returns_none() {
    let cache = SpaceKeyCache::new();
    assert!(cache.get("space_1").is_none());
}

#[test]
fn insert_and_get_roundtrip() {
    let cache = SpaceKeyCache::new();
    let key = random_key();
    cache.insert("space_1", key.clone());
    let out = cache.get("space_1").expect("present");
    assert_eq!(out.to_bytes(), key.to_bytes());
}

#[test]
fn remove_deletes_entry() {
    let cache = SpaceKeyCache::new();
    let key = random_key();
    cache.insert("space_1", key);
    cache.remove("space_1");
    assert!(cache.get("space_1").is_none());
}

#[test]
fn cache_is_send_sync_clone_via_arc() {
    let cache = SpaceKeyCache::new();
    let cache2 = cache.clone();
    let key = random_key();
    cache2.insert("space_1", key.clone());
    // Same inner Arc<RwLock<..>> so the clone sees the write.
    assert!(cache.get("space_1").is_some());
}

#[test]
fn populate_all_loads_all_own_space_identities_from_db() {
    let (conn, key_a, key_b) = seed_two_owned_spaces();
    let cache = SpaceKeyCache::new();
    let n = cache.populate_all(&conn).expect("populate");
    assert_eq!(n, 2);
    assert!(cache.contains("space_A"));
    assert!(cache.contains("space_B"));
    assert_eq!(
        cache.get("space_A").expect("A").to_bytes(),
        key_a.to_bytes()
    );
    assert_eq!(
        cache.get("space_B").expect("B").to_bytes(),
        key_b.to_bytes()
    );
}

#[test]
fn populate_all_replaces_existing_cache() {
    let (conn, _key_a, _key_b) = seed_two_owned_spaces();
    let cache = SpaceKeyCache::new();
    // Pre-seed with a stale entry that isn't in the DB — populate_all must clear it.
    cache.insert("space_stale", random_key());
    cache.populate_all(&conn).expect("populate");
    assert!(!cache.contains("space_stale"));
    assert!(cache.contains("space_A"));
    assert!(cache.contains("space_B"));
}

#[test]
fn get_or_reload_hits_db_on_miss() {
    let (conn, key_a, _key_b) = seed_two_owned_spaces();
    let cache = SpaceKeyCache::new();
    // No populate — cache is empty.
    let key = cache
        .get_or_reload(&conn, "space_A")
        .expect("reload")
        .expect("present");
    assert_eq!(key.to_bytes(), key_a.to_bytes());
    // Now cached.
    assert!(cache.contains("space_A"));
}

#[test]
fn get_or_reload_returns_none_for_unknown_space() {
    let (conn, _, _) = seed_two_owned_spaces();
    let cache = SpaceKeyCache::new();
    let out = cache
        .get_or_reload(&conn, "space_does_not_exist")
        .expect("query");
    assert!(out.is_none());
    // Negative results are not cached.
    assert!(!cache.contains("space_does_not_exist"));
}
