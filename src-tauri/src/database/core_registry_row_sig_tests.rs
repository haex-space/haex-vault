// src-tauri/src/database/core_registry_row_sig_tests.rs
//
// Integration tests for Task B.3: sign-on-write for `haex_shared_space_sync`
// registry rows within `execute_with_crdt`. Complements `core_execute_tests.rs`
// (F1 generic column-signing, F2 cross-table retro-sign) with the register
// row's own `row_sig` column, which covers the row's 11 identity fields
// (see `crdt::registry_row_sig::payload::RegistryRowSigPayload`).

use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use ed25519_dalek::SigningKey;
use rusqlite::Connection;
use serde_json::Value as JsonValue;
use std::sync::{Arc, Mutex};

use crate::crdt::column_sig::key_cache::SpaceKeyCache;
use crate::crdt::commands::apply::apply_remote_changes_to_db_scoped;
use crate::crdt::registry_row_sig::payload::RegistryRowSigPayload;
use crate::crdt::registry_row_sig::verify::verify_registry_row;
use crate::crdt::shared_space_trigger::{ensure_crdt_columns, DELETED_ROWS_TABLE};
use crate::crdt::space_scanner::scan_table_for_local_changes_scoped;
use crate::database::connection_context::ConnectionContext;
use crate::database::core::{self, install_tx_hlc_hooks, register_current_hlc_udf};
use crate::database::error::DatabaseError;
use crate::database::DbConnection;
use crate::space_delivery::local::sync_loop::local_to_remote_change;
use crate::table_names::{
    COL_SHARED_SPACE_SYNC_CREATED_AT, TABLE_CRDT_CONFIGS, TABLE_CRDT_DIRTY_TABLES,
    TABLE_CRDT_PENDING_COLUMNS, TABLE_S3_BACKENDS, TABLE_SHARED_SPACE_SYNC,
};
use crate::ucan::verify::did_key_from_public_key;
use haex_crdt::HlcService;

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
    did_alice: String,
}

/// In-memory DB with just enough schema for the registry-row sign-on-write
/// path: CRDT config/dirty-tables/UDF wiring, one owned identity ("alice")
/// that's a member of "space_1", and the full `haex_shared_space_sync`
/// schema (mirrors migrations 0000_jazzy_chat + 0014_registry_authorization_schema).
fn setup_fixture() -> Fixture {
    let conn = Connection::open_in_memory().expect("in-memory DB");

    let hlc = HlcService::new_with_uuid(
        crate::haex_crdt_providers::device_id::test_device_uuid_from_name("test-device-b3"),
    );
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

    // Identity + membership schema (I2 filter reads these) + the full
    // registry table shape (extension_public_key/extension_name nullable
    // + paired, category/type/labels nullable, authored_by_did/row_sig
    // default '' NOT NULL — matches migrations 0000 + 0014).
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
            extension_public_key TEXT,
            extension_name TEXT,
            category TEXT,
            type TEXT,
            type_label TEXT,
            category_label TEXT,
            authored_by_did TEXT DEFAULT '' NOT NULL,
            row_sig TEXT DEFAULT '' NOT NULL,
            created_at_no_sync TEXT DEFAULT (CURRENT_TIMESTAMP)
         );",
    )
    .unwrap();

    let key_alice = random_key();
    let did_alice = did_key_from_public_key(&key_alice.verifying_key());
    conn.execute(
        "INSERT INTO haex_identities (id, did, private_key) VALUES (?1, ?2, ?3)",
        rusqlite::params!["id-alice", &did_alice, pkcs8_b64(&key_alice)],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO haex_space_members (id, space_id, identity_id) VALUES (?1, ?2, ?3)",
        ["mem-alice", "space_1", "id-alice"],
    )
    .unwrap();

    {
        let tx = conn.unchecked_transaction().unwrap();
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
        did_alice,
    }
}

