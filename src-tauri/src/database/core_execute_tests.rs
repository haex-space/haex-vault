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
use crate::crdt::trigger::{ensure_crdt_columns, setup_triggers_for_table};
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
         CREATE TABLE {} (table_name TEXT PRIMARY KEY, last_modified TEXT);
         INSERT INTO {} (key, type, value) VALUES ('triggers_enabled', 'system', '1');",
        TABLE_CRDT_CONFIGS, TABLE_CRDT_DIRTY_TABLES, TABLE_CRDT_CONFIGS
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
        // The register itself carries CRDT meta so `execute_with_crdt` can
        // insert into it (F2 target).
        ensure_crdt_columns(&tx, "haex_shared_space_sync").unwrap();
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

// ---------------------------------------------------------------------------
// F2 — share-insert cross-table signing
// ---------------------------------------------------------------------------

/// F2 fixtures need a THIRD owned identity/space so a share INSERT can declare
/// a brand-new (previously-unsigned) space membership. Extends the F1 fixture
/// in place; the `did_c` return threads the new identity through to callers.
struct FixtureF2 {
    db: DbConnection,
    hlc: HlcService,
    cache: SpaceKeyCache,
    did_a: String,
    did_c: String,
}

fn setup_fixture_f2() -> FixtureF2 {
    let f = setup_fixture();
    let key_c = random_key();
    let did_c = did_key_from_public_key(&key_c.verifying_key());
    {
        let guard = f.db.0.lock().unwrap();
        let conn = guard.as_ref().unwrap();
        conn.execute(
            "INSERT INTO haex_identities (id, did, private_key) VALUES (?1, ?2, ?3)",
            rusqlite::params!["id-c", &did_c, pkcs8_b64(&key_c)],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO haex_space_members (id, space_id, identity_id) VALUES (?1, ?2, ?3)",
            ["mem-c", "space_C", "id-c"],
        )
        .unwrap();
        // Install UPDATE triggers on ext_calendar so that seeding writes
        // populate `haex_column_hlcs` per-column HLCs — F2 must be able to
        // read the ORIGINAL column HLC when it retro-signs.
        let tx = conn.unchecked_transaction().unwrap();
        setup_triggers_for_table(&tx, "ext_calendar", true).unwrap();
        tx.commit().unwrap();
        // The pre-seeded ext_calendar rows came in via raw SQL before the
        // CRDT columns existed, so their `haex_column_hlcs` blob is NULL.
        // `json_set(NULL, …)` yields NULL, which would defeat the seed UPDATE
        // that populates per-column HLCs. Initialise both meta columns to
        // empty JSON objects so triggers can extend them.
        conn.execute(
            "UPDATE ext_calendar SET haex_column_hlcs = '{}', haex_column_sigs = '{}'",
            [],
        )
        .unwrap();
    }
    // Refresh the cache to pick up the new identity/space.
    {
        let guard = f.db.0.lock().unwrap();
        let conn = guard.as_ref().unwrap();
        f.cache.populate_all(conn).expect("repopulate cache");
    }
    FixtureF2 {
        db: f.db,
        hlc: f.hlc,
        cache: f.cache,
        did_a: f.did_a,
        did_c,
    }
}

/// Force per-column HLCs onto `ext_calendar` row `SOLO` via a real
/// `execute_with_crdt` UPDATE — that path populates `haex_hlc` and
/// `haex_column_hlcs` the same way as a live vault would. F2 then reads
/// those HLCs when it retro-signs the row for the new space.
fn seed_solo_row_hlcs(f: &FixtureF2) {
    let hlc_mutex = Mutex::new(f.hlc.clone());
    let hlc_guard = hlc_mutex.lock().unwrap();
    core::execute_with_crdt(
        "UPDATE ext_calendar SET title = ?1 WHERE id = ?2".to_string(),
        vec![
            JsonValue::String("solo-warm".to_string()),
            JsonValue::String("SOLO".to_string()),
        ],
        &f.db,
        &hlc_guard,
        &f.cache,
    )
    .expect("warm-up UPDATE succeeds");
}

fn count_rows(db: &DbConnection, sql: &str) -> i64 {
    let guard = db.0.lock().unwrap();
    let conn = guard.as_ref().unwrap();
    conn.query_row(sql, [], |r| r.get::<_, i64>(0)).unwrap()
}

