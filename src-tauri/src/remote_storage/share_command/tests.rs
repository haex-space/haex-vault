//! Unit tests for `share_storage_backend_core` — the AppState-free inner
//! form of the Tauri command.
//!
//! The IAM control plane is mocked via [`MockIamAdapter`] so tests can exercise
//! happy path + rollback paths without touching AWS or Wasabi. Test DB setup
//! mirrors `iam_admin_creds/tests.rs`: in-memory SQLite + tx-scoped HLC hooks
//! + `ensure_crdt_columns` on every table we write to.

#![cfg(test)]

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use rusqlite::Connection;

use super::{
    share_storage_backend_core, IamAdapterFactory, IamAdminCredHint, ShareStorageBackendArgs,
    SharedStorageBackend,
};
use crate::crdt::column_sig::key_cache::SpaceKeyCache;
use crate::crdt::hlc::HlcService;
use crate::crdt::trigger::ensure_crdt_columns;
use crate::database::connection_context::ConnectionContext;
use crate::database::core::{install_tx_hlc_hooks, register_current_hlc_udf};
use crate::database::DbConnection;
use crate::remote_storage::error::StorageError;
use crate::remote_storage::iam_adapter::{IamAdapter, IamAdapterError, ProviderFlavor, ScopedCred};
use crate::remote_storage::iam_policy::IamPolicy;
use crate::remote_storage::provider::ProviderKind;
use crate::table_names::{TABLE_CRDT_CONFIGS, TABLE_CRDT_DIRTY_TABLES};

// ---------------------------------------------------------------------------
// Mock IAM adapter
// ---------------------------------------------------------------------------

/// Records every call the share flow makes into an inspectable log. Each of
/// the three async methods returns a scripted result — the tests pre-set the
/// slots before invoking `share_storage_backend_core`.
struct MockIamAdapter {
    probe_result: Mutex<Result<bool, IamAdapterError>>,
    create_result: Mutex<Result<ScopedCred, IamAdapterError>>,
    /// (user_name, access_key_id) tuples of every `delete_scoped_user` call.
    delete_calls: Arc<Mutex<Vec<(String, String)>>>,
    /// (user_name, policy_json_length) tuples of every `create_scoped_user`
    /// call — the JSON body itself is small but we only need a shape probe.
    create_calls: Arc<Mutex<Vec<(String, usize)>>>,
}

impl MockIamAdapter {
    fn new(
        probe: Result<bool, IamAdapterError>,
        create: Result<ScopedCred, IamAdapterError>,
    ) -> Self {
        Self {
            probe_result: Mutex::new(probe),
            create_result: Mutex::new(create),
            delete_calls: Arc::new(Mutex::new(Vec::new())),
            create_calls: Arc::new(Mutex::new(Vec::new())),
        }
    }
}

#[async_trait]
impl IamAdapter for MockIamAdapter {
    async fn create_scoped_user(
        &self,
        user_name: &str,
        policy: &IamPolicy,
    ) -> Result<ScopedCred, IamAdapterError> {
        let policy_len = serde_json::to_string(policy).map(|s| s.len()).unwrap_or(0);
        self.create_calls
            .lock()
            .unwrap()
            .push((user_name.to_string(), policy_len));
        // Replace the slot with `NotFound` so a caller that erroneously
        // re-uses the mock sees a distinct signal rather than the same
        // scripted result twice.
        std::mem::replace(
            &mut *self.create_result.lock().unwrap(),
            Err(IamAdapterError::NotFound),
        )
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
        Ok(())
    }

    async fn probe_iam_capability(&self) -> Result<bool, IamAdapterError> {
        let slot = &mut *self.probe_result.lock().unwrap();
        // Replace + return: we clone the result out because IamAdapterError is
        // Clone but Result isn't Copy.
        let out = slot.clone();
        *slot = Err(IamAdapterError::NotFound);
        out
    }
}

