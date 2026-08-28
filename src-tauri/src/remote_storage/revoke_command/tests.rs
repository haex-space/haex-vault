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
use crate::crdt::column_sig::key_cache::SpaceKeyCache;
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
///
/// Round F3b: `delete_scoped_user` now takes only `user_name` — the real
/// adapter discovers access-key ids via `ListAccessKeys` before deleting
/// them. The mock records only `user_name`.
struct MockIamAdapter {
    delete_result: Mutex<Result<(), IamAdapterError>>,
    /// Every `user_name` passed to `delete_scoped_user`.
    delete_calls: Arc<Mutex<Vec<String>>>,
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

    async fn delete_scoped_user(&self, user_name: &str) -> Result<(), IamAdapterError> {
        self.delete_calls
            .lock()
            .unwrap()
            .push(user_name.to_string());
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
            category TEXT,
            type TEXT,
            type_label TEXT,
            created_at TEXT DEFAULT (CURRENT_TIMESTAMP)
        );

        CREATE TABLE haex_s3_shared_access (
            id TEXT PRIMARY KEY NOT NULL,
            space_id TEXT NOT NULL,
            backend_id TEXT NOT NULL,
            member_did TEXT NOT NULL,
            encrypted_cred TEXT NOT NULL,
            epoch INTEGER NOT NULL,
            expires_at TEXT,
            created_at TEXT DEFAULT (CURRENT_TIMESTAMP) NOT NULL
        );
        CREATE UNIQUE INDEX haex_s3_shared_access_space_backend_did_uniq
            ON haex_s3_shared_access (space_id, backend_id, member_did);

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
            "haex_s3_shared_access",
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
    #[allow(dead_code)]
    access_key_id: String,
    space_id: String,
}

/// Insert one `shared_from_space` row + its mapping. Also stores an
/// `IamAdminCred` for the parent (so revoke's cred load succeeds) unless
/// `skip_admin_cred` is true.
///
/// `omit_access_key_id_in_config` mirrors the Round F3b post-cutover shape:
/// the child backend config no longer carries cred material. Existing tests
/// (that pre-date the cutover) still pass `false` — the presence of the
/// field in the config JSON is irrelevant to the new revoke flow, which
/// discovers keys via the IAM adapter.
fn seed_share(
    db: &DbConnection,
    hlc: &HlcService,
    parent_id: &str,
    skip_admin_cred: bool,
    omit_access_key_id_in_config: bool,
) -> SeededShare {
    let shared_id = rand_string("shared");
    let space_id = rand_string("space");
    let iam_user_name = rand_string("haex-share");
    let access_key_id = rand_string("ASIA");
    let mapping_id = rand_string("map");

    let shared_config = if omit_access_key_id_in_config {
        // Post-F3b shape: structural fields + iamUserName only, no cred.
        serde_json::json!({
            "endpoint": "https://s3.example.com",
            "bucket": "my-bucket",
            "region": "us-east-1",
            "pathStyle": true,
            "iamUserName": iam_user_name.clone(),
        })
        .to_string()
    } else {
        serde_json::json!({
            "endpoint": "https://s3.example.com",
            "bucket": "my-bucket",
            "region": "us-east-1",
            "pathStyle": true,
            "accessKeyId": access_key_id.clone(),
            "secretAccessKey": "scoped-secret",
            "iamUserName": iam_user_name.clone(),
        })
        .to_string()
    };

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
              extension_name, category, type, type_label)
             VALUES (?1, ?2, ?3, ?4, NULL, NULL, NULL, 'cloud_storage', 'Shared')",
            rusqlite::params![&mapping_id, "haex_s3_backends", &row_pks, &space_id],
        )
        .expect("seed shared mapping");

        // Simulate Round F3b's per-member ScopedCred fanout: two space
        // members → two `haex_s3_shared_access` rows keyed by
        // `(space_id, backend_id, member_did)`. The revoke DELETE must
        // drop both — anything less leaks sealed ScopedCred rows that
        // keep CRDT-syncing to every current and future member.
        for member_did in ["did:key:member1", "did:key:member2"] {
            let sa_id = rand_string("sa");
            conn.execute(
                "INSERT INTO haex_s3_shared_access
                 (id, space_id, backend_id, member_did, encrypted_cred, epoch)
                 VALUES (?1, ?2, ?3, ?4, ?5, 1)",
                rusqlite::params![
                    &sa_id,
                    &space_id,
                    &shared_id,
                    member_did,
                    "sealed-cred-placeholder",
                ],
            )
            .expect("seed shared_access fanout row");
        }
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

