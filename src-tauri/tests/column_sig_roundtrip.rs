//! Rust-side roundtrip integration test for column signatures (Phase 1,
//! Task J1). Exercises the P2P-apply half of the shared-space column-sig
//! flow without a TS layer or Tauri IPC harness:
//!
//!   sender  execute_with_crdt(INSERT/UPDATE ...) → row lands with
//!           `haex_column_sigs` populated (F1);
//!   read back the row, build a `RemoteColumnChange` carrying the sig,
//!   feed it to
//!   receiver apply_remote_changes_to_db(...) on a FRESH in-memory DB;
//!   assert  the row applied, the sig verified (no rejection), and
//!           `ensure_identity_stub` produced a `haex_identities` row for
//!           the sender's DID (ADR 0002 §6 receiver stub creation).
//!
//! SCOPE: this test covers the Rust P2P path only (`apply_remote_changes_to_db`).
//! The TS server-sync path (push.ts → HTTP → pull → apply.ts →
//! `verify_column_sig_batch`) would need a Tauri test harness or an E2E
//! test in `haex-e2e-tests`, both outside J1's scope. The TS post-decrypt
//! verify is unit-tested elsewhere (`apply-column-sig-verify.test.ts`).

use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use ed25519_dalek::SigningKey;
use haex_vault_lib::crdt::column_sig::key_cache::SpaceKeyCache;
use haex_vault_lib::crdt::commands::apply::{
    apply_remote_changes_to_db_scoped, ColumnSig, RemoteColumnChange,
};
use haex_vault_lib::crdt::hlc::HlcService;
use haex_vault_lib::crdt::trigger::{ensure_crdt_columns, setup_triggers_for_table};
use haex_vault_lib::database::connection_context::ConnectionContext;
use haex_vault_lib::database::core::{
    execute_with_crdt, install_tx_hlc_hooks, register_current_hlc_udf,
};
use haex_vault_lib::database::DbConnection;
use haex_vault_lib::table_names::{TABLE_CRDT_CONFIGS, TABLE_CRDT_DIRTY_TABLES};
use haex_vault_lib::ucan::verify::did_key_from_public_key;
use rusqlite::Connection;
use serde_json::Value as JsonValue;
use std::sync::{Arc, Mutex};

// ---------------------------------------------------------------------------
// PKCS8 wrapping for `haex_identities.private_key`. Mirrors the helper in
// `core_execute_tests.rs::pkcs8_b64` verbatim so the sender path we drive
// here uses exactly the storage format `SpaceKeyCache::populate_all`
// expects to find.
// ---------------------------------------------------------------------------
fn pkcs8_b64(key: &SigningKey) -> String {
    const PKCS8_PREFIX: [u8; 16] = [
        0x30, 0x2e, 0x02, 0x01, 0x00, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x04, 0x22, 0x04,
        0x20,
    ];
    let mut der = Vec::with_capacity(48);
    der.extend_from_slice(&PKCS8_PREFIX);
    der.extend_from_slice(&key.to_bytes());
    BASE64.encode(&der)
}

fn random_key() -> SigningKey {
    let seed: [u8; 32] = rand::random();
    SigningKey::from_bytes(&seed)
}

// ---------------------------------------------------------------------------
// Sender + receiver harness. The sender DB carries the full set of tables
// F1 needs to sign a row (identities + membership + register + extension
// table + CRDT config/dirty-tables + HLC UDF). The receiver DB carries
// only what `apply_remote_changes_to_db` reads/writes.
// ---------------------------------------------------------------------------
struct Sender {
    db: DbConnection,
    hlc: HlcService,
    cache: SpaceKeyCache,
    space_id: String,
    signer_did: String,
}