/// Factory that hands out a single pre-built mock adapter (wrapped in `Arc`).
struct MockIamAdapterFactory {
    adapter: Arc<MockIamAdapter>,
}

impl IamAdapterFactory for MockIamAdapterFactory {
    fn build(
        &self,
        _cred: &crate::remote_storage::iam_admin_creds::IamAdminCred,
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

/// Build an in-memory DB with all tables the share command reads or writes:
/// `haex_s3_backends`, `haex_shared_space_sync`, `haex_spaces`, plus the
/// two password-manager tables the IAM-admin cred store touches, plus CRDT
/// bookkeeping tables. Seeds one owned S3 backend + one space.
///
/// Returns `(db, hlc, storage_id, space_id)`.
fn setup_share_db() -> (DbConnection, HlcService, String, String) {
    let conn = Connection::open_in_memory().expect("open in-memory DB");
    let hlc_service = HlcService::new_for_testing("test-device-share");
    let ctx = ConnectionContext::new();
    register_current_hlc_udf(&conn, hlc_service.clone(), ctx.clone())
        .expect("register current_hlc UDF");
    install_tx_hlc_hooks(&conn, ctx).expect("install tx-HLC hooks");

    // CRDT bookkeeping tables.
    conn.execute_batch(&format!(
        "CREATE TABLE {} (key TEXT PRIMARY KEY, type TEXT NOT NULL, value TEXT NOT NULL);
         CREATE TABLE {} (table_name TEXT PRIMARY KEY, last_modified TEXT);",
        TABLE_CRDT_CONFIGS, TABLE_CRDT_DIRTY_TABLES,
    ))
    .expect("create crdt bookkeeping");

    // Application tables — column shape mirrored from
    // `src/database/schemas/{storage,spaces}.ts` and the shared_space_sync
    // migration. FK constraints omitted here (they need the extensions +
    // identities parent tables); the share flow does not exercise those FKs.
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

        CREATE TABLE haex_spaces (
            id TEXT PRIMARY KEY NOT NULL,
            type TEXT NOT NULL DEFAULT 'online',
            status TEXT NOT NULL DEFAULT 'active',
            name TEXT NOT NULL,
            owner_identity_id TEXT NOT NULL,
            origin_url TEXT,
            created_at TEXT DEFAULT (CURRENT_TIMESTAMP),
            modified_at TEXT DEFAULT (CURRENT_TIMESTAMP)
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

    // Every table the CRDT helpers write to needs its CRDT metadata columns.
    {
        let tx = conn.unchecked_transaction().expect("crdt-cols tx");
        for table in [
            "haex_s3_backends",
            "haex_shared_space_sync",
            "haex_spaces",
            "haex_passwords_item_details",
            "haex_passwords_item_key_values",
        ] {
            ensure_crdt_columns(&tx, table).expect("ensure crdt cols");
        }
        tx.commit().expect("commit crdt-cols tx");
    }

    // Seed one owned backend + one space so the happy-path finds them.
    let storage_id = rand_string("storage");
    let space_id = rand_string("space");

    let owner_config = serde_json::json!({
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
        rusqlite::params![&storage_id, &owner_config],
    )
    .expect("seed owner backend");

    conn.execute(
        "INSERT INTO haex_spaces (id, type, status, name, owner_identity_id)
         VALUES (?1, 'online', 'active', 'Team Q1', 'ident-owner')",
        rusqlite::params![&space_id],
    )
    .expect("seed space");

    // Runde 5 F2: inserting into `haex_shared_space_sync` now triggers
    // sig-based I2 — the vault must hold a signing key for the space via
    // (haex_space_members ⋈ haex_identities.private_key). Seed those two
    // tables + a valid PKCS8 blob so `SpaceKeyCache::get_or_reload`'s JIT
    // reload succeeds; the sig content is not asserted here.
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
    .expect("seed identities + members tables");
    let pkcs8_b64 = seeded_share_pkcs8_b64();
    conn.execute(
        "INSERT INTO haex_identities (id, did, private_key) VALUES (?1, ?2, ?3)",
        rusqlite::params!["ident-owner", "did:key:zOwnForShareTests", &pkcs8_b64],
    )
    .expect("seed identity");
    conn.execute(
        "INSERT INTO haex_space_members (id, space_id, identity_id)
         VALUES ('mem-owner', ?1, 'ident-owner')",
        rusqlite::params![&space_id],
    )
    .expect("seed membership");

    let db = DbConnection(Arc::new(Mutex::new(Some(conn))));
    (db, hlc_service, storage_id, space_id)
}

/// Deterministic PKCS8 Ed25519 blob for share-command tests (seeded random
/// bytes; content is not asserted, only well-formedness for JIT reload).
fn seeded_share_pkcs8_b64() -> String {
    use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
    let pkcs8_prefix: [u8; 16] = [
        0x30, 0x2e, 0x02, 0x01, 0x00, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x04, 0x22, 0x04,
        0x20,
    ];
    let seed: [u8; 32] = rand::random();
    let mut der = Vec::with_capacity(48);
    der.extend_from_slice(&pkcs8_prefix);
    der.extend_from_slice(&seed);
    BASE64.encode(&der)
}

fn make_hint() -> IamAdminCredHint {
    IamAdminCredHint {
        access_key_id: rand_string("AKIA"),
        secret_access_key: rand_string("secret"),
        provider_type: ProviderKind::Aws,
    }
}

fn make_scoped_cred() -> ScopedCred {
    ScopedCred {
        access_key_id: rand_string("ASIA"),
        secret_access_key: rand_string("s-secret"),
        iam_user_name: rand_string("haex-share"),
    }
}

fn args_with_hint(
    storage_id: &str,
    space_id: &str,
    hint: IamAdminCredHint,
    access_flags: i64,
) -> ShareStorageBackendArgs {
    ShareStorageBackendArgs {
        storage_id: storage_id.to_string(),
        space_id: space_id.to_string(),
        prefix: None,
        object_key: None,
        access_flags,
        iam_admin_cred_hint: Some(hint),
    }
}

// ---------------------------------------------------------------------------
// Happy-path tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn happy_path_writes_row_and_mapping() {
    let (db, hlc, storage_id, space_id) = setup_share_db();
    let scoped = make_scoped_cred();
    let adapter = Arc::new(MockIamAdapter::new(Ok(true), Ok(scoped.clone())));
    let factory = MockIamAdapterFactory {
        adapter: adapter.clone(),
    };

    let args = args_with_hint(&storage_id, &space_id, make_hint(), 0b0011); // LIST|GET

    let result: SharedStorageBackend =
        share_storage_backend_core(&db, &hlc, &SpaceKeyCache::new(), args, &factory)
            .await
            .expect("share should succeed");

    // Response shape reflects the mock output.
    assert_eq!(result.r#type, "s3");
    assert_eq!(result.iam_user_name, scoped.iam_user_name);
    assert!(
        result.name.contains("Team Q1"),
        "response name should include the space's human-readable name, got {}",
        result.name
    );

    // Assert the new haex_s3_backends row was written with the right parent
    // + origin_type + share flags + config (contains the scoped cred, not
    // the owner cred).
    let (parent_id, origin_type, share_flags, config_json): (String, String, i64, String) = {
        let guard = db.0.lock().unwrap();
        let conn = guard.as_ref().unwrap();
        conn.query_row(
            "SELECT parent_backend_id, origin_type, share_access_flags, config
             FROM haex_s3_backends WHERE id = ?1",
            [&result.id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, String>(3)?,
                ))
            },
        )
        .expect("query new backend row")
    };
    assert_eq!(parent_id, storage_id);
    assert_eq!(origin_type, "shared_from_space");
    assert_eq!(share_flags, 0b0011);

    let config: serde_json::Value = serde_json::from_str(&config_json).expect("config JSON parses");
    assert_eq!(
        config.get("accessKeyId").and_then(|v| v.as_str()),
        Some(scoped.access_key_id.as_str()),
        "config must carry the scoped access key, not the owner's"
    );
    assert_eq!(
        config.get("iamUserName").and_then(|v| v.as_str()),
        Some(scoped.iam_user_name.as_str()),
        "config must carry the IAM user name for revoke"
    );
    assert_eq!(
        config.get("bucket").and_then(|v| v.as_str()),
        Some("my-bucket"),
        "bucket must be inherited from owner"
    );

    // Assert the shared-space-sync mapping row exists. `type_label` carries
    // the human-readable row name (migration 0014 renamed `label` ->
    // `type_label`); `category` stays NULL for user-owned (non-extension)
    // shares.
    let (mapping_table, mapping_space, mapping_type, mapping_type_label): (
        String,
        String,
        String,
        String,
    ) = {
        let guard = db.0.lock().unwrap();
        let conn = guard.as_ref().unwrap();
        conn.query_row(
            "SELECT table_name, space_id, type, type_label FROM haex_shared_space_sync
             WHERE space_id = ?1",
            [&space_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                ))
            },
        )
        .expect("query mapping row")
    };
    assert_eq!(mapping_table, "haex_s3_backends");
    assert_eq!(mapping_space, space_id);
    assert_eq!(mapping_type, "cloud_storage");
    assert_eq!(
        mapping_type_label, result.name,
        "type_label must carry the shared row's display name"
    );

