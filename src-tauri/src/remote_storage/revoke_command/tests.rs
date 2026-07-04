//! Unit tests for `revoke_storage_share_core` — the AppState-free inner form
//! of the Tauri command.
//!
//! Mirrors `share_command/tests.rs`: in-memory SQLite + tx-scoped HLC hooks +
//! `ensure_crdt_columns` on every table we write to. The IAM adapter is
//! mocked so tests never touch AWS/Wasabi.
//!
//! The mock + fixture code is duplicated from `share_command/tests.rs`
//! rather than extracted — Rust does not cleanly share `#[cfg(test)]` code
//! between sibling modules, and pulling the helpers into a non-test module
//! would ship dead-in-production code. ~150 lines of duplication is cheaper
//! than the abstraction.

#![cfg(test)]

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use rusqlite::Connection;

use super::{revoke_storage_share_core, RevokeStorageShareArgs};
use crate::crdt::hlc::HlcService;
use crate::crdt::trigger::ensure_crdt_columns;
use crate::database::connection_context::ConnectionContext;
use crate::database::core::{install_tx_hlc_hooks, register_current_hlc_udf};
use crate::database::DbConnection;
use crate::remote_storage::error::StorageError;
use crate::remote_storage::iam_adapter::{IamAdapter, IamAdapterError, ProviderFlavor};
use crate::remote_storage::iam_admin_creds::{self, IamAdminCred};
use crate::remote_storage::iam_policy::IamPolicy;
use crate::remote_storage::provider::ProviderKind;
use crate::remote_storage::share_command::IamAdapterFactory;
use crate::table_names::{TABLE_CRDT_CONFIGS, TABLE_CRDT_DIRTY_TABLES};

// ---------------------------------------------------------------------------
// Mock IAM adapter — revoke-focused variant
// ---------------------------------------------------------------------------

/// Records every `delete_scoped_user` invocation and returns a scripted
/// result. The revoke flow does NOT call `create_scoped_user` or
/// `probe_iam_capability`, but we still implement them (returning `NotFound`)
/// so the trait bound is satisfied. If a test unexpectedly hits either it
/// gets a distinct error variant rather than a panic.
struct MockIamAdapter {
    delete_result: Mutex<Result<(), IamAdapterError>>,
    /// (user_name, access_key_id) tuples of every `delete_scoped_user` call.
    delete_calls: Arc<Mutex<Vec<(String, String)>>>,
}

impl MockIamAdapter {
    fn new(delete_result: Result<(), IamAdapterError>) -> Self {
        Self {
            delete_result: Mutex::new(delete_result),
            delete_calls: Arc::new(Mutex::new(Vec::new())),
        }
    }
}

#[async_trait]
impl IamAdapter for MockIamAdapter {
    async fn create_scoped_user(
        &self,
        _user_name: &str,
        _policy: &IamPolicy,
    ) -> Result<crate::remote_storage::iam_adapter::ScopedCred, IamAdapterError> {
        // Revoke flow never calls this — a hit here is a bug.
        Err(IamAdapterError::Other(
            "MockIamAdapter::create_scoped_user should not be called during revoke".to_string(),
        ))
    }

    async fn delete_scoped_user(
        &self,
        user_name: &str,
        access_key_id: &str,
    ) -> Result<(), IamAdapterError> {
        self.delete_calls
            .lock()
            .unwrap()
            .push((user_name.to_string(), access_key_id.to_string()));
        // Replace the slot with `NotFound` so a caller that erroneously
        // re-invokes the mock sees a distinct signal rather than the same
        // scripted result twice.
        std::mem::replace(
            &mut *self.delete_result.lock().unwrap(),
            Err(IamAdapterError::NotFound),
        )
    }

    async fn probe_iam_capability(&self) -> Result<bool, IamAdapterError> {
        // Revoke flow never probes.
        Err(IamAdapterError::Other(
            "MockIamAdapter::probe_iam_capability should not be called during revoke".to_string(),
        ))
    }
}