fn setup_sender() -> Sender {
    let conn = Connection::open_in_memory().expect("in-memory sender DB");

    let hlc = HlcService::new_for_testing("test-device-sender");
    let ctx = ConnectionContext::new();
    register_current_hlc_udf(&conn, hlc.clone(), ctx.clone()).unwrap();
    install_tx_hlc_hooks(&conn, ctx).unwrap();

    conn.execute_batch(&format!(
        "CREATE TABLE {} (key TEXT PRIMARY KEY, type TEXT NOT NULL, value TEXT NOT NULL);
         CREATE TABLE {} (table_name TEXT PRIMARY KEY, last_modified TEXT);
         INSERT INTO {} (key, type, value) VALUES ('triggers_enabled', 'system', '1');
         -- Column set + nullability mirrors migration 0000 for the columns
         -- `ensure_identity_stub` has to satisfy. `name TEXT NOT NULL` with no
         -- default is load-bearing: a stub INSERT that omits it is silently
         -- dropped by `OR IGNORE`, which is exactly the regression this
         -- fixture must be able to catch.
         CREATE TABLE haex_identities (
             id TEXT PRIMARY KEY NOT NULL,
             did TEXT NOT NULL,
             name TEXT NOT NULL,
             source TEXT DEFAULT 'contact' NOT NULL,
             private_key TEXT
         );
         CREATE UNIQUE INDEX haex_identities_did_unique ON haex_identities (did);
         CREATE TABLE haex_space_members (
             id TEXT PRIMARY KEY NOT NULL,
             space_id TEXT NOT NULL,
             identity_id TEXT NOT NULL
         );
         CREATE TABLE haex_shared_space_sync (
             id TEXT PRIMARY KEY NOT NULL,
             table_name TEXT NOT NULL,
             row_pks TEXT NOT NULL,
             space_id TEXT NOT NULL
         );
         CREATE TABLE ext_calendar (
             id TEXT PRIMARY KEY NOT NULL,
             title TEXT
         );",
        TABLE_CRDT_CONFIGS, TABLE_CRDT_DIRTY_TABLES, TABLE_CRDT_CONFIGS
    ))
    .unwrap();

    // One owned identity → one owned space → one shared row.
    let sender_key = random_key();
    let sender_did = did_key_from_public_key(&sender_key.verifying_key());
    let space_id = "space_roundtrip".to_string();
    conn.execute(
        "INSERT INTO haex_identities (id, did, name, private_key) VALUES (?1, ?2, ?2, ?3)",
        rusqlite::params!["id-sender", &sender_did, pkcs8_b64(&sender_key)],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO haex_space_members (id, space_id, identity_id) VALUES (?1, ?2, ?3)",
        ["mem-sender", &space_id, "id-sender"],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO ext_calendar (id, title) VALUES ('R', 'initial')",
        [],
    )
    .unwrap();
    // Grow both tables into CRDT-aware ones. The register must be signed too:
    // I2 deliberately ignores a mapping that was not authored by this vault.
    {
        let tx = conn.unchecked_transaction().unwrap();
        ensure_crdt_columns(&tx, "ext_calendar").unwrap();
        setup_triggers_for_table(&tx, "ext_calendar", true).unwrap();
        ensure_crdt_columns(&tx, "haex_shared_space_sync").unwrap();
        setup_triggers_for_table(&tx, "haex_shared_space_sync", true).unwrap();
        tx.commit().unwrap();
    }
    // The pre-seeded 'R' row was inserted before CRDT columns existed, so
    // its `haex_column_hlcs` is NULL. `json_set(NULL, …)` yields NULL and
    // the AFTER-UPDATE trigger would silently fail to extend it — matching
    // the F2 fixture's initialisation dance in core_execute_tests.rs.
    conn.execute(
        "UPDATE ext_calendar SET haex_column_hlcs = '{}', haex_column_sigs = '{}'",
        [],
    )
    .unwrap();

    let cache = SpaceKeyCache::new();
    cache.populate_all(&conn).expect("populate sender cache");

    let db = DbConnection(Arc::new(Mutex::new(Some(conn))));
    let hlc_mutex = Mutex::new(hlc.clone());
    let hlc_guard = hlc_mutex.lock().unwrap();
    execute_with_crdt(
        "INSERT INTO haex_shared_space_sync (id, table_name, row_pks, space_id) \
         VALUES (?1, ?2, ?3, ?4)"
            .to_string(),
        vec![
            JsonValue::String("share-1".to_string()),
            JsonValue::String("ext_calendar".to_string()),
            JsonValue::String(r#"{"id":"R"}"#.to_string()),
            JsonValue::String(space_id.clone()),
        ],
        &db,
        &hlc_guard,
        &cache,
    )
    .expect("self-authored share mapping must succeed");
    drop(hlc_guard);

    Sender {
        db,
        hlc,
        cache,
        space_id,
        signer_did: sender_did,
    }
}

/// Fresh receiver DB — apply-side only. Deliberately EMPTY of identities and
/// register so the assertions can prove:
///   1. `apply_remote_changes_to_db` sig-verifies without needing the
///      register on the receiver side (space_id resolved from the arriving
///      row's `space_id` column change).
///   2. `ensure_identity_stub` inserts a NEW `haex_identities` row for the
///      sender's DID (no pre-seed to shadow it).
fn setup_receiver() -> DbConnection {
    let conn = Connection::open_in_memory().expect("in-memory receiver DB");
    conn.execute_batch(&format!(
        "CREATE TABLE {} (key TEXT PRIMARY KEY, type TEXT, value TEXT);
         -- Faithful to migration 0000 for the columns the stub INSERT touches:
         -- `name TEXT NOT NULL` without a default is what made the original
         -- `INSERT OR IGNORE (id, did)` a silent no-op in production.
         CREATE TABLE haex_identities (
             id TEXT PRIMARY KEY NOT NULL,
             did TEXT NOT NULL,
             name TEXT NOT NULL,
             source TEXT DEFAULT 'contact' NOT NULL
         );
         CREATE UNIQUE INDEX haex_identities_did_unique ON haex_identities (did);
         CREATE TABLE haex_deleted_rows (
             id TEXT PRIMARY KEY,
             table_name TEXT NOT NULL,
             row_pks TEXT NOT NULL,
             haex_hlc TEXT,
             haex_column_hlcs TEXT NOT NULL DEFAULT '{{}}'
         );
         -- Same shape as the sender's `ext_calendar`. Extension tables carry
         -- NO inline `space_id`: membership lives in the share register
         -- (`haex_shared_space_sync`). The earlier version of this fixture
         -- gave the receiver an extra `space_id NOT NULL` column that the
         -- sender did not have, which forced the test to ship a synthetic
         -- `space_id` column change — and that change was what the apply-side
         -- verifier then used as its signature anchor. Keeping the schemas
         -- symmetric means the anchor can only come from the pull scope.
         CREATE TABLE ext_calendar (
             id TEXT PRIMARY KEY NOT NULL,
             title TEXT,
             haex_hlc TEXT,
             haex_column_hlcs TEXT NOT NULL DEFAULT '{{}}',
             haex_column_sigs TEXT NOT NULL DEFAULT '{{}}'
         );",
        TABLE_CRDT_CONFIGS
    ))
    .unwrap();
    DbConnection(Arc::new(Mutex::new(Some(conn))))
}

/// Read the sender's `haex_column_sigs` JSON for a row and extract the sig
/// record for `(column, space_id)`. Panics with a helpful message on any
/// shape mismatch so a regression in the sig-storage layer surfaces here.
fn extract_sig(
    db: &DbConnection,
    table: &str,
    row_id: &str,
    column: &str,
    space_id: &str,
) -> (String, String, String) {
    let guard = db.0.lock().unwrap();
    let conn = guard.as_ref().unwrap();
    let raw: String = conn
        .query_row(
            &format!("SELECT haex_column_sigs FROM \"{table}\" WHERE id = ?1"),
            [row_id],
            |r| r.get(0),
        )
        .unwrap_or_else(|e| panic!("read haex_column_sigs for {table}.{row_id}: {e}"));
    let parsed: JsonValue = serde_json::from_str(&raw)
        .unwrap_or_else(|e| panic!("parse haex_column_sigs JSON: {e} raw={raw}"));
    let map = parsed
        .as_object()
        .unwrap_or_else(|| panic!("haex_column_sigs is not a JSON object: {parsed:?}"));
    let col_entry = map
        .get(column)
        .and_then(|v| v.as_object())
        .unwrap_or_else(|| panic!("no sig entry for column {column}, map={map:?}"));
    let space_entry = col_entry
        .get(space_id)
        .and_then(|v| v.as_object())
        .unwrap_or_else(|| panic!("no sig entry for space {space_id}, col_entry={col_entry:?}"));
    let author_did = space_entry
        .get("authorDid")
        .and_then(|v| v.as_str())
        .unwrap_or_else(|| panic!("missing author_did, space_entry={space_entry:?}"))
        .to_string();
    let sig = space_entry
        .get("sig")
        .and_then(|v| v.as_str())
        .unwrap_or_else(|| panic!("missing sig, space_entry={space_entry:?}"))
        .to_string();
    let storage_class = space_entry
        .get("storageClass")
        .and_then(|v| v.as_str())
        .unwrap_or_else(|| panic!("missing storageClass, space_entry={space_entry:?}"))
        .to_string();
    (author_did, sig, storage_class)
}

/// Read (column_hlc, row_hlc) from a sender row. Both are populated by the
/// CRDT AFTER-UPDATE trigger; the column-level HLC is what F1 signs over,
/// so the receiver must build its `RemoteColumnChange.hlc_timestamp` from
/// that field, not from the row-level `haex_hlc`.
fn read_column_hlc(db: &DbConnection, table: &str, row_id: &str, column: &str) -> String {
    let guard = db.0.lock().unwrap();
    let conn = guard.as_ref().unwrap();
    let raw: String = conn
        .query_row(
            &format!("SELECT haex_column_hlcs FROM \"{table}\" WHERE id = ?1"),
            [row_id],
            |r| r.get(0),
        )
        .unwrap();
    let parsed: JsonValue = serde_json::from_str(&raw).unwrap();
    parsed
        .as_object()
        .and_then(|m| m.get(column))
        .and_then(|v| v.as_str())
        .unwrap_or_else(|| panic!("missing haex_column_hlcs[{column}], raw={raw}"))
        .to_string()
}

#[test]
fn roundtrip_write_then_apply_verifies_and_creates_identity_stub() {
    // ---- Sender: sign a row via execute_with_crdt --------------------------
    let sender = setup_sender();
    let hlc_mutex = Mutex::new(sender.hlc);
    {
        let hlc_guard = hlc_mutex.lock().unwrap();
        execute_with_crdt(
            "UPDATE ext_calendar SET title = ?1 WHERE id = ?2".to_string(),
            vec![
                JsonValue::String("Hello from sender".to_string()),
                JsonValue::String("R".to_string()),
            ],
            &sender.db,
            &hlc_guard,
            &sender.cache,
        )
        .expect("sender UPDATE must succeed");
    }

    let (author_did, sig_b64, storage_class) =
        extract_sig(&sender.db, "ext_calendar", "R", "title", &sender.space_id);
    assert_eq!(
        author_did, sender.signer_did,
        "sig.author_did must match sender's identity DID"
    );
    assert_eq!(storage_class, "text");
    let column_hlc = read_column_hlc(&sender.db, "ext_calendar", "R", "title");
    assert!(
        !column_hlc.is_empty(),
        "column HLC must be populated by the CRDT trigger"
    );

    // ---- Wire hop: build RemoteColumnChanges -------------------------------
    // `ext_calendar` is an extension table: it has no `space_id` column, so
    // the receiver cannot read the row's space off the row. The anchor comes
    // from the caller instead — the space the pull was scoped to, passed as
    // `expected_space_id` below.
    //
    // It deliberately does NOT come from a `space_id` column change riding
    // along in the batch. That value is unsigned and fully attacker-chosen,
    // and using it would let a peer nominate the space its own signatures are
    // verified under (see `resolve_row_space_id_for_sig`).
    let rowpks = r#"{"id":"R"}"#.to_string();
    let title_change = RemoteColumnChange {
        table_name: "ext_calendar".to_string(),
        row_pks: rowpks.clone(),
        column_name: "title".to_string(),
        hlc_timestamp: column_hlc.clone(),
        decrypted_value: JsonValue::String("Hello from sender".to_string()),
        sig: Some(ColumnSig {
            author_did: author_did.clone(),
            sig: sig_b64.clone(),
            storage_class: haex_vault_lib::crdt::column_sig::value_bytes::StorageClass::Text,
        }),
    };
    // ---- Receiver: apply ----------------------------------------------------
    let receiver = setup_receiver();
    apply_remote_changes_to_db_scoped(
        &receiver,
        vec![title_change],
        None, // local delivery
        None, // no HLC advance service needed for the apply-side assertions
        Some(&sender.space_id),
    )
    .expect("receiver apply must succeed");

    // ---- Assertions --------------------------------------------------------
    let guard = receiver.0.lock().unwrap();
    let conn = guard.as_ref().unwrap();

    // 1. Row applied with the sender's plaintext value.
    let landed_title: String = conn
        .query_row("SELECT title FROM ext_calendar WHERE id = 'R'", [], |r| {
            r.get(0)
        })
        .expect("row must be inserted on receiver");
    assert_eq!(landed_title, "Hello from sender");

    // 2. Identity stub created for the sender's DID (empty at setup time).
    let stub_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM haex_identities WHERE did = ?1",
            [&sender.signer_did],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        stub_count, 1,
        "receiver must have created exactly one haex_identities stub for the sender DID"
    );

    // 3. No `haex_identities` rows for random other DIDs — invalid sigs
    //    must not seed the table (this is the negative half of the flow;
    //    covered symmetrically by the lib-side apply_rejects_change_* test).
    let total_identities: i64 = conn
        .query_row("SELECT COUNT(*) FROM haex_identities", [], |r| r.get(0))
        .unwrap();
    assert_eq!(
        total_identities, 1,
        "only the sender's stub identity may be present"
    );

    // 4. The verified signature metadata lands alongside the value so this
    // receiver can relay the change without re-signing it.
    let receiver_sigs: String = conn
        .query_row(
            "SELECT haex_column_sigs FROM ext_calendar WHERE id = 'R'",
            [],
            |r| r.get(0),
        )
        .expect("receiver must persist column signatures");
    let receiver_sigs: JsonValue =
        serde_json::from_str(&receiver_sigs).expect("receiver sig JSON must parse");
    let landed_sig = &receiver_sigs["title"][&sender.space_id];
    assert_eq!(landed_sig["authorDid"], author_did);
    assert_eq!(landed_sig["sig"], sig_b64);
    assert_eq!(landed_sig["storageClass"], storage_class);
}