    // Adapter should NOT have been asked to delete the scoped user.
    assert!(
        adapter.delete_calls.lock().unwrap().is_empty(),
        "no rollback should have fired on the happy path"
    );
    // But it SHOULD have received exactly one create call, on our user_name.
    let creates = adapter.create_calls.lock().unwrap();
    assert_eq!(creates.len(), 1);
    assert!(creates[0].0.starts_with("haex-share-"));
}

#[tokio::test]
async fn iam_admin_cred_missing_when_no_hint_and_no_prior_cred() {
    let (db, hlc, storage_id, space_id) = setup_share_db();
    let adapter = Arc::new(MockIamAdapter::new(
        Ok(true),
        Err(IamAdapterError::NotFound),
    ));
    let factory = MockIamAdapterFactory { adapter };
    let args = ShareStorageBackendArgs {
        storage_id: storage_id.clone(),
        space_id,
        prefix: None,
        object_key: None,
        access_flags: 0b0001,
        iam_admin_cred_hint: None,
    };

    let err = share_storage_backend_core(&db, &hlc, &SpaceKeyCache::new(), args, &factory)
        .await
        .expect_err("must fail without a cred");
    match err {
        StorageError::IamAdminCredMissing { storage_id: id } => assert_eq!(id, storage_id),
        other => panic!("expected IamAdminCredMissing, got {other:?}"),
    }
}