/// Factory that hands out a single pre-built mock adapter (wrapped in `Arc`).
struct MockIamAdapterFactory {
    adapter: Arc<MockIamAdapter>,
}

impl IamAdapterFactory for MockIamAdapterFactory {
    fn build(
        &self,
        _cred: &IamAdminCred,
        _flavor: ProviderFlavor,
    ) -> Result<Arc<dyn IamAdapter>, IamAdapterError> {
        Ok(self.adapter.clone() as Arc<dyn IamAdapter>)
    }
}

// ---------------------------------------------------------------------------
// Test-DB fixture
// ---------------------------------------------------------------------------

fn rand_string(prefix: &str) -> String {
    let n: u64 = rand::random();
    format!("{prefix}-{n:016x}")
}

/// Build the in-memory DB with all tables the revoke command touches, plus
/// the CRDT bookkeeping. Returns a fresh (empty of share rows) fixture;
/// tests seed their own share data via `seed_share`.
///
/// Returns `(db, hlc, parent_storage_id)` — a parent-owner backend is seeded
/// so tests can drop a `shared_from_space` child pointing at it.
fn setup_revoke_db() -> (DbConnection, HlcService, String) {
    let conn = Connection::open_in_memory().expect("open in-memory DB");
    let hlc_service = HlcService::new_for_testing("test-device-revoke");
    let ctx = ConnectionContext::new();
    register_current_hlc_udf(&conn, hlc_service.clone(), ctx.clone())
        .expect("register current_hlc UDF");
    install_tx_hlc_hooks(&conn, ctx).expect("install tx-HLC hooks");

    conn.execute_batch(&format!(
        "CREATE TABLE {} (key TEXT PRIMARY KEY, type TEXT NOT NULL, value TEXT NOT NULL);
         CREATE TABLE {} (table_name TEXT PRIMARY KEY, last_modified TEXT);",
        TABLE_CRDT_CONFIGS, TABLE_CRDT_DIRTY_TABLES,
    ))
    .expect("create crdt bookkeeping");

    conn.execute_batch(
        "CREATE TABLE haex_s3_backends (
            id TEXT PRIMARY KEY NOT NULL,
            type TEXT NOT NULL,
            name TEXT NOT NULL,
            config TEXT NOT NULL,
            enabled INTEGER NOT NULL DEFAULT 1,
            parent_backend_id TEXT,
            origin_type TEXT NOT NULL DEFAULT 'owned',
            share_prefix TEXT,
            share_access_flags INTEGER,
            created_at TEXT DEFAULT (CURRENT_TIMESTAMP)
        );

        CREATE TABLE haex_shared_space_sync (
            id TEXT PRIMARY KEY NOT NULL,
            table_name TEXT NOT NULL,
            row_pks TEXT NOT NULL,
            space_id TEXT NOT NULL,
            extension_public_key TEXT,
            extension_name TEXT,
            group_id TEXT,
            type TEXT,
            label TEXT,
            created_at TEXT DEFAULT (CURRENT_TIMESTAMP)
        );

        CREATE TABLE haex_passwords_item_details (
            id TEXT PRIMARY KEY NOT NULL,
            title TEXT,
            username TEXT,
            password TEXT,
            note TEXT,
            icon TEXT,
            color TEXT,
            url TEXT,
            otp_secret TEXT,
            otp_digits INTEGER DEFAULT 6,
            otp_period INTEGER DEFAULT 30,
            otp_algorithm TEXT DEFAULT 'SHA1',
            expires_at TEXT,
            autofill_aliases TEXT,
            created_at TEXT DEFAULT (CURRENT_TIMESTAMP),
            updated_at TEXT DEFAULT (CURRENT_TIMESTAMP)
        );

        CREATE TABLE haex_passwords_item_key_values (
            id TEXT PRIMARY KEY NOT NULL,
            item_id TEXT NOT NULL,
            key TEXT,
            value TEXT,
            updated_at TEXT DEFAULT (CURRENT_TIMESTAMP),
            FOREIGN KEY (item_id) REFERENCES haex_passwords_item_details(id)
                ON UPDATE NO ACTION ON DELETE CASCADE
        );",
    )
    .expect("create app tables");

    {
        let tx = conn.unchecked_transaction().expect("crdt-cols tx");
        for table in [
            "haex_s3_backends",
            "haex_shared_space_sync",
            "haex_passwords_item_details",
            "haex_passwords_item_key_values",
        ] {
            ensure_crdt_columns(&tx, table).expect("ensure crdt cols");
        }
        tx.commit().expect("commit crdt-cols tx");
    }

    // Seed the parent (owned) backend that shared rows point at.
    let parent_id = rand_string("storage");
    let parent_config = serde_json::json!({
        "endpoint": "https://s3.example.com",
        "bucket": "my-bucket",
        "region": "us-east-1",
        "pathStyle": true,
        "accessKeyId": "owner-akid",
        "secretAccessKey": "owner-secret",
    })
    .to_string();

    conn.execute(
        "INSERT INTO haex_s3_backends (id, type, name, config, enabled, origin_type)
         VALUES (?1, 's3', 'Owner Bucket', ?2, 1, 'owned')",
        rusqlite::params![&parent_id, &parent_config],
    )
    .expect("seed owner backend");

    let db = DbConnection(Arc::new(Mutex::new(Some(conn))));
    (db, hlc_service, parent_id)
}