#[test]
fn insert_into_share_register_signs_all_columns_of_referenced_row() {
    let f = setup_fixture_f2();
    seed_solo_row_hlcs(&f);

    // Grab the row's per-column HLC for `title` — F2 must reuse it as the
    // preimage HLC, not the current tx HLC of the share INSERT.
    let title_hlc: String = {
        let guard = f.db.0.lock().unwrap();
        let conn = guard.as_ref().unwrap();
        conn.query_row(
            "SELECT json_extract(haex_column_hlcs, '$.title') \
             FROM ext_calendar WHERE id = 'SOLO'",
            [],
            |r| r.get(0),
        )
        .unwrap()
    };
    assert!(
        !title_hlc.is_empty(),
        "seed step must populate haex_column_hlcs.title"
    );

    let hlc_mutex = Mutex::new(f.hlc);
    let hlc_guard = hlc_mutex.lock().unwrap();

    core::execute_with_crdt(
        "INSERT INTO haex_shared_space_sync \
            (id, table_name, row_pks, space_id, authored_by_did) \
         VALUES (?1, ?2, ?3, ?4, ?5)"
            .to_string(),
        vec![
            JsonValue::String("share-C".to_string()),
            JsonValue::String("ext_calendar".to_string()),
            JsonValue::String(r#"{"id":"SOLO"}"#.to_string()),
            JsonValue::String("space_C".to_string()),
            JsonValue::String(f.did_c.clone()),
        ],
        &f.db,
        &hlc_guard,
        &f.cache,
    )
    .expect("share INSERT succeeds");

    let sigs = read_column_sigs_json(&f.db, "ext_calendar", "SOLO");
    let title = sigs
        .get("title")
        .and_then(|v| v.as_object())
        .expect("sigs must have a 'title' entry after share-register INSERT");
    assert!(
        title.contains_key("space_C"),
        "expected space_C sig on title, got: {:?}",
        title.keys().collect::<Vec<_>>()
    );
    let space_c = title.get("space_C").and_then(|v| v.as_object()).unwrap();
    assert_eq!(
        space_c.get("author_did").and_then(|v| v.as_str()),
        Some(f.did_c.as_str())
    );
    assert!(space_c.get("sig").and_then(|v| v.as_str()).is_some());
}

#[test]
fn share_insert_rejects_when_authored_by_did_is_foreign_i2_violation() {
    let f = setup_fixture_f2();
    seed_solo_row_hlcs(&f);

    let sigs_before_count = count_rows(
        &f.db,
        "SELECT COUNT(*) FROM haex_shared_space_sync WHERE id = 'share-EVIL'",
    );
    assert_eq!(sigs_before_count, 0);

    let hlc_mutex = Mutex::new(f.hlc);
    let hlc_guard = hlc_mutex.lock().unwrap();

    let foreign_did = "did:key:z6MkFakeForeignAuthorForI2Test".to_string();
    let result = core::execute_with_crdt(
        "INSERT INTO haex_shared_space_sync \
            (id, table_name, row_pks, space_id, authored_by_did) \
         VALUES (?1, ?2, ?3, ?4, ?5)"
            .to_string(),
        vec![
            JsonValue::String("share-EVIL".to_string()),
            JsonValue::String("ext_calendar".to_string()),
            JsonValue::String(r#"{"id":"SOLO"}"#.to_string()),
            JsonValue::String("space_C".to_string()),
            JsonValue::String(foreign_did),
        ],
        &f.db,
        &hlc_guard,
        &f.cache,
    );

    assert!(
        matches!(
            result,
            Err(crate::database::error::DatabaseError::I2ForeignShareInsert { .. })
        ),
        "expected I2ForeignShareInsert, got: {:?}",
        result
    );

    // Rollback: no register row must have persisted.
    let after = count_rows(
        &f.db,
        "SELECT COUNT(*) FROM haex_shared_space_sync WHERE id = 'share-EVIL'",
    );
    assert_eq!(after, 0, "foreign-share INSERT must roll back");

    // No sigs must exist for SOLO under space_C either.
    let sigs = read_column_sigs_json(&f.db, "ext_calendar", "SOLO");
    if let Some(t) = sigs.get("title").and_then(|v| v.as_object()) {
        assert!(
            !t.contains_key("space_C"),
            "foreign share must not produce a space_C sig, got: {:?}",
            t.keys().collect::<Vec<_>>()
        );
    }
}

#[test]
fn share_insert_rejects_when_target_table_is_haex_system_i1_violation() {
    let f = setup_fixture_f2();

    let hlc_mutex = Mutex::new(f.hlc);
    let hlc_guard = hlc_mutex.lock().unwrap();

    let result = core::execute_with_crdt(
        "INSERT INTO haex_shared_space_sync \
            (id, table_name, row_pks, space_id, authored_by_did) \
         VALUES (?1, ?2, ?3, ?4, ?5)"
            .to_string(),
        vec![
            JsonValue::String("share-SYS".to_string()),
            JsonValue::String("haex_identities".to_string()),
            JsonValue::String(r#"{"id":"id-a"}"#.to_string()),
            JsonValue::String("space_C".to_string()),
            JsonValue::String(f.did_a.clone()),
        ],
        &f.db,
        &hlc_guard,
        &f.cache,
    );

    assert!(
        matches!(
            result,
            Err(crate::database::error::DatabaseError::I1RegisterTargetsSystemTable { .. })
        ),
        "expected I1RegisterTargetsSystemTable, got: {:?}",
        result
    );

    let after = count_rows(
        &f.db,
        "SELECT COUNT(*) FROM haex_shared_space_sync WHERE id = 'share-SYS'",
    );
    assert_eq!(after, 0, "I1-violating INSERT must roll back");
}

#[test]
fn share_insert_reuses_historical_column_hlc_not_tx_hlc() {
    // The sig preimage binds `hlc = <col>__crdt_ts` — the row's historical
    // per-column HLC captured when the value was written, not the tx HLC of
    // the register INSERT. Verifying by construction: build the same preimage
    // manually with the seeded HLC and check that the persisted sig verifies
    // against it — a sig built from the wrong HLC would fail Ed25519 verify.
    use crate::crdt::column_sig::preimage::build_preimage;
    use ed25519_dalek::{Signature, Verifier};

    let f = setup_fixture_f2();
    seed_solo_row_hlcs(&f);

    let title_hlc: String = {
        let guard = f.db.0.lock().unwrap();
        let conn = guard.as_ref().unwrap();
        conn.query_row(
            "SELECT json_extract(haex_column_hlcs, '$.title') \
             FROM ext_calendar WHERE id = 'SOLO'",
            [],
            |r| r.get(0),
        )
        .unwrap()
    };

    let hlc_mutex = Mutex::new(f.hlc);
    let hlc_guard = hlc_mutex.lock().unwrap();
    core::execute_with_crdt(
        "INSERT INTO haex_shared_space_sync \
            (id, table_name, row_pks, space_id, authored_by_did) \
         VALUES (?1, ?2, ?3, ?4, ?5)"
            .to_string(),
        vec![
            JsonValue::String("share-D".to_string()),
            JsonValue::String("ext_calendar".to_string()),
            JsonValue::String(r#"{"id":"SOLO"}"#.to_string()),
            JsonValue::String("space_C".to_string()),
            JsonValue::String(f.did_c.clone()),
        ],
        &f.db,
        &hlc_guard,
        &f.cache,
    )
    .expect("share INSERT succeeds");
    drop(hlc_guard);

    let sigs = read_column_sigs_json(&f.db, "ext_calendar", "SOLO");
    let title = sigs.get("title").and_then(|v| v.as_object()).unwrap();
    let space_c = title.get("space_C").and_then(|v| v.as_object()).unwrap();
    let sig_b64 = space_c.get("sig").and_then(|v| v.as_str()).unwrap();
    let sig_bytes = BASE64.decode(sig_b64).unwrap();
    let sig = Signature::from_slice(&sig_bytes).unwrap();

    // Reconstruct: value = "solo-warm" (from seed_solo_row_hlcs).
    use crate::crdt::column_sig::value_bytes;
    use rusqlite::types::Value as SqlValue;
    let value_bytes = value_bytes::to_canonical_bytes(&SqlValue::Text("solo-warm".to_string()));
    let preimage = build_preimage(
        b"space_C",
        b"ext_calendar",
        br#"{"id":"SOLO"}"#,
        b"title",
        title_hlc.as_bytes(),
        f.did_c.as_bytes(),
        &value_bytes,
    );

    // Grab space_C's public key and verify.
    let vk = f
        .cache
        .get("space_C")
        .expect("space_C key cached")
        .verifying_key();
    vk.verify(&preimage, &sig)
        .expect("sig must verify against the ORIGINAL per-column HLC preimage");
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
