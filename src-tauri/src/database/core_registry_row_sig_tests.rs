// src-tauri/src/database/core_registry_row_sig_tests.rs
//
// Integration tests for Task B.3: sign-on-write for `haex_shared_space_sync`
// registry rows within `execute_with_crdt`. Complements `core_execute_tests.rs`
// (F1 generic column-signing, F2 cross-table retro-sign) with the register
// row's own `row_sig` column, which covers the row's 12 identity fields
// (see `crdt::registry_row_sig::payload::RegistryRowSigPayload`).

use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use ed25519_dalek::SigningKey;
use rusqlite::Connection;
use serde_json::Value as JsonValue;
use std::sync::{Arc, Mutex};

use crate::crdt::column_sig::key_cache::SpaceKeyCache;
use crate::crdt::hlc::HlcService;
use crate::crdt::registry_row_sig::payload::RegistryRowSigPayload;
use crate::crdt::registry_row_sig::verify::verify_registry_row;
use crate::crdt::trigger::ensure_crdt_columns;
use crate::database::connection_context::ConnectionContext;
use crate::database::core::{self, install_tx_hlc_hooks, register_current_hlc_udf};
use crate::database::error::DatabaseError;
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
    did_alice: String,
}

/// In-memory DB with just enough schema for the registry-row sign-on-write
/// path: CRDT config/dirty-tables/UDF wiring, one owned identity ("alice")
/// that's a member of "space_1", and the full `haex_shared_space_sync`
/// schema (mirrors migrations 0000_jazzy_chat + 0014_registry_authorization_schema).
fn setup_fixture() -> Fixture {
    let conn = Connection::open_in_memory().expect("in-memory DB");

    let hlc = HlcService::new_for_testing("test-device-b3");
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
            created_at TEXT DEFAULT (CURRENT_TIMESTAMP)
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
    created_at: String,
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
            created_at: &self.created_at,
        }
    }
}

fn load_row(db: &DbConnection, id: &str) -> StoredRow {
    let guard = db.0.lock().unwrap();
    let conn = guard.as_ref().unwrap();
    conn.query_row(
        "SELECT id, space_id, table_name, row_pks, extension_public_key, extension_name, \
                category, type, category_label, type_label, authored_by_did, created_at, row_sig \
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
                created_at: r.get(11)?,
                row_sig: r.get(12)?,
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
    // write to haex_hlc/haex_column_hlcs/haex_column_sigs regardless of
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
            "UPDATE haex_shared_space_sync SET haex_hlc = 'fake-remote-hlc' WHERE id = 'row-5'",
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