/// Seeded shared-row artefacts — the test asserts against these to verify
/// the revoke deleted the correct rows / called the adapter with the right
/// arguments.
struct SeededShare {
    shared_id: String,
    iam_user_name: String,
    access_key_id: String,
    space_id: String,
}

/// Insert one `shared_from_space` row + its mapping. Also stores an
/// `IamAdminCred` for the parent (so revoke's cred load succeeds) unless
/// `skip_admin_cred` is true.
fn seed_share(
    db: &DbConnection,
    hlc: &HlcService,
    parent_id: &str,
    skip_admin_cred: bool,
) -> SeededShare {
    let shared_id = rand_string("shared");
    let space_id = rand_string("space");
    let iam_user_name = rand_string("haex-share");
    let access_key_id = rand_string("ASIA");
    let mapping_id = rand_string("map");

    let shared_config = serde_json::json!({
        "endpoint": "https://s3.example.com",
        "bucket": "my-bucket",
        "region": "us-east-1",
        "pathStyle": true,
        "accessKeyId": access_key_id.clone(),
        "secretAccessKey": "scoped-secret",
        "iamUserName": iam_user_name.clone(),
    })
    .to_string();

    let row_pks = serde_json::to_string(&vec![shared_id.clone()]).expect("row_pks json");

    {
        let guard = db.0.lock().unwrap();
        let conn = guard.as_ref().unwrap();
        conn.execute(
            "INSERT INTO haex_s3_backends
             (id, type, name, config, enabled, parent_backend_id, origin_type,
              share_prefix, share_access_flags)
             VALUES (?1, 's3', 'Shared', ?2, 1, ?3, 'shared_from_space', NULL, 3)",
            rusqlite::params![&shared_id, &shared_config, parent_id],
        )
        .expect("seed shared backend");

        conn.execute(
            "INSERT INTO haex_shared_space_sync
             (id, table_name, row_pks, space_id, extension_public_key,
              extension_name, group_id, type, label)
             VALUES (?1, ?2, ?3, ?4, NULL, NULL, NULL, 'cloud_storage', 'Shared')",
            rusqlite::params![&mapping_id, "haex_s3_backends", &row_pks, &space_id],
        )
        .expect("seed shared mapping");
    }

    if !skip_admin_cred {
        let hlc_local = std::sync::Mutex::new(hlc.clone());
        let guard = hlc_local.lock().expect("lock local hlc for seed");
        let cred = IamAdminCred {
            access_key_id: rand_string("AKIA"),
            secret_access_key: rand_string("secret"),
            provider_type: ProviderKind::Aws,
        };
        iam_admin_creds::store(db, &guard, parent_id, &cred).expect("seed admin cred");
    }

    SeededShare {
        shared_id,
        iam_user_name,
        access_key_id,
        space_id,
    }
}

