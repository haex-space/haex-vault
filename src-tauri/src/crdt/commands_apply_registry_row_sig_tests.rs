//! Task B.5 integration tests — end-to-end apply-pipeline coverage for the
//! `haex_shared_space_sync` row-sig gate ("Stage 5b"), wired into
//! `apply_remote_changes_to_db_scoped` (`apply/db.rs`) via
//! `apply::registry_row_gate`.
//!
//! Complements the pure-function unit tests in
//! `registry_row_sig::puller_verify_tests` (Task B.4) — which never touch a
//! database — with the real DB-apply wiring: batching per-column
//! `RemoteColumnChange`s into an `IncomingRegistryChange`, the persisted-row
//! fallback for UPDATEs, and dropping the whole row atomically on a bad
//! `row_sig`.

use super::*;
use crate::crdt::column_sig::sign::sign_column;
use crate::crdt::column_sig::value_bytes::{self, StorageClass};
use crate::crdt::registry_row_sig::payload::RegistryRowSigPayload;
use crate::crdt::registry_row_sig::sign::sign_registry_row;
use crate::crdt::trigger::{ensure_crdt_columns, DELETED_ROWS_TABLE};
use crate::database::DbConnection;
use crate::table_names::{
    COL_SHARED_SPACE_SYNC_AUTHORED_BY_DID, COL_SHARED_SPACE_SYNC_CATEGORY,
    COL_SHARED_SPACE_SYNC_CATEGORY_LABEL, COL_SHARED_SPACE_SYNC_CREATED_AT,
    COL_SHARED_SPACE_SYNC_EXTENSION_NAME, COL_SHARED_SPACE_SYNC_EXTENSION_PUBLIC_KEY,
    COL_SHARED_SPACE_SYNC_ROW_PKS, COL_SHARED_SPACE_SYNC_ROW_SIG, COL_SHARED_SPACE_SYNC_SPACE_ID,
    COL_SHARED_SPACE_SYNC_TABLE_NAME, COL_SHARED_SPACE_SYNC_TYPE, COL_SHARED_SPACE_SYNC_TYPE_LABEL,
    TABLE_CRDT_CONFIGS, TABLE_SHARED_SPACE_SYNC,
};
use crate::ucan::verify::did_key_from_public_key;
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use ed25519_dalek::{SigningKey, VerifyingKey};
use rusqlite::types::Value as SqlValue;
use serde_json::Value as JsonValue;
use std::sync::{Arc, Mutex};

fn generate_keypair() -> (SigningKey, VerifyingKey) {
    let sk = SigningKey::from_bytes(&rand::random::<[u8; 32]>());
    let pk = sk.verifying_key();
    (sk, pk)
}