/// A SECOND, independent in-memory DB — a fresh peer that has never seen any
/// row this test inserts on the sender (`setup_fixture`'s DB). Unlike
/// `setup_fixture`, this carries no HLC UDF / tx-hook wiring and no signing
/// key cache: a receiving peer applies incoming changes via
/// `apply_remote_changes_to_db_scoped` directly (raw SQL, not
/// `execute_with_crdt`) and verifies `row_sig`/per-column sigs purely from
/// the DIDs carried on the wire, so it needs neither. Schema mirrors
/// `crdt::commands_apply_registry_row_sig_tests::setup_registry_db` — the
/// existing fixture already proven to work with the real apply pipeline.
fn setup_fresh_peer_db() -> DbConnection {
    let conn = Connection::open_in_memory().expect("in-memory DB");
    conn.execute_batch(&format!(
        "CREATE TABLE {TABLE_CRDT_CONFIGS} (key TEXT PRIMARY KEY, type TEXT, value TEXT);
         CREATE TABLE {DELETED_ROWS_TABLE} (
             id TEXT PRIMARY KEY,
             table_name TEXT NOT NULL,
             row_pks TEXT NOT NULL,
             haex_hlc_no_sync TEXT,
             haex_column_hlcs_no_sync TEXT NOT NULL DEFAULT '{{}}'
         );
         CREATE TABLE {TABLE_CRDT_PENDING_COLUMNS} (
             table_name TEXT NOT NULL,
             column_name TEXT NOT NULL,
             row_pks TEXT NOT NULL,
             PRIMARY KEY(table_name, column_name, row_pks)
         );
         CREATE TABLE haex_identities (
             id TEXT PRIMARY KEY NOT NULL,
             did TEXT NOT NULL,
             name TEXT NOT NULL,
             source TEXT DEFAULT 'contact' NOT NULL
         );
         CREATE UNIQUE INDEX haex_identities_did_unique ON haex_identities (did);
         CREATE TABLE {TABLE_SHARED_SPACE_SYNC} (
            id TEXT PRIMARY KEY NOT NULL,
            table_name TEXT NOT NULL,
            row_pks TEXT NOT NULL,
            space_id TEXT NOT NULL,
            extension_public_key TEXT,
            extension_name TEXT,
            category TEXT,
            type TEXT,
            type_label TEXT,
            category_label TEXT,
            authored_by_did TEXT DEFAULT '' NOT NULL,
            row_sig TEXT DEFAULT '' NOT NULL,
            created_at_no_sync TEXT DEFAULT (CURRENT_TIMESTAMP)
         );"
    ))
    .unwrap();
    {
        let tx = conn.unchecked_transaction().unwrap();
        ensure_crdt_columns(&tx, TABLE_SHARED_SPACE_SYNC).unwrap();
        tx.commit().unwrap();
    }
    DbConnection(Arc::new(Mutex::new(Some(conn))))
}

/// A `haex_shared_space_sync` row as read back from the DB, for assertions
/// and for rebuilding the exact payload the sign-on-write pass should have
/// signed.
struct StoredRow {
    id: String,
    space_id: String,
    table_name: String,
    row_pks: String,
    extension_public_key: Option<String>,
    extension_name: Option<String>,
    category: Option<String>,
    r#type: Option<String>,
    category_label: Option<String>,
    type_label: Option<String>,
    authored_by_did: String,
    row_sig: String,
}

impl StoredRow {
    fn payload(&self) -> RegistryRowSigPayload<'_> {
        RegistryRowSigPayload {
            id: &self.id,
            space_id: &self.space_id,
            table_name: &self.table_name,
            row_pks: &self.row_pks,
            extension_public_key: self.extension_public_key.as_deref(),
            extension_name: self.extension_name.as_deref(),
            category: self.category.as_deref(),
            r#type: self.r#type.as_deref(),
            category_label: self.category_label.as_deref(),
            type_label: self.type_label.as_deref(),
            authored_by_did: &self.authored_by_did,
        }
    }
}

fn load_row(db: &DbConnection, id: &str) -> StoredRow {
    let guard = db.0.lock().unwrap();
    let conn = guard.as_ref().unwrap();
    conn.query_row(
        "SELECT id, space_id, table_name, row_pks, extension_public_key, extension_name, \
                category, type, category_label, type_label, authored_by_did, row_sig \
         FROM haex_shared_space_sync WHERE id = ?1",
        [id],
        |r| {
            Ok(StoredRow {
                id: r.get(0)?,
                space_id: r.get(1)?,
                table_name: r.get(2)?,
                row_pks: r.get(3)?,
                extension_public_key: r.get(4)?,
                extension_name: r.get(5)?,
                category: r.get(6)?,
                r#type: r.get(7)?,
                category_label: r.get(8)?,
                type_label: r.get(9)?,
                authored_by_did: r.get(10)?,
                row_sig: r.get(11)?,
            })
        },
    )
    .unwrap()
}

fn insert_minimal_row(f: &Fixture, id: &str, row_pks: &str) {
    let hlc_mutex = Mutex::new(f.hlc.clone());
    let hlc_guard = hlc_mutex.lock().unwrap();
    core::execute_with_crdt(
        "INSERT INTO haex_shared_space_sync (id, table_name, row_pks, space_id) \
         VALUES (?1, ?2, ?3, ?4)"
            .to_string(),
        vec![
            JsonValue::String(id.to_string()),
            JsonValue::String("ext_calendar".to_string()),
            JsonValue::String(row_pks.to_string()),
            JsonValue::String("space_1".to_string()),
        ],
        &f.db,
        &hlc_guard,
        &f.cache,
    )
    .expect("minimal insert succeeds");
}