fn shared_backend_exists(db: &DbConnection, id: &str) -> bool {
    let guard = db.0.lock().unwrap();
    let conn = guard.as_ref().unwrap();
    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM haex_s3_backends WHERE id = ?1",
            [id],
            |row| row.get(0),
        )
        .expect("count shared rows");
    count > 0
}

fn mapping_exists_for_shared(db: &DbConnection, shared_id: &str) -> bool {
    let guard = db.0.lock().unwrap();
    let conn = guard.as_ref().unwrap();
    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM haex_shared_space_sync \
             WHERE table_name = 'haex_s3_backends' \
               AND json_extract(row_pks, '$[0]') = ?1",
            [shared_id],
            |row| row.get(0),
        )
        .expect("count mapping rows");
    count > 0
}

// ---------------------------------------------------------------------------
// Happy path
// ---------------------------------------------------------------------------

#[tokio::test]
async fn happy_path_deletes_iam_user_and_both_db_rows() {
    let (db, hlc, parent_id) = setup_revoke_db();
    let seeded = seed_share(&db, &hlc, &parent_id, false);
    let adapter = Arc::new(MockIamAdapter::new(Ok(())));
    let factory = MockIamAdapterFactory {
        adapter: adapter.clone(),
    };

    revoke_storage_share_core(
        &db,
        &hlc,
        RevokeStorageShareArgs {
            shared_backend_id: seeded.shared_id.clone(),
        },
        &factory,
    )
    .await
    .expect("revoke should succeed");

    // Adapter received exactly one delete call with matching args.
    let deletes = adapter.delete_calls.lock().unwrap();
    assert_eq!(deletes.len(), 1, "adapter.delete_scoped_user called once");
    assert_eq!(
        deletes[0].0, seeded.iam_user_name,
        "must delete the right IAM user"
    );
    assert_eq!(
        deletes[0].1, seeded.access_key_id,
        "must delete the right access key"
    );
    drop(deletes);

    // Both DB rows gone.
    assert!(
        !shared_backend_exists(&db, &seeded.shared_id),
        "shared s3_backends row must be deleted"
    );
    assert!(
        !mapping_exists_for_shared(&db, &seeded.shared_id),
        "haex_shared_space_sync mapping must be deleted"
    );

    // The parent (owned) row is untouched.
    assert!(
        shared_backend_exists(&db, &parent_id),
        "parent (owned) row must not be touched"
    );

    // The space_id is unused by any other assertion — silence the unused warning.
    let _ = seeded.space_id;
}

// ---------------------------------------------------------------------------
// Not-found + wrong-origin
// ---------------------------------------------------------------------------

#[tokio::test]
async fn storage_not_found_when_shared_id_unknown() {
    let (db, hlc, _parent_id) = setup_revoke_db();
    let adapter = Arc::new(MockIamAdapter::new(Ok(())));
    let factory = MockIamAdapterFactory {
        adapter: adapter.clone(),
    };
    let unknown_id = rand_string("no-such-share");

    let err = revoke_storage_share_core(
        &db,
        &hlc,
        RevokeStorageShareArgs {
            shared_backend_id: unknown_id.clone(),
        },
        &factory,
    )
    .await
    .expect_err("unknown id must fail");

    match err {
        StorageError::StorageNotFound { storage_id } => assert_eq!(storage_id, unknown_id),
        other => panic!("expected StorageNotFound, got {other:?}"),
    }
    assert!(
        adapter.delete_calls.lock().unwrap().is_empty(),
        "no adapter call on not-found"
    );
}