/// In-memory DB with just enough schema to drive the apply pipeline against
/// `haex_shared_space_sync`: the CRDT config table (triggers-enabled toggle)
/// plus the full migration-0014 register shape, upgraded with CRDT columns
/// so the apply loop's row-exists / column-HLC bookkeeping works without
/// hitting the auto-upgrade branch.
fn setup_registry_db() -> DbConnection {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    conn.execute_batch(&format!(
        "CREATE TABLE {TABLE_CRDT_CONFIGS} (key TEXT PRIMARY KEY, type TEXT, value TEXT);
         CREATE TABLE {DELETED_ROWS_TABLE} (
             id TEXT PRIMARY KEY,
             table_name TEXT NOT NULL,
             row_pks TEXT NOT NULL,
             haex_hlc TEXT,
             haex_column_hlcs TEXT NOT NULL DEFAULT '{{}}'
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
            created_at TEXT DEFAULT (CURRENT_TIMESTAMP)
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

/// One registry row's worth of business-column values, owned so both the
/// `RegistryRowSigPayload` (borrowing) and the wire batch (owned) can be
/// built from the same data without fighting lifetimes.
struct RegistryFields {
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
}

impl RegistryFields {
    fn sample(id: &str, space_id: &str, authored_by_did: &str) -> Self {
        RegistryFields {
            id: id.to_string(),
            space_id: space_id.to_string(),
            table_name: "ext_calendar_v1".to_string(),
            row_pks: r#"{"id":"evt-1"}"#.to_string(),
            extension_public_key: Some("epk".to_string()),
            extension_name: Some("calendar".to_string()),
            category: Some("work".to_string()),
            r#type: Some("event".to_string()),
            category_label: Some("Work".to_string()),
            type_label: Some("Event".to_string()),
            authored_by_did: authored_by_did.to_string(),
            created_at: "2026-07-31T00:00:00Z".to_string(),
        }
    }

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

    fn row_sig_b64(&self, sk: &SigningKey) -> String {
        BASE64.encode(sign_registry_row(&self.payload(), sk).to_bytes())
    }

    /// Every business column as `(column_name, wire_value)`, in no
    /// particular order — used to build the unsigned/signed
    /// `RemoteColumnChange` batch. `id` is deliberately absent: it is the
    /// row's own CRDT primary key and, like every PK elsewhere in this
    /// pipeline, is never carried as a column-level change.
    fn as_columns(&self) -> Vec<(&'static str, JsonValue)> {
        let opt = |v: &Option<String>| match v {
            Some(s) => JsonValue::String(s.clone()),
            None => JsonValue::Null,
        };
        vec![
            (
                COL_SHARED_SPACE_SYNC_SPACE_ID,
                JsonValue::String(self.space_id.clone()),
            ),
            (
                COL_SHARED_SPACE_SYNC_TABLE_NAME,
                JsonValue::String(self.table_name.clone()),
            ),
            (
                COL_SHARED_SPACE_SYNC_ROW_PKS,
                JsonValue::String(self.row_pks.clone()),
            ),
            (
                COL_SHARED_SPACE_SYNC_EXTENSION_PUBLIC_KEY,
                opt(&self.extension_public_key),
            ),
            (
                COL_SHARED_SPACE_SYNC_EXTENSION_NAME,
                opt(&self.extension_name),
            ),
            (COL_SHARED_SPACE_SYNC_CATEGORY, opt(&self.category)),
            (COL_SHARED_SPACE_SYNC_TYPE, opt(&self.r#type)),
            (
                COL_SHARED_SPACE_SYNC_CATEGORY_LABEL,
                opt(&self.category_label),
            ),
            (COL_SHARED_SPACE_SYNC_TYPE_LABEL, opt(&self.type_label)),
            (
                COL_SHARED_SPACE_SYNC_AUTHORED_BY_DID,
                JsonValue::String(self.authored_by_did.clone()),
            ),
            (
                COL_SHARED_SPACE_SYNC_CREATED_AT,
                JsonValue::String(self.created_at.clone()),
            ),
        ]
    }
}

/// Build one `RemoteColumnChange`. When `column_sk` is `Some`, also attaches
/// a valid per-column Ed25519 signature (the OTHER, pre-existing sig gate —
/// `verify_change_sig` in `db.rs`) so the batch also satisfies
/// `enforce_sigs == true` scoped applies.
fn build_change(
    row_pks_json: &str,
    column: &'static str,
    value: JsonValue,
    hlc: &str,
    column_sk: Option<&SigningKey>,
    space_id: &str,
) -> RemoteColumnChange {
    let sql_value = match &value {
        JsonValue::Null => SqlValue::Null,
        JsonValue::String(s) => SqlValue::Text(s.clone()),
        other => panic!("registry test columns are text-or-null, got {other:?}"),
    };
    let sig = column_sk.map(|sk| {
        let did = did_key_from_public_key(&sk.verifying_key());
        let value_bytes_vec = value_bytes::to_canonical_bytes(&sql_value);
        let signature = sign_column(
            sk,
            space_id.as_bytes(),
            TABLE_SHARED_SPACE_SYNC.as_bytes(),
            row_pks_json.as_bytes(),
            column.as_bytes(),
            hlc.as_bytes(),
            did.as_bytes(),
            &value_bytes_vec,
        );
        ColumnSig {
            author_did: did,
            sig: BASE64.encode(signature.to_bytes()),
            storage_class: StorageClass::of(&sql_value),
        }
    });
    RemoteColumnChange {
        table_name: TABLE_SHARED_SPACE_SYNC.to_string(),
        row_pks: row_pks_json.to_string(),
        column_name: column.to_string(),
        hlc_timestamp: hlc.to_string(),
        decrypted_value: value,
        sig,
    }
}

/// Build a full INSERT (or wholesale UPDATE) batch for `fields`: every
/// business column plus a fresh `row_sig` self-signed with `row_sk`, all at
/// the same `hlc` (same-transaction semantics). `column_sk` is `Some` only
/// for the scoped/`enforce_sigs == true` test, which also needs a valid
/// per-column signature on every field.
fn full_batch(
    fields: &RegistryFields,
    row_sk: &SigningKey,
    column_sk: Option<&SigningKey>,
    hlc: &str,
) -> Vec<RemoteColumnChange> {
    let row_pks_json = format!(r#"{{"id":"{}"}}"#, fields.id);
    let mut changes: Vec<RemoteColumnChange> = fields
        .as_columns()
        .into_iter()
        .map(|(col, value)| {
            build_change(&row_pks_json, col, value, hlc, column_sk, &fields.space_id)
        })
        .collect();
    changes.push(build_change(
        &row_pks_json,
        COL_SHARED_SPACE_SYNC_ROW_SIG,
        JsonValue::String(fields.row_sig_b64(row_sk)),
        hlc,
        column_sk,
        &fields.space_id,
    ));
    changes
}

fn query_registry_row(db: &DbConnection, id: &str) -> Option<(String, String, Option<String>)> {
    let guard = db.0.lock().unwrap();
    let conn = guard.as_ref().unwrap();
    conn.query_row(
        &format!(
            "SELECT {COL_SHARED_SPACE_SYNC_AUTHORED_BY_DID}, {COL_SHARED_SPACE_SYNC_TABLE_NAME}, \
                    {COL_SHARED_SPACE_SYNC_CATEGORY} \
             FROM {TABLE_SHARED_SPACE_SYNC} WHERE id = ?1"
        ),
        [id],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    )
    .ok()
}

/// A validly self-signed INSERT lands: the row's `row_sig` verifies against
/// its own `authored_by_did` and the row is applied normally.
#[test]
fn apply_pipeline_accepts_valid_registry_row_insert() {
    let db = setup_registry_db();
    let (sk, pk) = generate_keypair();
    let did = did_key_from_public_key(&pk);
    let fields = RegistryFields::sample("reg-1", "space-1", &did);
    let batch = full_batch(&fields, &sk, None, "1/aaa");

    apply_remote_changes_to_db(&db, batch, None, None).expect("apply must succeed");

    let stored = query_registry_row(&db, "reg-1").expect("row must exist");
    assert_eq!(stored.0, did, "authored_by_did must match");
    assert_eq!(stored.1, "ext_calendar_v1");
    assert_eq!(stored.2.as_deref(), Some("work"));
}

/// A forged `row_sig` must drop the row's ENTIRE change set atomically — not
/// just the `row_sig` column — leaving no trace of the row in the DB.
#[test]
fn apply_pipeline_rejects_registry_row_with_bad_row_sig() {
    let db = setup_registry_db();
    let (sk, pk) = generate_keypair();
    let did = did_key_from_public_key(&pk);
    let fields = RegistryFields::sample("reg-2", "space-1", &did);
    let mut batch = full_batch(&fields, &sk, None, "1/aaa");

    // Replace the correctly-signed row_sig with well-formed-but-wrong bytes
    // (still present in the batch, still valid base64 — just not a
    // signature over this payload).
    let row_sig_change = batch
        .iter_mut()
        .find(|c| c.column_name == COL_SHARED_SPACE_SYNC_ROW_SIG)
        .expect("batch must carry a row_sig change");
    row_sig_change.decrypted_value = JsonValue::String(BASE64.encode([0u8; 64]));

    apply_remote_changes_to_db(&db, batch, None, None)
        .expect("apply must succeed — rejection is row-scoped, not fatal");

    assert!(
        query_registry_row(&db, "reg-2").is_none(),
        "forged row_sig must drop the entire row, not just the row_sig column"
    );
}

/// An UPDATE that relabels `authored_by_did` (even with an internally
/// consistent, freshly self-signed `row_sig` for the NEW author) must be
/// rejected wholesale — authorship is immutable post-creation. The
/// previously-applied row must be untouched.
#[test]
fn apply_pipeline_rejects_registry_update_that_changes_authored_by_did() {
    let db = setup_registry_db();
    let (sk_alice, pk_alice) = generate_keypair();
    let did_alice = did_key_from_public_key(&pk_alice);
    let fields_v1 = RegistryFields::sample("reg-3", "space-1", &did_alice);
    apply_remote_changes_to_db(
        &db,
        full_batch(&fields_v1, &sk_alice, None, "1/aaa"),
        None,
        None,
    )
    .expect("seed insert must succeed");

    let (sk_bob, pk_bob) = generate_keypair();
    let did_bob = did_key_from_public_key(&pk_bob);
    let mut fields_v2 = RegistryFields::sample("reg-3", "space-1", &did_bob);
    fields_v2.category = Some("hijacked".to_string());
    let hijack_batch = full_batch(&fields_v2, &sk_bob, None, "2/bbb");

    apply_remote_changes_to_db(&db, hijack_batch, None, None)
        .expect("apply must succeed — rejection is row-scoped, not fatal");

    let stored = query_registry_row(&db, "reg-3").expect("original row must still exist");
    assert_eq!(stored.0, did_alice, "authored_by_did must remain immutable");
    assert_eq!(
        stored.2.as_deref(),
        Some("work"),
        "the hijack UPDATE must be dropped in full, not partially applied"
    );
}

/// The row-sig gate (Stage 5b) and the pre-existing per-column sig gate are
/// stacked, not exclusive. A row with a VALID row_sig can still have one of
/// its OWN columns dropped by the per-column gate — orthogonally, on its own
/// per-column signature.
#[test]
fn apply_pipeline_allows_column_sig_failure_to_be_orthogonal_to_row_sig() {
    let db = setup_registry_db();
    let (row_sk, row_pk) = generate_keypair();
    let did = did_key_from_public_key(&row_pk);
    let (col_sk, _col_pk) = generate_keypair();
    let fields = RegistryFields::sample("reg-4", "space-1", &did);
    let mut batch = full_batch(&fields, &row_sk, Some(&col_sk), "1/aaa");

    // Corrupt ONLY category's per-column signature. row_sig was built over
    // the correct (uncorrupted) `category` value, so the row-level gate
    // still verifies the row as a whole.
    let category_change = batch
        .iter_mut()
        .find(|c| c.column_name == COL_SHARED_SPACE_SYNC_CATEGORY)
        .expect("batch must carry a category change");
    if let Some(sig) = category_change.sig.as_mut() {
        sig.sig = BASE64.encode([0u8; 64]);
    }

    apply_remote_changes_to_db_scoped(&db, batch, None, None, Some("space-1"))
        .expect("apply must succeed — both gates are row/column-scoped, not fatal");

    let stored = query_registry_row(&db, "reg-4").expect("row-sig-verified row must be inserted");
    assert_eq!(stored.1, "ext_calendar_v1");
    assert_eq!(
        stored.2, None,
        "category's own bad column-sig must still be dropped, independently of the passing row-sig"
    );
}