#[tokio::test]
async fn iam_admin_cred_missing_cleared_by_hint() {
    // First call with hint → stores cred + succeeds. Second call without hint
    // → loads the stored cred instead of returning IamAdminCredMissing.
    //
    // Run 2 uses a DIFFERENT `access_flags` so the idempotency short-circuit
    // (which returns the run-1 row unchanged) does NOT fire — we specifically
    // want to exercise the fresh-provision path with only a stored cred.
    let (db, hlc, storage_id, space_id) = setup_share_db();

    // Run 1: with hint. Fresh adapter, script probe=Ok(true), create=Ok(cred).
    let scoped_1 = make_scoped_cred();
    let adapter_1 = Arc::new(MockIamAdapter::new(Ok(true), Ok(scoped_1.clone())));
    let factory_1 = MockIamAdapterFactory { adapter: adapter_1 };
    let args_1 = args_with_hint(&storage_id, &space_id, make_hint(), 0b0001);
    let out_1 = share_storage_backend_core(&db, &hlc, &SpaceKeyCache::new(), args_1, &factory_1)
        .await
        .expect("first share should succeed via hint");
    assert_eq!(out_1.iam_user_name, scoped_1.iam_user_name);

    // Run 2: without hint, different flags so dedupe skips. Must reuse the
    // stored cred (no IamAdminCredMissing).
    let scoped_2 = make_scoped_cred();
    let adapter_2 = Arc::new(MockIamAdapter::new(Ok(true), Ok(scoped_2.clone())));
    let factory_2 = MockIamAdapterFactory { adapter: adapter_2 };
    let args_2 = ShareStorageBackendArgs {
        storage_id: storage_id.clone(),
        space_id,
        prefix: None,
        object_key: None,
        access_flags: 0b0011,
        iam_admin_cred_hint: None,
    };
    let out_2 = share_storage_backend_core(&db, &hlc, &SpaceKeyCache::new(), args_2, &factory_2)
        .await
        .expect("second share should succeed by reusing stored cred");
    assert_eq!(out_2.iam_user_name, scoped_2.iam_user_name);
}

