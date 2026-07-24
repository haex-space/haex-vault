use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use ed25519_dalek::SigningKey;
use rusqlite::{types::Value, Connection};

use super::key_cache::SpaceKeyCache;
use super::limits::MAX_VALUE_BYTES_LEN;
use super::register_lookup::RegisterLookup;
use super::verify::verify_column_sig;
use super::write::{sign_column_for_spaces, SignForSpacesError};
use crate::ucan::verify::did_key_from_public_key;

// ---------------------------------------------------------------------------
// Test fixture — extends key_cache_tests::seed_two_owned_spaces with the
// share register (`haex_shared_space_sync`) so `RegisterLookup::resolve`
// can join.
//
// Semantics assumed for the register (matches design-doc §E1 + plan E1):
//   * infra tables → space_id from the row itself (not exercised here — E2
//     runs on extension tables in every test that would touch the register)
//   * extension tables → SELECT space_id FROM haex_shared_space_sync
//     WHERE table_name = ? AND row_pks = ? AND authored_by_did IN (my dids)
// ---------------------------------------------------------------------------

fn random_key() -> SigningKey {
    let seed: [u8; 32] = rand::random();
    SigningKey::from_bytes(&seed)
}

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

struct Fixture {
    conn: Connection,
    key_a: SigningKey,
    key_b: SigningKey,
    did_a: String,
    did_b: String,
}

