// src-tauri/src/database/core_execute_tests.rs
//
// Integration tests for the column-signing side of `execute_with_crdt`
// (Task F1 in the shared-space Phase 1 plan). Verifies that every INSERT/
// UPDATE through the CRDT execute path lands with a fresh Ed25519 signature
// under `haex_column_sigs` for every space the row is shared into.

use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use ed25519_dalek::SigningKey;
use rusqlite::Connection;
use serde_json::Value as JsonValue;
use std::sync::{Arc, Mutex};

use crate::crdt::column_sig::key_cache::SpaceKeyCache;
use crate::crdt::hlc::HlcService;
use crate::crdt::trigger::ensure_crdt_columns;
use crate::database::connection_context::ConnectionContext;
use crate::database::core::{self, install_tx_hlc_hooks, register_current_hlc_udf};
use crate::database::DbConnection;
use crate::table_names::{TABLE_CRDT_CONFIGS, TABLE_CRDT_DIRTY_TABLES};
use crate::ucan::verify::did_key_from_public_key;

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
    db: DbConnection,
    hlc: HlcService,
    cache: SpaceKeyCache,
    did_a: String,
    did_b: String,
}

/// Builds an in-memory DB that mirrors just enough of the production schema
/// for the sign path to run end-to-end:
///   - CRDT config + dirty-tables + UDF wiring so `execute_with_crdt` works.
///   - Two owned identities → two owned spaces.
///   - `haex_shared_space_sync` register with one shared extension-row.
///   - `haex_space_devices` seeded with an infra row for space_A.
///   - Extension table `ext_calendar` with CRDT columns.
fn setup_fixture() -> Fixture {
    let conn = Connection::open_in_memory().expect("in-memory DB");

    let hlc = HlcService::new_for_testing("test-device-f1");
    let ctx = ConnectionContext::new();
    register_current_hlc_udf(&conn, hlc.clone(), ctx.clone()).unwrap();
    install_tx_hlc_hooks(&conn, ctx).unwrap();

    conn.execute_batch(&format!(
        "CREATE TABLE {} (key TEXT PRIMARY KEY, type TEXT NOT NULL, value TEXT NOT NULL);
         CREATE TABLE {} (table_name TEXT PRIMARY KEY, last_modified TEXT);",
        TABLE_CRDT_CONFIGS, TABLE_CRDT_DIRTY_TABLES
    ))
    .unwrap();

    // Identity + membership schema (I2 filter reads these).
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
    .unwrap();

    let key_a = random_key();
    let key_b = random_key();
    let did_a = did_key_from_public_key(&key_a.verifying_key());
    let did_b = did_key_from_public_key(&key_b.verifying_key());

    conn.execute(
        "INSERT INTO haex_identities (id, did, private_key) VALUES (?1, ?2, ?3)",
        rusqlite::params!["id-a", &did_a, pkcs8_b64(&key_a)],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO haex_identities (id, did, private_key) VALUES (?1, ?2, ?3)",
        rusqlite::params!["id-b", &did_b, pkcs8_b64(&key_b)],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO haex_space_members (id, space_id, identity_id) VALUES (?1, ?2, ?3)",
        ["mem-a", "space_A", "id-a"],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO haex_space_members (id, space_id, identity_id) VALUES (?1, ?2, ?3)",
        ["mem-b", "space_B", "id-b"],
    )
    .unwrap();

    // Extension table with CRDT columns. Row `R` is shared into BOTH owned
    // spaces; row `SOLO` is not shared into any space.
    conn.execute_batch(
        "CREATE TABLE ext_calendar (
            id TEXT PRIMARY KEY NOT NULL,
            title TEXT
         );
         INSERT INTO ext_calendar (id, title) VALUES ('R', 'initial');
         INSERT INTO ext_calendar (id, title) VALUES ('SOLO', 'unshared');",
    )
    .unwrap();
    conn.execute(
        "INSERT INTO haex_shared_space_sync
            (id, table_name, row_pks, space_id, authored_by_did)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        rusqlite::params![
            "share-A",
            "ext_calendar",
            r#"{"id":"R"}"#,
            "space_A",
            &did_a
        ],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO haex_shared_space_sync
            (id, table_name, row_pks, space_id, authored_by_did)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        rusqlite::params![
            "share-B",
            "ext_calendar",
            r#"{"id":"R"}"#,
            "space_B",
            &did_b
        ],
    )
    .unwrap();

    // Infra table haex_space_devices (space-scoped): row for space_A.
    conn.execute_batch(
        "CREATE TABLE haex_space_devices (
            id TEXT PRIMARY KEY NOT NULL,
            space_id TEXT NOT NULL,
            endpoint_id TEXT,
            avatar TEXT
         );
         INSERT INTO haex_space_devices (id, space_id, endpoint_id, avatar)
         VALUES ('dev-A1', 'space_A', 'ep-1', 'old-avatar');",
    )
    .unwrap();

    {
        let tx = conn.unchecked_transaction().unwrap();
        ensure_crdt_columns(&tx, "ext_calendar").unwrap();
        ensure_crdt_columns(&tx, "haex_space_devices").unwrap();
        tx.commit().unwrap();
    }

    let cache = SpaceKeyCache::new();
    cache.populate_all(&conn).expect("populate cache");

    let db = DbConnection(Arc::new(Mutex::new(Some(conn))));
    Fixture {
        db,
        hlc,
        cache,
        did_a,
        did_b,
    }
}