#[tokio::test]
async fn probe_access_denied_returns_iam_admin_insufficient() {
    let (db, hlc, storage_id, space_id) = setup_share_db();
    let adapter = Arc::new(MockIamAdapter::new(
        Err(IamAdapterError::AccessDenied("denied".to_string())),
        Err(IamAdapterError::NotFound),
    ));
    let factory = MockIamAdapterFactory {
        adapter: adapter.clone(),
    };
    let args = args_with_hint(&storage_id, &space_id, make_hint(), 0b0001);

    let err = share_storage_backend_core(&db, &hlc, &SpaceKeyCache::new(), args, &factory)
        .await
        .expect_err("must reject when probe denies");
    assert!(matches!(err, StorageError::IamAdminInsufficient));
    // No user should be created — probe rejects before create runs.
    assert!(adapter.create_calls.lock().unwrap().is_empty());
}

#[tokio::test]
async fn create_scoped_user_failure_writes_no_db_rows() {
    let (db, hlc, storage_id, space_id) = setup_share_db();
    let adapter = Arc::new(MockIamAdapter::new(
        Ok(true),
        Err(IamAdapterError::Network("boom".to_string())),
    ));
    let factory = MockIamAdapterFactory { adapter };
    let args = args_with_hint(&storage_id, &space_id, make_hint(), 0b0001);

    let err = share_storage_backend_core(&db, &hlc, &SpaceKeyCache::new(), args, &factory)
        .await
        .expect_err("must propagate adapter failure");
    match err {
        StorageError::IamOperationFailed { operation, reason } => {
            assert_eq!(
                operation, "create_scoped_user",
                "operation must identify the failing IAM step"
            );
            assert!(reason.contains("boom"), "reason must carry the cause");
        }
        other => panic!("expected IamOperationFailed, got {other:?}"),
    }

    // No new backend row should exist (only the seeded one).
    let count: i64 = {
        let guard = db.0.lock().unwrap();
        let conn = guard.as_ref().unwrap();
        conn.query_row(
            "SELECT COUNT(*) FROM haex_s3_backends WHERE origin_type = 'shared_from_space'",
            [],
            |row| row.get(0),
        )
        .expect("count shared rows")
    };
    assert_eq!(count, 0);
}