fn shared_access_count(db: &DbConnection, space_id: &str, backend_id: &str) -> i64 {
    let guard = db.0.lock().unwrap();
    let conn = guard.as_ref().unwrap();
    conn.query_row(
        "SELECT COUNT(*) FROM haex_s3_shared_access \
         WHERE space_id = ?1 AND backend_id = ?2",
        rusqlite::params![space_id, backend_id],
        |row| row.get(0),
    )
    .expect("count shared_access rows")
}

// ---------------------------------------------------------------------------
// Happy path
// ---------------------------------------------------------------------------

#[tokio::test]
async fn happy_path_deletes_iam_user_and_both_db_rows() {
    let (db, hlc, parent_id) = setup_revoke_db();
    let seeded = seed_share(&db, &hlc, &parent_id, false, false);
    let adapter = Arc::new(MockIamAdapter::new(Ok(())));
    let factory = MockIamAdapterFactory {
        adapter: adapter.clone(),
    };

    // Pre-assert: the per-member fanout was actually seeded, so the
    // post-revoke count == 0 assertion below is meaningful (rather than
    // vacuous). Two members → two rows.
    assert_eq!(
        shared_access_count(&db, &seeded.space_id, &seeded.shared_id),
        2,
        "seed must have written the per-member shared_access fanout"
    );

    revoke_storage_share_core(
        &db,
        &hlc,
        &SpaceKeyCache::new(),
        RevokeStorageShareArgs {
            shared_backend_id: seeded.shared_id.clone(),
        },
        &factory,
    )
    .await
    .expect("revoke should succeed");

    // Adapter received exactly one delete call with the correct IAM user.
    // Round F3b: the trait's `delete_scoped_user` takes only `user_name` —
    // the aws_compat impl enumerates access-key ids internally at revoke
    // time. The stored `seeded.access_key_id` is no longer part of the
    // vault→adapter interface; the fixture keeps it for historical shape
    // but the assertion moves to `iam_user_name`.
    let deletes = adapter.delete_calls.lock().unwrap();
    assert_eq!(deletes.len(), 1, "adapter.delete_scoped_user called once");
    assert_eq!(
        deletes[0], seeded.iam_user_name,
        "must delete the right IAM user"
    );
    drop(deletes);

    // All three row sets gone: the sealed shared_access fanout, the
    // sync mapping, and the child s3_backends row itself.
    assert_eq!(
        shared_access_count(&db, &seeded.space_id, &seeded.shared_id),
        0,
        "haex_s3_shared_access rows must be deleted on revoke — otherwise \
         sealed ScopedCred rows keep CRDT-syncing to every space member"
    );
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
        &SpaceKeyCache::new(),
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
        &SpaceKeyCache::new(),
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
    let seeded = seed_share(&db, &hlc, &parent_id, true, false); // skip_admin_cred
    let adapter = Arc::new(MockIamAdapter::new(Ok(())));
    let factory = MockIamAdapterFactory {
        adapter: adapter.clone(),
    };

    let err = revoke_storage_share_core(
        &db,
        &hlc,
        &SpaceKeyCache::new(),
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
    let seeded = seed_share(&db, &hlc, &parent_id, false, false);
    let adapter = Arc::new(MockIamAdapter::new(Err(IamAdapterError::NotFound)));
    let factory = MockIamAdapterFactory {
        adapter: adapter.clone(),
    };

    revoke_storage_share_core(
        &db,
        &hlc,
        &SpaceKeyCache::new(),
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
// F3b regression: revoke succeeds when the child config lacks accessKeyId
// ---------------------------------------------------------------------------

#[tokio::test]
async fn revoke_succeeds_when_config_missing_access_key_id() {
    // C1: Round F3b's `share_storage_backend_core` strips cred fields
    // (accessKeyId / secretAccessKey / sessionToken) out of the child
    // backend row's config JSON. `revoke_storage_share_core` must NOT
    // read those fields — it only needs `iamUserName` from the config,
    // and the trait's `delete_scoped_user` enumerates access keys at
    // the provider on its own.
    //
    // Regression cover: a share row written under F3b (no accessKeyId in
    // the config) must revoke cleanly.
    let (db, hlc, parent_id) = setup_revoke_db();
    let seeded = seed_share(&db, &hlc, &parent_id, false, true);
    let adapter = Arc::new(MockIamAdapter::new(Ok(())));
    let factory = MockIamAdapterFactory {
        adapter: adapter.clone(),
    };

    revoke_storage_share_core(
        &db,
        &hlc,
        &SpaceKeyCache::new(),
        RevokeStorageShareArgs {
            shared_backend_id: seeded.shared_id.clone(),
        },
        &factory,
    )
    .await
    .expect("revoke must succeed even when accessKeyId is absent from config");

    // Adapter called once, with the seeded iam_user_name.
    let deletes = adapter.delete_calls.lock().unwrap();
    assert_eq!(
        deletes.len(),
        1,
        "adapter.delete_scoped_user must be called once even without config accessKeyId"
    );
    assert_eq!(deletes[0], seeded.iam_user_name);
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
    let seeded = seed_share(&db, &hlc, &parent_id, false, false);
    let adapter = Arc::new(MockIamAdapter::new(Err(IamAdapterError::Network(
        "transient boom".to_string(),
    ))));
    let factory = MockIamAdapterFactory {
        adapter: adapter.clone(),
    };

    let err = revoke_storage_share_core(
        &db,
        &hlc,
        &SpaceKeyCache::new(),
        RevokeStorageShareArgs {
            shared_backend_id: seeded.shared_id.clone(),
        },
        &factory,
    )
    .await
    .expect_err("IAM Network error must propagate");

    match err {
        StorageError::IamOperationFailed { operation, reason } => {
            assert_eq!(
                operation, "delete_scoped_user",
                "operation must identify the revoke's IAM step"
            );
            assert!(
                reason.contains("transient boom"),
                "reason must carry cause, got {reason}"
            );
        }
        other => panic!("expected IamOperationFailed, got {other:?}"),
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
    // Drop the mapping table to force the DB-cleanup phase to fail AFTER
    // the IAM adapter has already succeeded. The command must surface a
    // DatabaseError; the s3_backends row remains untouched. The concrete
    // failing statement is now the `resolve_bound_space_id` SELECT (which
    // reads the mapping to key the `haex_s3_shared_access` DELETE), not
    // the mapping DELETE itself — either way the halt happens before the
    // s3_backends DELETE, which is what this test cares about.
    let (db, hlc, parent_id) = setup_revoke_db();
    let seeded = seed_share(&db, &hlc, &parent_id, false, false);

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
        &SpaceKeyCache::new(),
        RevokeStorageShareArgs {
            shared_backend_id: seeded.shared_id.clone(),
        },
        &factory,
    )
    .await
    .expect_err("DB-cleanup phase against missing mapping table must fail");

    assert!(
        matches!(err, StorageError::DatabaseError { .. }),
        "expected DatabaseError, got {err:?}"
    );

    // IAM adapter WAS called.
    assert_eq!(adapter.delete_calls.lock().unwrap().len(), 1);

    // s3_backends row is still around — the DB-cleanup phase failed at
    // its first statement (either the shared_access space-id resolve or
    // the mapping DELETE), so we never reached the s3_backends DELETE.
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

#[test]
fn iam_operation_failed_serializes_with_operation_and_reason() {
    // Pin the new variant's shape: `{"type":"IamOperationFailed","details":{"operation":"...","reason":"..."}}`.
    // Frontend pattern-matches on `type` and reads both fields, so a rename
    // would silently break UI. Belt-and-braces alongside the runtime asserts
    // in create_scoped_user_failure_writes_no_db_rows /
    // iam_other_error_prevents_db_deletion.
    let err = StorageError::IamOperationFailed {
        operation: "delete_scoped_user".to_string(),
        reason: "http 500".to_string(),
    };
    let json = serde_json::to_value(&err).expect("serialize");
    assert_eq!(json["type"], "IamOperationFailed");
    assert_eq!(json["details"]["operation"], "delete_scoped_user");
    assert_eq!(json["details"]["reason"], "http 500");
}

// ---------------------------------------------------------------------------
// assert_parent_exists: existence-only check (I2)
// ---------------------------------------------------------------------------
//
// After the I2 fix, the parent-existence guard no longer filters on
// `origin_type = 'owned'`. Two properties matter:
//   (a) revoke succeeds when the parent exists, regardless of its origin_type
//   (b) revoke returns ParentBackendMissing when the parent row is truly gone

#[tokio::test]
async fn parent_with_non_owned_origin_still_permits_revoke() {
    // Simulate a corrupted parent (origin_type flipped away from 'owned' —
    // e.g. because a prior migration/repair changed it). The existence-only
    // check must let the revoke proceed rather than raise a misleading
    // ParentBackendMissing.
    let (db, hlc, parent_id) = setup_revoke_db();
    let seeded = seed_share(&db, &hlc, &parent_id, false, false);

    // Corrupt the parent's origin_type.
    {
        let guard = db.0.lock().unwrap();
        let conn = guard.as_ref().unwrap();
        conn.execute(
            "UPDATE haex_s3_backends SET origin_type = 'unknown' WHERE id = ?1",
            [&parent_id],
        )
        .expect("flip parent origin_type");
    }

    let adapter = Arc::new(MockIamAdapter::new(Ok(())));
    let factory = MockIamAdapterFactory {
        adapter: adapter.clone(),
    };

    revoke_storage_share_core(
        &db,
        &hlc,
        &SpaceKeyCache::new(),
        RevokeStorageShareArgs {
            shared_backend_id: seeded.shared_id.clone(),
        },
        &factory,
    )
    .await
    .expect("revoke must proceed when parent row exists, regardless of origin_type");

    // Shared row and mapping should be gone; IAM adapter was called exactly once.
    assert!(!shared_backend_exists(&db, &seeded.shared_id));
    assert!(!mapping_exists_for_shared(&db, &seeded.shared_id));
    assert_eq!(adapter.delete_calls.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn parent_absent_still_returns_parent_backend_missing() {
    // Regression guard: dropping the origin_type filter must NOT weaken the
    // existence check. If the parent row is truly absent, the flow still has
    // to bail with ParentBackendMissing before touching IAM.
    let (db, hlc, parent_id) = setup_revoke_db();
    let seeded = seed_share(&db, &hlc, &parent_id, false, false);

    // Delete the parent row directly (bypass CRDT path — we want the row
    // gone from the base table for the existence-check target).
    {
        let guard = db.0.lock().unwrap();
        let conn = guard.as_ref().unwrap();
        conn.execute("DELETE FROM haex_s3_backends WHERE id = ?1", [&parent_id])
            .expect("drop parent row");
    }

    let adapter = Arc::new(MockIamAdapter::new(Ok(())));
    let factory = MockIamAdapterFactory {
        adapter: adapter.clone(),
    };

    let err = revoke_storage_share_core(
        &db,
        &hlc,
        &SpaceKeyCache::new(),
        RevokeStorageShareArgs {
            shared_backend_id: seeded.shared_id.clone(),
        },
        &factory,
    )
    .await
    .expect_err("must halt when parent row is gone");

    match err {
        StorageError::ParentBackendMissing { parent_backend_id } => {
            assert_eq!(parent_backend_id, parent_id);
        }
        other => panic!("expected ParentBackendMissing, got {other:?}"),
    }
    assert!(
        adapter.delete_calls.lock().unwrap().is_empty(),
        "IAM must not be touched when parent existence check fails"
    );
}

// ---------------------------------------------------------------------------
// I1a acceptance test: enforced ON DELETE CASCADE on parent_backend_id
// ---------------------------------------------------------------------------
//
// The migration manual_0002_haex_s3_backends_cascade_fk.sql rebuilds
// haex_s3_backends so the self-referential FK on parent_backend_id gets an
// enforced `ON DELETE CASCADE`. This test builds a DB with the *post-migration*
// schema (identical FK spec) and asserts that deleting the parent row
// automatically deletes its `shared_from_space` children.
//
// We do NOT try to run the migration itself against the test fixture —
// migration application is exercised at the database-level integration tier.
// The FK constraint text here is byte-for-byte the same as in the migration
// (see manual_0002_haex_s3_backends_cascade_fk.sql), so a passing test proves
// SQLite enforces the CASCADE that the migration installs.

#[test]
fn parent_delete_cascades_to_shared_children_when_fk_is_enforced() {
    let conn = Connection::open_in_memory().expect("open in-memory DB");
    conn.execute("PRAGMA foreign_keys=ON;", [])
        .expect("enable foreign keys");

    // Post-migration schema for haex_s3_backends — matches the CREATE TABLE
    // in manual_0002_haex_s3_backends_cascade_fk.sql byte-for-byte on the
    // parent_backend_id FK spec.
    conn.execute_batch(
        "CREATE TABLE haex_s3_backends (
            id TEXT PRIMARY KEY NOT NULL,
            type TEXT NOT NULL,
            name TEXT NOT NULL,
            config TEXT NOT NULL,
            enabled INTEGER NOT NULL DEFAULT 1,
            parent_backend_id TEXT REFERENCES haex_s3_backends(id) ON DELETE CASCADE,
            origin_type TEXT NOT NULL DEFAULT 'owned',
            share_prefix TEXT,
            share_access_flags INTEGER,
            created_at TEXT DEFAULT (CURRENT_TIMESTAMP)
        );",
    )
    .expect("create post-migration haex_s3_backends");

    // Seed a parent + a shared_from_space child.
    let parent_id = "parent-owned";
    let shared_id = "shared-from-space";
    conn.execute(
        "INSERT INTO haex_s3_backends (id, type, name, config, enabled, origin_type)
         VALUES (?1, 's3', 'Owner', '{}', 1, 'owned')",
        [parent_id],
    )
    .expect("insert parent");
    conn.execute(
        "INSERT INTO haex_s3_backends
         (id, type, name, config, enabled, parent_backend_id, origin_type)
         VALUES (?1, 's3', 'Shared', '{}', 1, ?2, 'shared_from_space')",
        [shared_id, parent_id],
    )
    .expect("insert child");

    // Sanity: both rows are present pre-cascade.
    let pre_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM haex_s3_backends", [], |r| r.get(0))
        .expect("count pre");
    assert_eq!(pre_count, 2);

    // Directly DELETE the parent — trigger the CASCADE.
    conn.execute("DELETE FROM haex_s3_backends WHERE id = ?1", [parent_id])
        .expect("delete parent");

    // Child must be gone via the FK cascade.
    let post_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM haex_s3_backends", [], |r| r.get(0))
        .expect("count post");
    assert_eq!(
        post_count, 0,
        "child row must be cascade-deleted along with its parent"
    );

    // Also confirm the FK metadata itself carries CASCADE — this catches a
    // regression where the FK is present but declared with NO ACTION.
    let cascade_present: bool = conn
        .query_row(
            "SELECT EXISTS(
                 SELECT 1 FROM pragma_foreign_key_list('haex_s3_backends')
                 WHERE \"from\" = 'parent_backend_id' AND on_delete = 'CASCADE'
             )",
            [],
            |r| r.get(0),
        )
        .expect("query fk list");
    assert!(
        cascade_present,
        "haex_s3_backends.parent_backend_id must declare ON DELETE CASCADE"
    );
}