#[tokio::test]
async fn not_a_share_row_when_passed_owned_backend_id() {
    let (db, hlc, parent_id) = setup_revoke_db();
    let adapter = Arc::new(MockIamAdapter::new(Ok(())));
    let factory = MockIamAdapterFactory {
        adapter: adapter.clone(),
    };

    let err = revoke_storage_share_core(
        &db,
        &hlc,
        RevokeStorageShareArgs {
            shared_backend_id: parent_id.clone(),
        },
        &factory,
    )
    .await
    .expect_err("owned backend id must be rejected");

    match err {
        StorageError::NotAShareRow { origin_type } => {
            assert_eq!(origin_type, "owned", "must carry actual origin_type");
        }
        other => panic!("expected NotAShareRow, got {other:?}"),
    }
    assert!(
        adapter.delete_calls.lock().unwrap().is_empty(),
        "no adapter call on wrong origin"
    );
    // The parent row itself must not have been deleted.
    assert!(
        shared_backend_exists(&db, &parent_id),
        "owned row must not be deleted by wrong-origin rejection"
    );
}

// ---------------------------------------------------------------------------
// IAM admin cred missing
// ---------------------------------------------------------------------------

#[tokio::test]
async fn iam_admin_cred_missing_when_no_cred_stored() {
    let (db, hlc, parent_id) = setup_revoke_db();
    let seeded = seed_share(&db, &hlc, &parent_id, true); // skip_admin_cred
    let adapter = Arc::new(MockIamAdapter::new(Ok(())));
    let factory = MockIamAdapterFactory {
        adapter: adapter.clone(),
    };

    let err = revoke_storage_share_core(
        &db,
        &hlc,
        RevokeStorageShareArgs {
            shared_backend_id: seeded.shared_id.clone(),
        },
        &factory,
    )
    .await
    .expect_err("missing cred must halt");

    match err {
        StorageError::IamAdminCredMissing { storage_id } => {
            assert_eq!(
                storage_id, parent_id,
                "must reference parent, not shared id"
            );
        }
        other => panic!("expected IamAdminCredMissing, got {other:?}"),
    }

    assert!(
        adapter.delete_calls.lock().unwrap().is_empty(),
        "no adapter call when cred is missing"
    );
    // DB rows remain — the user needs a chance to re-input the cred and retry.
    assert!(
        shared_backend_exists(&db, &seeded.shared_id),
        "shared row must remain when revoke halts pre-IAM"
    );
    assert!(
        mapping_exists_for_shared(&db, &seeded.shared_id),
        "mapping must remain when revoke halts pre-IAM"
    );
}

// ---------------------------------------------------------------------------
// IAM NotFound is idempotent
// ---------------------------------------------------------------------------

#[tokio::test]
async fn iam_not_found_is_idempotent_db_still_cleaned() {
    // The IAM user may already be gone (user did manual cleanup at provider,
    // or a prior partial revoke fired). Revoke should complete + clean DB.
    let (db, hlc, parent_id) = setup_revoke_db();
    let seeded = seed_share(&db, &hlc, &parent_id, false);
    let adapter = Arc::new(MockIamAdapter::new(Err(IamAdapterError::NotFound)));
    let factory = MockIamAdapterFactory {
        adapter: adapter.clone(),
    };

    revoke_storage_share_core(
        &db,
        &hlc,
        RevokeStorageShareArgs {
            shared_backend_id: seeded.shared_id.clone(),
        },
        &factory,
    )
    .await
    .expect("NotFound at provider must not fail the revoke");

    // Adapter was still called (idempotency is delegated to the adapter).
    assert_eq!(
        adapter.delete_calls.lock().unwrap().len(),
        1,
        "adapter must have been invoked exactly once"
    );

    // Both DB rows deleted.
    assert!(!shared_backend_exists(&db, &seeded.shared_id));
    assert!(!mapping_exists_for_shared(&db, &seeded.shared_id));
}

// ---------------------------------------------------------------------------
// IAM error path leaves DB alone
// ---------------------------------------------------------------------------