#[tokio::test]
async fn db_failure_after_iam_success_calls_delete_scoped_user() {
    // Force the DB insert to fail after the adapter has already provisioned
    // the scoped user. The share command should then call delete_scoped_user
    // to roll the provider-side change back, AND the just-inserted
    // s3_backends row must be deleted so we don't leak an orphan.
    let (db, hlc, storage_id, space_id) = setup_share_db();

    // We need a failure AFTER the adapter's create_scoped_user has run — so
    // dropping haex_s3_backends is wrong (that table is READ during
    // load_owner_backend, before the adapter). Rebuild
    // haex_shared_space_sync with a NOT NULL column the code never fills,
    // so the s3_backends INSERT succeeds but the mapping INSERT fails on
    // the constraint. Keeping the table around means `find_existing_share`
    // can still run its SELECT and correctly report "no prior share".
    {
        let guard = db.0.lock().unwrap();
        let conn = guard.as_ref().unwrap();
        conn.execute_batch(
            "DROP TABLE haex_shared_space_sync;
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
                 must_be_present TEXT NOT NULL,
                 created_at TEXT DEFAULT (CURRENT_TIMESTAMP)
             );",
        )
        .expect("rebuild mapping table with breaking NOT NULL column");
        let tx = conn
            .unchecked_transaction()
            .expect("begin crdt-cols tx for rebuilt mapping table");
        ensure_crdt_columns(&tx, "haex_shared_space_sync").expect("ensure crdt cols");
        tx.commit().expect("commit crdt-cols tx");
    }

    let scoped = make_scoped_cred();
    let adapter = Arc::new(MockIamAdapter::new(Ok(true), Ok(scoped.clone())));
    let factory = MockIamAdapterFactory {
        adapter: adapter.clone(),
    };
    let args = args_with_hint(&storage_id, &space_id, make_hint(), 0b0001);

    let err = share_storage_backend_core(&db, &hlc, &SpaceKeyCache::new(), args, &factory)
        .await
        .expect_err("db insert must fail");
    assert!(
        matches!(err, StorageError::DatabaseError { .. }),
        "expected DatabaseError, got {err:?}"
    );

    // Now: the adapter must have received exactly one delete call, targeting
    // the same user_name + access_key_id it just handed out.
    let deletes = adapter.delete_calls.lock().unwrap();
    assert_eq!(
        deletes.len(),
        1,
        "adapter.delete_scoped_user must be called once on DB failure"
    );
    assert_eq!(deletes[0].1, scoped.access_key_id);
    assert!(deletes[0].0.starts_with("haex-share-"));

    // Orphan invariant: the s3_backends row for this parent must have been
    // cleaned up when the mapping insert failed. Only the seeded owner row
    // remains — no `shared_from_space` child.
    let orphan_count: i64 = {
        let guard = db.0.lock().unwrap();
        let conn = guard.as_ref().unwrap();
        conn.query_row(
            "SELECT COUNT(*) FROM haex_s3_backends \
             WHERE parent_backend_id = ?1",
            [&storage_id],
            |row| row.get(0),
        )
        .expect("count orphan child rows")
    };
    assert_eq!(
        orphan_count, 0,
        "mapping-insert failure must delete the just-inserted s3_backends \
         row; leaving an orphan share row is not acceptable"
    );
}

#[tokio::test]
async fn share_returns_existing_when_called_twice_with_same_args() {
    // Idempotency guard: a double-click on Share must not create a second
    // IAM user or a second DB row. The second call should short-circuit
    // to the row created by the first, with `create_calls == 1` on the
    // adapter.
    let (db, hlc, storage_id, space_id) = setup_share_db();

    // Adapter script for run 1: probe ok + create ok. Run 2 will short-
    // circuit before touching the adapter, so the create-slot never runs.
    let scoped = make_scoped_cred();
    let adapter = Arc::new(MockIamAdapter::new(Ok(true), Ok(scoped.clone())));
    let factory = MockIamAdapterFactory {
        adapter: adapter.clone(),
    };

    let args_1 = args_with_hint(&storage_id, &space_id, make_hint(), 0b0011);
    let out_1 = share_storage_backend_core(&db, &hlc, &SpaceKeyCache::new(), args_1, &factory)
        .await
        .expect("first share should succeed");

    // Second call: identical scope, no hint (dedupe runs before the cred
    // load, so IamAdminCredMissing must not fire even without a hint).
    let args_2 = ShareStorageBackendArgs {
        storage_id: storage_id.clone(),
        space_id: space_id.clone(),
        prefix: None,
        object_key: None,
        access_flags: 0b0011,
        iam_admin_cred_hint: None,
    };
    let out_2 = share_storage_backend_core(&db, &hlc, &SpaceKeyCache::new(), args_2, &factory)
        .await
        .expect("second share should short-circuit to the existing row");

    // Same DB row surfaced twice — id and IAM user name must be identical.
    assert_eq!(out_1.id, out_2.id, "dedupe must return the same row id");
    assert_eq!(out_1.iam_user_name, out_2.iam_user_name);
    assert_eq!(out_1.iam_user_name, scoped.iam_user_name);

    // Adapter received exactly one create call across both invocations.
    let creates = adapter.create_calls.lock().unwrap();
    assert_eq!(
        creates.len(),
        1,
        "dedupe must not double-provision IAM (real AWS $)"
    );

    // DB has exactly one `shared_from_space` row for this parent.
    let child_count: i64 = {
        let guard = db.0.lock().unwrap();
        let conn = guard.as_ref().unwrap();
        conn.query_row(
            "SELECT COUNT(*) FROM haex_s3_backends \
             WHERE parent_backend_id = ?1 \
               AND origin_type = 'shared_from_space' \
               AND share_access_flags = 3",
            [&storage_id],
            |row| row.get(0),
        )
        .expect("count child rows")
    };
    assert_eq!(child_count, 1, "dedupe must not insert a second child row");
}