/// In-memory DB with:
///   * two owned identities → two owned spaces (space_A, space_B)
///   * one shared row R in the extension table `ext_calendar` that is
///     shared into BOTH spaces (register entry per space, each authored
///     by that space's own DID)
///
/// The DB models only what the queries under test read; the production
/// schema has more columns.
fn seed_shared_to_two_spaces() -> Fixture {
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
         );
         CREATE TABLE haex_shared_space_sync (
            id TEXT PRIMARY KEY NOT NULL,
            table_name TEXT NOT NULL,
            row_pks TEXT NOT NULL,
            space_id TEXT NOT NULL,
            authored_by_did TEXT
         );",
    )
    .expect("create schema");

    let key_a = random_key();
    let key_b = random_key();
    let did_a = did_key_from_public_key(&key_a.verifying_key());
    let did_b = did_key_from_public_key(&key_b.verifying_key());

    conn.execute(
        "INSERT INTO haex_identities (id, did, private_key) VALUES (?1, ?2, ?3)",
        ["id-own-a", &did_a, &pkcs8_b64(&key_a)],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO haex_identities (id, did, private_key) VALUES (?1, ?2, ?3)",
        ["id-own-b", &did_b, &pkcs8_b64(&key_b)],
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

    // Row R in ext_calendar is shared into both spaces (I authored the share
    // for each space under my own identity there).
    conn.execute(
        "INSERT INTO haex_shared_space_sync
            (id, table_name, row_pks, space_id, authored_by_did)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        [
            "share-A",
            "ext_calendar",
            r#"{"id":"R"}"#,
            "space_A",
            &did_a,
        ],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO haex_shared_space_sync
            (id, table_name, row_pks, space_id, authored_by_did)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        [
            "share-B",
            "ext_calendar",
            r#"{"id":"R"}"#,
            "space_B",
            &did_b,
        ],
    )
    .unwrap();

    Fixture {
        conn,
        key_a,
        key_b,
        did_a,
        did_b,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[test]
fn sign_for_two_spaces_produces_two_sigs() {
    let f = seed_shared_to_two_spaces();
    let cache = SpaceKeyCache::new();
    cache.populate_all(&f.conn).expect("populate");
    let register = RegisterLookup::new();

    let out = sign_column_for_spaces(
        &f.conn,
        &cache,
        &register,
        "ext_calendar",
        r#"{"id":"R"}"#,
        "title",
        "hlc-1",
        &Value::Text("Hi".into()),
    )
    .expect("sign");

    assert_eq!(out.len(), 2);
    assert!(out.contains_key("space_A"));
    assert!(out.contains_key("space_B"));

    // Author DID must match the space-owning key we signed with.
    assert_eq!(
        out.get("space_A").expect("A").author_did,
        did_key_from_public_key(&f.key_a.verifying_key())
    );
    assert_eq!(
        out.get("space_B").expect("B").author_did,
        did_key_from_public_key(&f.key_b.verifying_key())
    );
}

#[test]
fn signed_bytes_verify_with_derived_did() {
    let f = seed_shared_to_two_spaces();
    let cache = SpaceKeyCache::new();
    cache.populate_all(&f.conn).expect("populate");
    let register = RegisterLookup::new();

    let out = sign_column_for_spaces(
        &f.conn,
        &cache,
        &register,
        "ext_calendar",
        r#"{"id":"R"}"#,
        "title",
        "hlc-1",
        &Value::Text("Hi".into()),
    )
    .expect("sign");

    let value_bytes = super::value_bytes::to_canonical_bytes(&Value::Text("Hi".into()));
    for (space_id, rec) in &out {
        verify_column_sig(
            space_id.as_bytes(),
            b"ext_calendar",
            r#"{"id":"R"}"#.as_bytes(),
            b"title",
            b"hlc-1",
            &rec.author_did,
            &value_bytes,
            &rec.sig,
        )
        .expect("verify");
    }

    // Sanity — the two DIDs match the two owned keys, exactly once each.
    let dids: std::collections::HashSet<_> = out.values().map(|r| r.author_did.clone()).collect();
    assert_eq!(dids.len(), 2);
    assert!(dids.contains(&f.did_a));
    assert!(dids.contains(&f.did_b));
}

#[test]
fn sign_returns_empty_for_row_shared_to_no_spaces() {
    // Same schema, but no register entries for this (table, row).
    let f = seed_shared_to_two_spaces();
    let cache = SpaceKeyCache::new();
    cache.populate_all(&f.conn).expect("populate");
    let register = RegisterLookup::new();

    let out = sign_column_for_spaces(
        &f.conn,
        &cache,
        &register,
        "ext_calendar",
        r#"{"id":"UNSHARED"}"#,
        "title",
        "hlc-1",
        &Value::Text("x".into()),
    )
    .expect("sign");

    assert!(out.is_empty());
}

#[test]
fn sign_rejects_oversized_value_bytes() {
    let f = seed_shared_to_two_spaces();
    let cache = SpaceKeyCache::new();
    cache.populate_all(&f.conn).expect("populate");
    let register = RegisterLookup::new();

    // TEXT storage-class → bytes are UTF-8 verbatim; one ASCII byte per char.
    let too_big = "a".repeat(MAX_VALUE_BYTES_LEN + 1);
    let err = sign_column_for_spaces(
        &f.conn,
        &cache,
        &register,
        "ext_calendar",
        r#"{"id":"R"}"#,
        "title",
        "hlc-1",
        &Value::Text(too_big),
    )
    .expect_err("must reject oversized value");

    assert!(matches!(err, SignForSpacesError::ValueBytesTooLarge(_)));
}

#[test]
fn sign_returns_no_key_error_when_cache_and_db_lack_space() {
    // A register entry pointing at space_C, but no matching
    // `haex_space_members`/`haex_identities` row → `SpaceKeyCache::get_or_reload`
    // returns `Ok(None)` and we surface `NoKeyForSpace`.
    let f = seed_shared_to_two_spaces();
    // authored_by_did = did_a so the RegisterLookup I2 filter surfaces this row,
    // but space_C has no owning identity in members → no key can be loaded.
    f.conn
        .execute(
            "INSERT INTO haex_shared_space_sync
                (id, table_name, row_pks, space_id, authored_by_did)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            ["share-C", "ext_ghost", r#"{"id":"G"}"#, "space_C", &f.did_a],
        )
        .unwrap();

    let cache = SpaceKeyCache::new();
    // Do NOT populate — even if we did, space_C isn't in members and wouldn't
    // land in the cache. get_or_reload will also miss on the JIT path.
    let register = RegisterLookup::new();

    let err = sign_column_for_spaces(
        &f.conn,
        &cache,
        &register,
        "ext_ghost",
        r#"{"id":"G"}"#,
        "title",
        "hlc-1",
        &Value::Text("x".into()),
    )
    .expect_err("must surface NoKeyForSpace");

    assert!(matches!(err, SignForSpacesError::NoKeyForSpace(ref s) if s == "space_C"));
}