#[test]
fn test_execute_with_crdt_signs_registry_row_on_insert() {
    let f = setup_fixture();
    let hlc_mutex = Mutex::new(f.hlc.clone());
    let hlc_guard = hlc_mutex.lock().unwrap();

    core::execute_with_crdt(
        "INSERT INTO haex_shared_space_sync \
            (id, table_name, row_pks, space_id, authored_by_did) \
         VALUES (?1, ?2, ?3, ?4, ?5)"
            .to_string(),
        vec![
            JsonValue::String("row-1".to_string()),
            JsonValue::String("ext_calendar".to_string()),
            JsonValue::String(r#"{"id":"evt-1"}"#.to_string()),
            JsonValue::String("space_1".to_string()),
            JsonValue::String(f.did_alice.clone()),
        ],
        &f.db,
        &hlc_guard,
        &f.cache,
    )
    .expect("insert succeeds");
    drop(hlc_guard);

    let row = load_row(&f.db, "row-1");
    assert!(!row.row_sig.is_empty(), "row_sig must be populated");

    let sig_bytes = BASE64
        .decode(&row.row_sig)
        .expect("row_sig is valid base64");
    let pk = f
        .cache
        .get("space_1")
        .expect("space_1 key cached")
        .verifying_key();
    assert!(
        verify_registry_row(&row.payload(), &sig_bytes, &pk).is_ok(),
        "row_sig must verify against the persisted payload"
    );
}

/// PR #741 finding 3 (CRITICAL): `persist_shared_backend`
/// (`remote_storage::share_command`) writes `row_pks` as a JSON ARRAY
/// (`["<uuid>"]`), not the JSON-object shape the CRDT scanner produces for
/// every other table. Before the fix, `canonicalize_row_pks` only accepted
/// objects, so this INSERT aborted the whole write with a deserialize error.
/// Mirrors `persist_shared_backend`'s exact `row_pks` construction.
#[test]
fn test_execute_with_crdt_signs_registry_row_with_array_row_pks() {
    let f = setup_fixture();
    let row_pks_json = serde_json::to_string(&vec!["backend-id-1"]).unwrap();
    insert_minimal_row(&f, "row-array", &row_pks_json);

    let row = load_row(&f.db, "row-array");
    assert_eq!(row.row_pks, r#"["backend-id-1"]"#);
    assert!(!row.row_sig.is_empty(), "row_sig must be populated");

    let sig_bytes = BASE64
        .decode(&row.row_sig)
        .expect("row_sig is valid base64");
    let pk = f
        .cache
        .get("space_1")
        .expect("space_1 key cached")
        .verifying_key();
    assert!(
        verify_registry_row(&row.payload(), &sig_bytes, &pk).is_ok(),
        "row_sig must verify against the persisted payload"
    );
}

#[test]
fn test_execute_with_crdt_auto_populates_authored_by_did_when_missing() {
    let f = setup_fixture();
    // No `authored_by_did` in the column list at all — the DB default ''
    // applies, and the sign-on-write pass must auto-populate it.
    insert_minimal_row(&f, "row-2", r#"{"id":"evt-2"}"#);

    let row = load_row(&f.db, "row-2");
    assert_eq!(row.authored_by_did, f.did_alice);
    assert!(!row.row_sig.is_empty());
}

#[test]
fn test_execute_with_crdt_rejects_registry_write_with_foreign_authored_by_did() {
    let f = setup_fixture();
    let hlc_mutex = Mutex::new(f.hlc.clone());
    let hlc_guard = hlc_mutex.lock().unwrap();

    let result = core::execute_with_crdt(
        "INSERT INTO haex_shared_space_sync \
            (id, table_name, row_pks, space_id, authored_by_did) \
         VALUES (?1, ?2, ?3, ?4, ?5)"
            .to_string(),
        vec![
            JsonValue::String("row-evil".to_string()),
            JsonValue::String("ext_calendar".to_string()),
            JsonValue::String(r#"{"id":"evt-evil"}"#.to_string()),
            JsonValue::String("space_1".to_string()),
            // Not this vault's DID for space_1 — cannot forge foreign
            // authorship on a local write.
            JsonValue::String("did:key:mallory".to_string()),
        ],
        &f.db,
        &hlc_guard,
        &f.cache,
    );

    assert!(
        matches!(
            result,
            Err(DatabaseError::RegistryRowForeignAuthoredByDid { .. })
        ),
        "expected RegistryRowForeignAuthoredByDid, got: {:?}",
        result
    );
    drop(hlc_guard);

    let guard = f.db.0.lock().unwrap();
    let conn = guard.as_ref().unwrap();
    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM haex_shared_space_sync WHERE id = 'row-evil'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(count, 0, "forged-authorship INSERT must roll back");
}

#[test]
fn test_execute_with_crdt_rejects_authored_by_did_update() {
    let f = setup_fixture();
    insert_minimal_row(&f, "row-3", r#"{"id":"evt-3"}"#);

    let hlc_mutex = Mutex::new(f.hlc.clone());
    let hlc_guard = hlc_mutex.lock().unwrap();
    let result = core::execute_with_crdt(
        "UPDATE haex_shared_space_sync SET authored_by_did = ?1 WHERE id = ?2".to_string(),
        vec![
            JsonValue::String("did:key:bob".to_string()),
            JsonValue::String("row-3".to_string()),
        ],
        &f.db,
        &hlc_guard,
        &f.cache,
    );

    assert!(
        matches!(
            result,
            Err(DatabaseError::RegistryRowAuthoredByDidImmutable { .. })
        ),
        "expected RegistryRowAuthoredByDidImmutable, got: {:?}",
        result
    );
    drop(hlc_guard);

    let row = load_row(&f.db, "row-3");
    assert_eq!(
        row.authored_by_did, f.did_alice,
        "rejected UPDATE must roll back, authored_by_did stays as auto-populated"
    );
}

#[test]
fn test_execute_with_crdt_resigns_on_payload_column_update() {
    let f = setup_fixture();
    let hlc_mutex = Mutex::new(f.hlc.clone());
    {
        let hlc_guard = hlc_mutex.lock().unwrap();
        core::execute_with_crdt(
            "INSERT INTO haex_shared_space_sync (id, table_name, row_pks, space_id, category) \
             VALUES (?1, ?2, ?3, ?4, ?5)"
                .to_string(),
            vec![
                JsonValue::String("row-4".to_string()),
                JsonValue::String("ext_calendar".to_string()),
                JsonValue::String(r#"{"id":"evt-4"}"#.to_string()),
                JsonValue::String("space_1".to_string()),
                JsonValue::String("work".to_string()),
            ],
            &f.db,
            &hlc_guard,
            &f.cache,
        )
        .expect("insert succeeds");
    }
    let original_sig = load_row(&f.db, "row-4").row_sig;
    assert!(!original_sig.is_empty());

    let hlc_guard = hlc_mutex.lock().unwrap();
    core::execute_with_crdt(
        "UPDATE haex_shared_space_sync SET category = ?1 WHERE id = ?2".to_string(),
        vec![
            JsonValue::String("leisure".to_string()),
            JsonValue::String("row-4".to_string()),
        ],
        &f.db,
        &hlc_guard,
        &f.cache,
    )
    .expect("update succeeds");
    drop(hlc_guard);

    let row = load_row(&f.db, "row-4");
    assert_eq!(row.category.as_deref(), Some("leisure"));
    assert_ne!(
        row.row_sig, original_sig,
        "changed payload field must re-sign"
    );

    let sig_bytes = BASE64.decode(&row.row_sig).unwrap();
    let pk = f.cache.get("space_1").unwrap().verifying_key();
    assert!(
        verify_registry_row(&row.payload(), &sig_bytes, &pk).is_ok(),
        "new sig must verify against the new payload"
    );
}

#[test]
fn test_execute_with_crdt_does_not_resign_on_sync_meta_only_update() {
    // Every non-CRDT-meta column of this table is either one of the 12
    // signed fields or `row_sig` itself (rejected as a direct write, see
    // `test_execute_with_crdt_rejects_direct_row_sig_write`) — so there is
    // no legitimate `execute_with_crdt` call that touches only sync-meta
    // columns; `CrdtMetaColumnWriteForbidden` already rejects any caller
    // write to haex_hlc_no_sync/haex_column_hlcs_no_sync/haex_column_sigs_no_sync regardless of
    // table. The realistic equivalent of "a CRDT-internal update touches
    // sync meta" is a raw connection write, exactly like the CRDT-apply
    // path (`apply_remote_changes_to_db_scoped`) uses when merging remote
    // state — it does not go through `execute_with_crdt` either. Assert
    // that such a write leaves `row_sig` untouched: the sign-on-write pass
    // only ever fires from inside `execute_with_crdt`.
    let f = setup_fixture();
    insert_minimal_row(&f, "row-5", r#"{"id":"evt-5"}"#);
    let original_sig = load_row(&f.db, "row-5").row_sig;
    assert!(!original_sig.is_empty());

    {
        let guard = f.db.0.lock().unwrap();
        let conn = guard.as_ref().unwrap();
        conn.execute(
            "UPDATE haex_shared_space_sync SET haex_hlc_no_sync = 'fake-remote-hlc' WHERE id = 'row-5'",
            [],
        )
        .unwrap();
    }

    let row = load_row(&f.db, "row-5");
    assert_eq!(
        row.row_sig, original_sig,
        "raw meta-only write must not resign"
    );
}

// ---------------------------------------------------------------------------
// Additional guards discovered while implementing B.3 (not in the original
// task's test list, but required for the stated invariants to actually hold
// — see report Concerns/Deviations).
// ---------------------------------------------------------------------------

#[test]
fn test_execute_with_crdt_rejects_direct_row_sig_write() {
    // row_sig is derived exclusively by the sign-on-write pass. Without this
    // guard, an UPDATE that touches only `row_sig` (the one column on this
    // table that is neither a signed field nor forbidden CRDT meta) would
    // skip re-signing entirely and let a caller-supplied value straight
    // through — a forgery/replay vector.
    let f = setup_fixture();
    insert_minimal_row(&f, "row-6", r#"{"id":"evt-6"}"#);

    let hlc_mutex = Mutex::new(f.hlc.clone());
    let hlc_guard = hlc_mutex.lock().unwrap();
    let result = core::execute_with_crdt(
        "UPDATE haex_shared_space_sync SET row_sig = ?1 WHERE id = ?2".to_string(),
        vec![
            JsonValue::String("forged-sig".to_string()),
            JsonValue::String("row-6".to_string()),
        ],
        &f.db,
        &hlc_guard,
        &f.cache,
    );

    assert!(
        matches!(
            result,
            Err(DatabaseError::RegistryRowSigColumnWriteForbidden { .. })
        ),
        "expected RegistryRowSigColumnWriteForbidden, got: {:?}",
        result
    );
}

#[test]
fn test_execute_with_crdt_rejects_registry_insert_for_space_without_local_key() {
    // Mirrors F2's I2: holding the space's signing key IS the authorization
    // to author a registry row for it. `space_UNKNOWN` has no member row in
    // the fixture, so the vault holds no key for it.
    let f = setup_fixture();
    let hlc_mutex = Mutex::new(f.hlc.clone());
    let hlc_guard = hlc_mutex.lock().unwrap();

    let result = core::execute_with_crdt(
        "INSERT INTO haex_shared_space_sync (id, table_name, row_pks, space_id) \
         VALUES (?1, ?2, ?3, ?4)"
            .to_string(),
        vec![
            JsonValue::String("row-unowned".to_string()),
            JsonValue::String("ext_calendar".to_string()),
            JsonValue::String(r#"{"id":"evt-unowned"}"#.to_string()),
            JsonValue::String("space_UNKNOWN".to_string()),
        ],
        &f.db,
        &hlc_guard,
        &f.cache,
    );

    assert!(
        matches!(result, Err(DatabaseError::I2ForeignShareInsert { .. })),
        "expected I2ForeignShareInsert, got: {:?}",
        result
    );
}

#[test]
fn test_execute_with_crdt_canonicalizes_row_pks_before_signing() {
    // Concern 2 (Task B.3): row_pks must be canonical JSON before it is
    // signed and persisted — RegisterLookup::resolve compares against it
    // with exact-string equality, so two callers writing the same PK set in
    // different key orders must land on one shared, canonical form.
    let f = setup_fixture();
    let hlc_mutex = Mutex::new(f.hlc.clone());
    let hlc_guard = hlc_mutex.lock().unwrap();

    core::execute_with_crdt(
        "INSERT INTO haex_shared_space_sync (id, table_name, row_pks, space_id) \
         VALUES (?1, ?2, ?3, ?4)"
            .to_string(),
        vec![
            JsonValue::String("row-7".to_string()),
            JsonValue::String("ext_multi_pk".to_string()),
            JsonValue::String(r#"{"b":2,"a":1}"#.to_string()),
            JsonValue::String("space_1".to_string()),
        ],
        &f.db,
        &hlc_guard,
        &f.cache,
    )
    .expect("insert succeeds");
    drop(hlc_guard);

    let row = load_row(&f.db, "row-7");
    assert_eq!(
        row.row_pks, r#"{"a":1,"b":2}"#,
        "row_pks must be persisted in canonical (sorted-key) form"
    );

    let sig_bytes = BASE64.decode(&row.row_sig).unwrap();
    let pk = f.cache.get("space_1").unwrap().verifying_key();
    assert!(
        verify_registry_row(&row.payload(), &sig_bytes, &pk).is_ok(),
        "sig must verify against the canonicalised row_pks actually persisted"
    );
}

// ---------------------------------------------------------------------------
// Case-insensitivity regression (spec-review Critical finding): SQL column
// identifiers are case-insensitive, but the guards above matched touched
// column names with `==` against lowercase constants — `SET ROW_SIG = …`
// bypassed `RegistryRowSigColumnWriteForbidden` and `SET AUTHORED_BY_DID = …`
// bypassed `RegistryRowAuthoredByDidImmutable`. Fixed by case-folding column
// identifiers once, in `extract_touched_for_signing`.
// ---------------------------------------------------------------------------

#[test]
fn test_execute_with_crdt_rejects_uppercase_row_sig_write() {
    let f = setup_fixture();
    insert_minimal_row(&f, "row-8", r#"{"id":"evt-8"}"#);

    let hlc_mutex = Mutex::new(f.hlc.clone());
    let hlc_guard = hlc_mutex.lock().unwrap();
    let result = core::execute_with_crdt(
        "UPDATE haex_shared_space_sync SET ROW_SIG = ?1 WHERE id = ?2".to_string(),
        vec![
            JsonValue::String("totally-forged-sig".to_string()),
            JsonValue::String("row-8".to_string()),
        ],
        &f.db,
        &hlc_guard,
        &f.cache,
    );

    assert!(
        matches!(
            result,
            Err(DatabaseError::RegistryRowSigColumnWriteForbidden { .. })
        ),
        "uppercase ROW_SIG must still be rejected, got: {:?}",
        result
    );
}

#[test]
fn test_execute_with_crdt_rejects_mixedcase_row_sig_write() {
    let f = setup_fixture();
    insert_minimal_row(&f, "row-9", r#"{"id":"evt-9"}"#);

    let hlc_mutex = Mutex::new(f.hlc.clone());
    let hlc_guard = hlc_mutex.lock().unwrap();
    let result = core::execute_with_crdt(
        "UPDATE haex_shared_space_sync SET Row_Sig = ?1 WHERE id = ?2".to_string(),
        vec![
            JsonValue::String("totally-forged-sig".to_string()),
            JsonValue::String("row-9".to_string()),
        ],
        &f.db,
        &hlc_guard,
        &f.cache,
    );

    assert!(
        matches!(
            result,
            Err(DatabaseError::RegistryRowSigColumnWriteForbidden { .. })
        ),
        "mixed-case Row_Sig must still be rejected, got: {:?}",
        result
    );
}

#[test]
fn test_execute_with_crdt_rejects_uppercase_authored_by_did_update() {
    let f = setup_fixture();
    insert_minimal_row(&f, "row-10", r#"{"id":"evt-10"}"#);

    let hlc_mutex = Mutex::new(f.hlc.clone());
    let hlc_guard = hlc_mutex.lock().unwrap();
    let result = core::execute_with_crdt(
        "UPDATE haex_shared_space_sync SET AUTHORED_BY_DID = ?1 WHERE id = ?2".to_string(),
        vec![
            JsonValue::String("did:key:bob".to_string()),
            JsonValue::String("row-10".to_string()),
        ],
        &f.db,
        &hlc_guard,
        &f.cache,
    );

    assert!(
        matches!(
            result,
            Err(DatabaseError::RegistryRowAuthoredByDidImmutable { .. })
        ),
        "uppercase AUTHORED_BY_DID must still be rejected, got: {:?}",
        result
    );
}

#[test]
fn test_execute_with_crdt_rejects_mixedcase_authored_by_did_update() {
    let f = setup_fixture();
    insert_minimal_row(&f, "row-11", r#"{"id":"evt-11"}"#);

    let hlc_mutex = Mutex::new(f.hlc.clone());
    let hlc_guard = hlc_mutex.lock().unwrap();
    let result = core::execute_with_crdt(
        "UPDATE haex_shared_space_sync SET authored_By_did = ?1 WHERE id = ?2".to_string(),
        vec![
            JsonValue::String("did:key:bob".to_string()),
            JsonValue::String("row-11".to_string()),
        ],
        &f.db,
        &hlc_guard,
        &f.cache,
    );

    assert!(
        matches!(
            result,
            Err(DatabaseError::RegistryRowAuthoredByDidImmutable { .. })
        ),
        "mixed-case authored_By_did must still be rejected, got: {:?}",
        result
    );
}

#[test]
fn test_execute_with_crdt_signs_registry_row_when_table_name_uppercase() {
    // Same threat model as the column-case bypass: the register-table
    // dispatch check in sign_registry_row_self compares the extracted table
    // name against TABLE_SHARED_SPACE_SYNC. An uppercase table name must not
    // let the whole sign pass be silently skipped.
    let f = setup_fixture();
    let hlc_mutex = Mutex::new(f.hlc.clone());
    let hlc_guard = hlc_mutex.lock().unwrap();

    core::execute_with_crdt(
        "INSERT INTO HAEX_SHARED_SPACE_SYNC (id, table_name, row_pks, space_id) \
         VALUES (?1, ?2, ?3, ?4)"
            .to_string(),
        vec![
            JsonValue::String("row-12".to_string()),
            JsonValue::String("ext_calendar".to_string()),
            JsonValue::String(r#"{"id":"evt-12"}"#.to_string()),
            JsonValue::String("space_1".to_string()),
        ],
        &f.db,
        &hlc_guard,
        &f.cache,
    )
    .expect("insert succeeds even with uppercase table name");
    drop(hlc_guard);

    let row = load_row(&f.db, "row-12");
    assert!(
        !row.row_sig.is_empty(),
        "uppercase table name must not bypass sign_registry_row_self"
    );
}

#[test]
fn test_execute_with_crdt_signs_registry_row_when_table_name_mixedcase() {
    let f = setup_fixture();
    let hlc_mutex = Mutex::new(f.hlc.clone());
    let hlc_guard = hlc_mutex.lock().unwrap();

    core::execute_with_crdt(
        "INSERT INTO Haex_Shared_Space_Sync (id, table_name, row_pks, space_id) \
         VALUES (?1, ?2, ?3, ?4)"
            .to_string(),
        vec![
            JsonValue::String("row-13".to_string()),
            JsonValue::String("ext_calendar".to_string()),
            JsonValue::String(r#"{"id":"evt-13"}"#.to_string()),
            JsonValue::String("space_1".to_string()),
        ],
        &f.db,
        &hlc_guard,
        &f.cache,
    )
    .expect("insert succeeds even with mixed-case table name");
    drop(hlc_guard);

    let row = load_row(&f.db, "row-13");
    assert!(
        !row.row_sig.is_empty(),
        "mixed-case table name must not bypass sign_registry_row_self"
    );
}

// Note: this file previously carried `test_execute_with_crdt_resigns_row_with_null_created_at`
// (PR #741 finding 8), which simulated a persisted NULL `created_at_no_sync`
// and confirmed a payload-signed UPDATE still succeeded instead of erroring
// out on the fetch. That guard is gone as of the fix removing
// `created_at_no_sync` from `RegistryRowSigPayload` entirely: the sign-on-write
// SELECT (`database::core::execute::sign_registry_row_self`) no longer reads
// this column at all, so a NULL persisted value can no longer affect the
// fetch or the signature — there is nothing left for that scenario to guard
// against. `created_at_no_sync` remains a real, nullable column; only its
// participation in the signed payload was removed.

// ---------------------------------------------------------------------------
// Production-path roundtrip regression test.
//
// The bug: the real production INSERT (`remote_storage::share_command`)
// omits `created_at_no_sync` from its column list, so SQLite fills it from
// the schema default. The sign-on-write chokepoint used to read that
// persisted value and include it in `RegistryRowSigPayload` — but
// `created_at_no_sync` ends in `_no_sync`, so the real crate scanner never
// ships it to a peer. A fresh peer receiving the row reconstructed the
// field as `None` while the signer had signed `Some(...)`, so every real
// share silently failed Ed25519 verification on every fresh peer and was
// dropped without error. Fixed by removing `created_at_no_sync` from the
// signed payload entirely (see `crdt::registry_row_sig::payload`).
//
// This test proves the fix closes the gap end to end, using only real
// production machinery: the real `execute_with_crdt` chokepoint with
// production's exact INSERT column list, the real crate scanner, the real
// wire-conversion function, and the real apply pipeline on a second, fresh
// peer DB that has never seen the row.
// ---------------------------------------------------------------------------

#[test]
fn real_scanner_roundtrip_survives_a_fresh_peer() {
    let f = setup_fixture();

    // Step 1/2: insert exactly the column list `remote_storage::share_command`'s
    // real INSERT uses (`mod.rs`'s `insert_mapping` SQL) — omitting
    // `created_at_no_sync`, `authored_by_did` and `row_sig` so the DB
    // defaults populate them, matching real production behaviour rather
    // than a test shortcut.
    let row_pks_json = serde_json::to_string(&vec!["backend-1"]).unwrap();
    {
        let hlc_mutex = Mutex::new(f.hlc.clone());
        let hlc_guard = hlc_mutex.lock().unwrap();
        core::execute_with_crdt(
            format!(
                "INSERT INTO {TABLE_SHARED_SPACE_SYNC} \
                 (id, table_name, row_pks, space_id, extension_public_key, extension_name, \
                  category, type, type_label) \
                 VALUES (?1, ?2, ?3, ?4, NULL, NULL, NULL, ?5, ?6)"
            ),
            vec![
                JsonValue::String("reg-roundtrip-1".to_string()),
                JsonValue::String(TABLE_S3_BACKENDS.to_string()),
                JsonValue::String(row_pks_json),
                JsonValue::String("space_1".to_string()),
                JsonValue::String("cloud_storage".to_string()),
                JsonValue::String("My Share".to_string()),
            ],
            &f.db,
            &hlc_guard,
            &f.cache,
        )
        .expect("production-shaped insert succeeds");
    }

    let sender_row = load_row(&f.db, "reg-roundtrip-1");
    assert!(
        !sender_row.row_sig.is_empty(),
        "row must be self-signed on insert"
    );
    assert_eq!(sender_row.authored_by_did, f.did_alice);

    // Step 3: scan with the REAL crate scanner, not a hand-built
    // `RemoteColumnChange` list.
    let changes = {
        let guard = f.db.0.lock().unwrap();
        let conn = guard.as_ref().unwrap();
        scan_table_for_local_changes_scoped(
            conn,
            TABLE_SHARED_SPACE_SYNC,
            None,
            "test-device",
            Some("space_1"),
            None,
        )
        .expect("scan succeeds")
    };
    let col_names: Vec<&str> = changes.iter().map(|c| c.column_name.as_str()).collect();
    assert!(
        !col_names.contains(&COL_SHARED_SPACE_SYNC_CREATED_AT),
        "sanity check: the real scanner must never ship created_at_no_sync — \
         if it did, this test would not be exercising the actual production bug"
    );
    assert_eq!(
        col_names.len(),
        11,
        "expected the 11 non-PK, non-_no_sync columns (10 payload fields + row_sig)"
    );

    // Step 4: convert via the real wire-conversion function.
    let remote_changes: Vec<_> = changes.iter().map(local_to_remote_change).collect();

    // Step 5: a SECOND, fresh peer DB that has never seen this row. `Some(space_id)`
    // matches the real peer-pull call site (`space_delivery::local::sync_loop::pull`),
    // which always passes the space it pulled for.
    let receiver_db = setup_fresh_peer_db();
    apply_remote_changes_to_db_scoped(&receiver_db, remote_changes, None, None, Some("space_1"))
        .expect("apply must not error — a rejection would be row-scoped, not fatal");

    // Step 6: the row DOES land on the fresh peer — this is what this task's
    // fix guarantees: before the fix, B.5's row-sig gate rejected the WHOLE
    // row (created_at_no_sync payload mismatch → SignatureInvalid) and the
    // row was silently absent. All business fields survive intact.
    let received = load_row(&receiver_db, "reg-roundtrip-1");
    assert_eq!(received.space_id, "space_1");
    assert_eq!(received.table_name, TABLE_S3_BACKENDS);
    assert_eq!(received.row_pks, r#"["backend-1"]"#);
    assert_eq!(received.r#type.as_deref(), Some("cloud_storage"));
    assert_eq!(received.type_label.as_deref(), Some("My Share"));
    assert_eq!(
        received.authored_by_did, f.did_alice,
        "authored_by_did must survive the roundtrip"
    );

    // `row_sig` now survives the roundtrip: `sign_registry_row_self` writes
    // a per-column signature for `row_sig` immediately after its raw
    // UPDATE, so the scanner ships the `row_sig` column change with a
    // valid signature and the receiver's per-column signature gate
    // (`verify_change_sig` in `crdt/commands/apply/policy.rs`,
    // `enforce_sigs` branch) accepts it instead of dropping it as
    // unsigned. The receiver's persisted `row_sig` must additionally
    // verify against the received payload — a matching-but-invalid
    // signature would be a regression against the tamper-detection
    // guarantee the column exists for in the first place.
    assert!(
        !received.row_sig.is_empty(),
        "row_sig must survive a real space-scoped apply"
    );
    let received_sig_bytes = BASE64
        .decode(&received.row_sig)
        .expect("received row_sig is valid base64");
    let sender_verifying_key = f
        .cache
        .get("space_1")
        .expect("sender's space_1 signing key cached")
        .verifying_key();
    assert!(
        verify_registry_row(
            &received.payload(),
            &received_sig_bytes,
            &sender_verifying_key
        )
        .is_ok(),
        "row_sig must verify against the received payload under the sender's space_1 key"
    );
}