fn read_column_sigs_json(
    db: &DbConnection,
    table: &str,
    id: &str,
) -> serde_json::Map<String, JsonValue> {
    let guard = db.0.lock().unwrap();
    let conn = guard.as_ref().unwrap();
    let raw: String = conn
        .query_row(
            &format!("SELECT haex_column_sigs FROM \"{table}\" WHERE id = ?1"),
            [id],
            |r| r.get(0),
        )
        .unwrap();
    let v: JsonValue = serde_json::from_str(&raw).unwrap();
    match v {
        JsonValue::Object(m) => m,
        other => panic!("haex_column_sigs is not a JSON object: {other:?}"),
    }
}

#[test]
fn execute_with_crdt_signs_all_changed_columns_for_all_spaces() {
    let f = setup_fixture();
    let hlc_mutex = Mutex::new(f.hlc);
    let hlc_guard = hlc_mutex.lock().unwrap();

    core::execute_with_crdt(
        "UPDATE ext_calendar SET title = ?1 WHERE id = ?2".to_string(),
        vec![
            JsonValue::String("Hello".to_string()),
            JsonValue::String("R".to_string()),
        ],
        &f.db,
        &hlc_guard,
        &f.cache,
    )
    .expect("update succeeds");

    let sigs = read_column_sigs_json(&f.db, "ext_calendar", "R");
    let title = sigs
        .get("title")
        .and_then(|v| v.as_object())
        .expect("sigs must have a 'title' entry");
    assert!(
        title.contains_key("space_A"),
        "expected space_A sig, got: {:?}",
        title.keys().collect::<Vec<_>>()
    );
    assert!(
        title.contains_key("space_B"),
        "expected space_B sig, got: {:?}",
        title.keys().collect::<Vec<_>>()
    );

    let space_a = title.get("space_A").and_then(|v| v.as_object()).unwrap();
    assert_eq!(
        space_a.get("author_did").and_then(|v| v.as_str()),
        Some(f.did_a.as_str())
    );
    assert!(space_a.get("sig").and_then(|v| v.as_str()).is_some());

    let space_b = title.get("space_B").and_then(|v| v.as_object()).unwrap();
    assert_eq!(
        space_b.get("author_did").and_then(|v| v.as_str()),
        Some(f.did_b.as_str())
    );
}

#[test]
fn execute_with_crdt_skips_signing_when_row_has_no_spaces() {
    let f = setup_fixture();
    let hlc_mutex = Mutex::new(f.hlc);
    let hlc_guard = hlc_mutex.lock().unwrap();

    core::execute_with_crdt(
        "UPDATE ext_calendar SET title = ?1 WHERE id = ?2".to_string(),
        vec![
            JsonValue::String("still-solo".to_string()),
            JsonValue::String("SOLO".to_string()),
        ],
        &f.db,
        &hlc_guard,
        &f.cache,
    )
    .expect("update succeeds");

    let sigs = read_column_sigs_json(&f.db, "ext_calendar", "SOLO");
    assert!(
        sigs.is_empty(),
        "unshared row must not accumulate sigs, got: {:?}",
        sigs
    );
}

#[test]
fn execute_with_crdt_signs_infra_table_columns_for_row_space() {
    let f = setup_fixture();
    let hlc_mutex = Mutex::new(f.hlc);
    let hlc_guard = hlc_mutex.lock().unwrap();

    core::execute_with_crdt(
        "UPDATE haex_space_devices SET avatar = ?1 WHERE id = ?2".to_string(),
        vec![
            JsonValue::String("new-avatar".to_string()),
            JsonValue::String("dev-A1".to_string()),
        ],
        &f.db,
        &hlc_guard,
        &f.cache,
    )
    .expect("update succeeds");

    let sigs = read_column_sigs_json(&f.db, "haex_space_devices", "dev-A1");
    let avatar = sigs
        .get("avatar")
        .and_then(|v| v.as_object())
        .expect("sigs must have an 'avatar' entry");
    assert!(avatar.contains_key("space_A"));
    assert!(
        !avatar.contains_key("space_B"),
        "infra row lives in space_A only, must not sign for space_B"
    );

    let space_a = avatar.get("space_A").and_then(|v| v.as_object()).unwrap();
    assert_eq!(
        space_a.get("author_did").and_then(|v| v.as_str()),
        Some(f.did_a.as_str())
    );
}