#[tokio::test]
async fn iam_other_error_prevents_db_deletion() {
    // If the IAM delete fails with anything other than NotFound, we must
    // NOT touch the DB — a subsequent retry needs to find the same rows to
    // re-drive the flow.
    let (db, hlc, parent_id) = setup_revoke_db();
    let seeded = seed_share(&db, &hlc, &parent_id, false);
    let adapter = Arc::new(MockIamAdapter::new(Err(IamAdapterError::Network(
        "transient boom".to_string(),
    ))));
    let factory = MockIamAdapterFactory {
        adapter: adapter.clone(),
    };

    let err = revoke_storage_share_core(
        &db,
        &hlc,
        RevokeStorageShareArgs {
            shared_backend_id: seeded.shared_id.clone(),
        },
        &factory,
    )
    .await
    .expect_err("IAM Network error must propagate");

    match err {
        StorageError::IamProvisioningFailed { reason } => {
            assert!(
                reason.contains("transient boom"),
                "reason must carry cause, got {reason}"
            );
            assert!(
                reason.contains("revoke"),
                "reason should be tagged as revoke path, got {reason}"
            );
        }
        other => panic!("expected IamProvisioningFailed, got {other:?}"),
    }

    // DB rows must still be present — safety property.
    assert!(
        shared_backend_exists(&db, &seeded.shared_id),
        "shared row must remain when IAM revoke fails"
    );
    assert!(
        mapping_exists_for_shared(&db, &seeded.shared_id),
        "mapping must remain when IAM revoke fails"
    );
}

// ---------------------------------------------------------------------------
// DB delete failure after IAM success
// ---------------------------------------------------------------------------

#[tokio::test]
async fn db_delete_failure_after_iam_success_surfaces_error() {
    // Drop the mapping table to force the first DELETE (mapping) to fail
    // AFTER the IAM adapter has already succeeded. The command must surface
    // a DatabaseError; the s3_backends row remains untouched.
    let (db, hlc, parent_id) = setup_revoke_db();
    let seeded = seed_share(&db, &hlc, &parent_id, false);

    {
        let guard = db.0.lock().unwrap();
        let conn = guard.as_ref().unwrap();
        conn.execute_batch("DROP TABLE haex_shared_space_sync;")
            .expect("drop mapping table");
    }

    let adapter = Arc::new(MockIamAdapter::new(Ok(())));
    let factory = MockIamAdapterFactory {
        adapter: adapter.clone(),
    };

    let err = revoke_storage_share_core(
        &db,
        &hlc,
        RevokeStorageShareArgs {
            shared_backend_id: seeded.shared_id.clone(),
        },
        &factory,
    )
    .await
    .expect_err("mapping DELETE against missing table must fail");

    assert!(
        matches!(err, StorageError::DatabaseError { .. }),
        "expected DatabaseError, got {err:?}"
    );

    // IAM adapter WAS called.
    assert_eq!(adapter.delete_calls.lock().unwrap().len(), 1);

    // s3_backends row is still around — mapping DELETE failed before we
    // reached the s3_backends DELETE, so the shared row is intact.
    assert!(
        shared_backend_exists(&db, &seeded.shared_id),
        "s3_backends row must remain when mapping delete fails first"
    );
}

// ---------------------------------------------------------------------------
// Error serialization pin
// ---------------------------------------------------------------------------

#[test]
fn not_a_share_row_serializes_with_camel_case_tag() {
    // Frontend pattern-matches on `type` + reads camelCase `details` — pin
    // the shape so a rename accident becomes a red test rather than a silent
    // frontend breakage.
    let err = StorageError::NotAShareRow {
        origin_type: "owned".to_string(),
    };
    let json = serde_json::to_value(&err).expect("serialize");
    assert_eq!(json["type"], "NotAShareRow");
    assert_eq!(json["details"]["origin_type"], "owned");
}

#[test]
fn parent_backend_missing_serializes_with_camel_case_tag() {
    let err = StorageError::ParentBackendMissing {
        parent_backend_id: "parent-xyz".to_string(),
    };
    let json = serde_json::to_value(&err).expect("serialize");
    assert_eq!(json["type"], "ParentBackendMissing");
    assert_eq!(json["details"]["parent_backend_id"], "parent-xyz");
}