// ---------------------------------------------------------------------------
// Argument validation
// ---------------------------------------------------------------------------

#[tokio::test]
async fn access_flags_zero_is_invalid() {
    let (db, hlc, storage_id, space_id) = setup_share_db();
    let adapter = Arc::new(MockIamAdapter::new(
        Ok(true),
        Err(IamAdapterError::NotFound),
    ));
    let factory = MockIamAdapterFactory {
        adapter: adapter.clone(),
    };
    let args = args_with_hint(&storage_id, &space_id, make_hint(), 0);

    let err = share_storage_backend_core(&db, &hlc, &SpaceKeyCache::new(), args, &factory)
        .await
        .expect_err("access_flags=0 must be rejected");
    assert!(matches!(err, StorageError::InvalidArgs { .. }));
    assert!(adapter.create_calls.lock().unwrap().is_empty());
}

#[tokio::test]
async fn prefix_with_iam_wildcard_characters_is_invalid() {
    // `*` / `?` are IAM wildcards in Resource ARNs and StringLike conditions;
    // a folder literally named `logs*` must not silently widen the policy.
    for bad_prefix in ["logs*", "foo?bar", "media/*"] {
        let (db, hlc, storage_id, space_id) = setup_share_db();
        let adapter = Arc::new(MockIamAdapter::new(
            Ok(true),
            Err(IamAdapterError::NotFound),
        ));
        let factory = MockIamAdapterFactory {
            adapter: adapter.clone(),
        };
        let mut args = args_with_hint(&storage_id, &space_id, make_hint(), 0b0011);
        args.prefix = Some(bad_prefix.to_string());

        let err = share_storage_backend_core(&db, &hlc, &SpaceKeyCache::new(), args, &factory)
            .await
            .expect_err("wildcard prefix must be rejected");
        assert!(
            matches!(err, StorageError::InvalidArgs { .. }),
            "prefix {bad_prefix:?} must yield InvalidArgs, got {err:?}"
        );
        assert!(adapter.create_calls.lock().unwrap().is_empty());
    }
}

#[tokio::test]
async fn object_key_present_returns_object_scope_not_yet_supported() {
    let (db, hlc, storage_id, space_id) = setup_share_db();
    let adapter = Arc::new(MockIamAdapter::new(
        Ok(true),
        Err(IamAdapterError::NotFound),
    ));
    let factory = MockIamAdapterFactory {
        adapter: adapter.clone(),
    };
    let args = ShareStorageBackendArgs {
        storage_id,
        space_id,
        prefix: None,
        object_key: Some("some/key.txt".to_string()),
        access_flags: 0b0011,
        iam_admin_cred_hint: Some(make_hint()),
    };

    let err = share_storage_backend_core(&db, &hlc, &SpaceKeyCache::new(), args, &factory)
        .await
        .expect_err("object scope must be v1-rejected");
    assert!(matches!(err, StorageError::ObjectScopeNotYetSupported));
    assert!(adapter.create_calls.lock().unwrap().is_empty());
}

#[tokio::test]
async fn storage_not_found_when_id_unknown() {
    let (db, hlc, _storage_id, space_id) = setup_share_db();
    let adapter = Arc::new(MockIamAdapter::new(
        Ok(true),
        Err(IamAdapterError::NotFound),
    ));
    let factory = MockIamAdapterFactory { adapter };

    let unknown_id = rand_string("no-such-storage");
    let args = args_with_hint(&unknown_id, &space_id, make_hint(), 0b0001);

    let err = share_storage_backend_core(&db, &hlc, &SpaceKeyCache::new(), args, &factory)
        .await
        .expect_err("unknown storage_id must fail");
    match err {
        StorageError::StorageNotFound { storage_id: id } => assert_eq!(id, unknown_id),
        other => panic!("expected StorageNotFound, got {other:?}"),
    }
}

#[tokio::test]
async fn minio_provider_rejected_in_hint() {
    // `ProviderKind` accepts MinIO but the adapter conversion refuses it.
    // Rejection must happen BEFORE any IAM call or on-disk cred write.
    let (db, hlc, storage_id, space_id) = setup_share_db();
    let adapter = Arc::new(MockIamAdapter::new(
        Ok(true),
        Err(IamAdapterError::NotFound),
    ));
    let factory = MockIamAdapterFactory {
        adapter: adapter.clone(),
    };

    let minio_hint = IamAdminCredHint {
        access_key_id: rand_string("AKIA"),
        secret_access_key: rand_string("secret"),
        provider_type: ProviderKind::Minio,
    };
    let args = args_with_hint(&storage_id, &space_id, minio_hint, 0b0001);

    let err = share_storage_backend_core(&db, &hlc, &SpaceKeyCache::new(), args, &factory)
        .await
        .expect_err("MinIO must be rejected upfront");
    match err {
        StorageError::UnsupportedProvider { provider_type } => {
            assert!(
                provider_type.contains("minio"),
                "message should mention minio, got {provider_type}"
            );
        }
        other => panic!("expected UnsupportedProvider, got {other:?}"),
    }
    // No IAM calls, no probe — early bail before we spend on the provider.
    assert!(adapter.create_calls.lock().unwrap().is_empty());
}

#[test]
fn unknown_provider_string_fails_at_serde_boundary() {
    // Deserialising the args from the Tauri frontend rejects any value not
    // in the closed `ProviderKind` set. This is the compile-time-flavored
    // guarantee replacing the old runtime pairwise validators.
    let raw = serde_json::json!({
        "accessKeyId": "AKIA",
        "secretAccessKey": "secret",
        "providerType": "gcs",
    });
    let res: Result<IamAdminCredHint, _> = serde_json::from_value(raw);
    assert!(
        res.is_err(),
        "unknown providerType must fail deserialisation"
    );
}

// ---------------------------------------------------------------------------
// Error serialization round-trip
// ---------------------------------------------------------------------------

#[test]
fn iam_admin_cred_missing_serializes_with_camel_case_tag() {
    // Frontends pattern-match on the `type` discriminator + camelCase field
    // names from the `details` sub-object. Pin the shape so a rename accident
    // becomes a red test rather than a silent frontend breakage.
    let err = StorageError::IamAdminCredMissing {
        storage_id: "abc-123".to_string(),
    };
    let json = serde_json::to_value(&err).expect("serialize");
    assert_eq!(json["type"], "IamAdminCredMissing");
    // Fields are serialized as-is (no rename), matching how the existing
    // StorageError variants encode `reason`, `id`, etc. — the frontend
    // reads `details.storage_id`.
    assert_eq!(json["details"]["storage_id"], "abc-123");
}
