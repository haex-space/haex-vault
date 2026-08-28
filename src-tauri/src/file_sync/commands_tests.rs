//! Tests for the cloud sync-rule / space-binding consistency check in
//! `create_provider("cloud", ...)`.
//!
//! Exercises `verify_cloud_space_binding` directly against an in-memory
//! `haex_shared_space_sync` table — mirrors the join
//! `remote_storage::share_command::find_existing_share` uses to recover a
//! shared backend's bound space (`row_pks[0]` holds the backend id).

use std::sync::{Arc, Mutex as StdMutex};

use rusqlite::{params, Connection};
use uuid::Uuid;

use crate::database::DbConnection;
use crate::file_sync::commands::{verify_cloud_space_binding, FileSyncCommandError};

// ---------------------------------------------------------------------------
// Phase 4 Round F3b — assemble_cloud_provider wire-up seam
// ---------------------------------------------------------------------------
//
// The three tests below pin `assemble_cloud_provider` — the seam
// `create_provider("cloud", ...)` delegates to. They cover:
//
// 1. Space-scoped rule + shared_access row present → the outermost decorator
//    must be `ScopedProvider` (so `display_name` starts with `scoped(`).
// 2. Space-scoped rule + no shared_access row for this viewer → clear error,
//    not silent fallthrough to owner-only credentials.
// 3. Owner-only rule → no `ScopedProvider` wrap (regression guard for the
//    F2 path).
//
// The helper accepts only the two `AppState` fields the cloud arm needs
// (DbConnection + `vault_key` slot), so tests plumb both by hand. This
// mirrors the F3a `wrap_helper_*` pattern in `crypto/tests.rs` and avoids
// standing up a full `AppState` fake for a wire-up assertion.

mod assemble_cloud_provider_tests {
    use std::sync::{Arc, Mutex as StdMutex};

    use rusqlite::{params, Connection};
    use uuid::Uuid;
    use zeroize::Zeroizing;

    use crate::database::DbConnection;
    use crate::file_sync::commands::{assemble_cloud_provider, FileSyncCommandError};
    use crate::remote_storage::iam_adapter::ScopedCred;
    use crate::remote_storage::shared_access::crypto::seal_scoped_cred;

    /// Random 32-byte epoch key. Kept out of CodeQL's hardcoded-credential
    /// net.
    fn random_epoch_key() -> [u8; 32] {
        rand::random()
    }

    /// Random vault-key slot. Owner-only sealing uses this — the space-
    /// scoped path never consults it, but assemble_cloud_provider still
    /// requires the type.
    fn slot_with(key: [u8; 32]) -> Arc<StdMutex<Option<Zeroizing<[u8; 32]>>>> {
        Arc::new(StdMutex::new(Some(Zeroizing::new(key))))
    }

    /// Build an in-memory DB carrying every table the cloud-arm seam
    /// reads: the space-binding chokepoint (`verify_cloud_space_binding`),
    /// the backend config store (`get_backend_instance_from_db_with_overrides`),
    /// the shared-access rows and MLS epoch keys the space-scoped path
    /// looks up, and the vault-owner-DID lookup that names the viewer.
    ///
    /// No CRDT triggers are installed — the seam only *reads*, and
    /// `select_with_crdt` is a thin wrapper over `SELECT` with no
    /// bookkeeping side effects. This keeps the test setup local and
    /// mirrors the pattern used by the existing `verify_cloud_space_binding`
    /// suite in this file.
    fn setup_full_db() -> DbConnection {
        let conn = Connection::open_in_memory().expect("in-memory DB");
        conn.execute_batch(
            "CREATE TABLE haex_shared_space_sync (
                id TEXT PRIMARY KEY,
                table_name TEXT NOT NULL,
                row_pks TEXT NOT NULL,
                space_id TEXT NOT NULL
            );
            CREATE TABLE haex_s3_backends (
                id TEXT PRIMARY KEY,
                type TEXT NOT NULL DEFAULT 's3',
                config TEXT NOT NULL DEFAULT '{}',
                origin_type TEXT NOT NULL DEFAULT 'owned'
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
            CREATE TABLE haex_mls_sync_keys (
                id TEXT PRIMARY KEY,
                space_id TEXT NOT NULL,
                epoch INTEGER NOT NULL,
                key_data TEXT NOT NULL
            );
            CREATE TABLE haex_spaces (
                id TEXT PRIMARY KEY,
                type TEXT NOT NULL,
                owner_identity_id TEXT NOT NULL
            );
            CREATE TABLE haex_identities (
                id TEXT PRIMARY KEY,
                did TEXT NOT NULL,
                name TEXT,
                source TEXT,
                private_key TEXT
            );",
        )
        .expect("schema setup");
        DbConnection(Arc::new(StdMutex::new(Some(conn))))
    }

    /// Seed a valid S3-shaped config for the given backend id. `origin_type`
    /// decides whether the space-binding check treats it as shared or owner-
    /// only. The `accessKeyId` / `secretAccessKey` in the stored config are
    /// left as sentinel values so any test that neglects the ScopedCred
    /// override would surface those bytes in the resulting backend.
    fn seed_backend_config(db: &DbConnection, backend_id: &str, origin_type: &str) {
        let config = serde_json::json!({
            "region": "us-east-1",
            "bucket": "test-bucket",
            "accessKeyId": "STORED_ADMIN_KEY",
            "secretAccessKey": "STORED_ADMIN_SECRET",
        })
        .to_string();
        let guard = db.0.lock().expect("db lock");
        let conn = guard.as_ref().expect("db open");
        conn.execute(
            "INSERT INTO haex_s3_backends (id, type, config, origin_type) \
             VALUES (?1, 's3', ?2, ?3)",
            params![backend_id, config, origin_type],
        )
        .expect("seed backend row");
    }

    /// Bind a backend to a space through `haex_shared_space_sync` — the
    /// mapping `verify_cloud_space_binding` consults.
    fn seed_share_mapping(db: &DbConnection, backend_id: &str, space_id: &str) {
        let row_pks = serde_json::to_string(&vec![backend_id]).expect("serialize row_pks");
        let guard = db.0.lock().expect("db lock");
        let conn = guard.as_ref().expect("db open");
        conn.execute(
            "INSERT INTO haex_shared_space_sync (id, table_name, row_pks, space_id) \
             VALUES (?1, 'haex_s3_backends', ?2, ?3)",
            params![Uuid::new_v4().to_string(), row_pks, space_id],
        )
        .expect("seed share mapping");
    }

    /// Store the base64-encoded epoch key at `(space_id, epoch)` — the
    /// shape `resolve_key` decodes.
    fn seed_mls_key(db: &DbConnection, space_id: &str, epoch: u64, key: &[u8; 32]) {
        use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
        let key_b64 = B64.encode(key);
        let guard = db.0.lock().expect("db lock");
        let conn = guard.as_ref().expect("db open");
        conn.execute(
            "INSERT INTO haex_mls_sync_keys (id, space_id, epoch, key_data) \
             VALUES (?1, ?2, ?3, ?4)",
            params![Uuid::new_v4().to_string(), space_id, epoch as i64, key_b64],
        )
        .expect("seed mls key");
    }

    /// Seed a vault-owner identity + `haex_spaces` row of `type='vault'`,
    /// so `resolve_vault_owner_did` returns `member_did`.
    fn seed_vault_owner(db: &DbConnection, member_did: &str) {
        let identity_id = Uuid::new_v4().to_string();
        let guard = db.0.lock().expect("db lock");
        let conn = guard.as_ref().expect("db open");
        conn.execute(
            "INSERT INTO haex_identities (id, did, name) VALUES (?1, ?2, 'vault owner')",
            params![identity_id, member_did],
        )
        .expect("seed identity");
        conn.execute(
            "INSERT INTO haex_spaces (id, type, owner_identity_id) VALUES (?1, 'vault', ?2)",
            params![Uuid::new_v4().to_string(), identity_id],
        )
        .expect("seed vault space");
    }

    /// Seal a fresh `ScopedCred` under the epoch key and insert the
    /// `haex_s3_shared_access` row `find_shared_access` will fetch.
    ///
    /// Returns the `ScopedCred` that was sealed, so a test can assert the
    /// unsealed override actually threads through the backend factory —
    /// though the three F3b tests only need the display-name assertion.
    fn seed_shared_access_row(
        db: &DbConnection,
        space_id: &str,
        backend_id: &str,
        member_did: &str,
        epoch: u64,
        epoch_key: &[u8; 32],
    ) -> ScopedCred {
        let cred = ScopedCred {
            access_key_id: format!("SCOPED_{}", Uuid::new_v4().simple()),
            secret_access_key: format!("SECRET_{}", Uuid::new_v4().simple()),
            iam_user_name: format!("scoped-user-{}", Uuid::new_v4().simple()),
        };
        let sealed =
            seal_scoped_cred(&cred, epoch_key, epoch).expect("seal ScopedCred for test row");
        let guard = db.0.lock().expect("db lock");
        let conn = guard.as_ref().expect("db open");
        conn.execute(
            "INSERT INTO haex_s3_shared_access \
             (id, space_id, backend_id, member_did, encrypted_cred, epoch, created_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, '2026-08-28T00:00:00Z')",
            params![
                Uuid::new_v4().to_string(),
                space_id,
                backend_id,
                member_did,
                sealed,
                epoch as i64,
            ],
        )
        .expect("seed shared_access row");
        cred
    }

    /// Space-scoped rule with a matching `haex_s3_shared_access` row —
    /// the outermost decorator must be `ScopedProvider`.
    #[tokio::test]
    async fn assemble_cloud_with_space_installs_scoped_provider() {
        let db = setup_full_db();
        let space_id = format!("space-{}", Uuid::new_v4().simple());
        let backend_id = format!("backend-{}", Uuid::new_v4().simple());
        let member_did = "did:key:zVaultOwner";
        let epoch: u64 = 7;
        let sync_key = random_epoch_key();

        seed_backend_config(&db, &backend_id, "shared_from_space");
        seed_share_mapping(&db, &backend_id, &space_id);
        seed_mls_key(&db, &space_id, epoch, &sync_key);
        seed_vault_owner(&db, member_did);
        // The seal path must use the same file-key that `resolve_key` will
        // return on the open side — i.e. the domain-separated derivative
        // of the stored sync_key, not the sync_key itself. Ask the
        // resolver directly rather than duplicating its derivation.
        let epoch_key = crate::file_sync::crypto::key_resolver::resolve_key(&space_id, epoch, &db)
            .expect("resolve epoch key for test seal");
        seed_shared_access_row(&db, &space_id, &backend_id, member_did, epoch, &epoch_key);

        let cfg = serde_json::json!({
            "backendId": backend_id,
            "spaceId": space_id,
            "prefix": format!("space-{}/", space_id),
        });
        let provider = assemble_cloud_provider(
            &cfg,
            "rule-scoped",
            /* is_target */ false,
            DbConnection(db.0.clone()),
            slot_with(random_epoch_key()),
        )
        .await
        .expect("assemble_cloud_provider must succeed on shared-backend happy path");

        let name = provider.display_name();
        assert!(
            name.starts_with("scoped("),
            "outermost decorator must be ScopedProvider — got display_name={name:?}"
        );
    }

    /// Space-scoped rule against a shared backend but no
    /// `haex_s3_shared_access` row for the current viewer (revoked, or
    /// never provisioned) must fail with a clear error — silently falling
    /// back to the stored owner credentials would ship writes under the
    /// wrong identity.
    #[tokio::test]
    async fn assemble_cloud_without_shared_access_row_errors() {
        let db = setup_full_db();
        let space_id = format!("space-{}", Uuid::new_v4().simple());
        let backend_id = format!("backend-{}", Uuid::new_v4().simple());
        let member_did = "did:key:zVaultOwner";
        // MLS key is seeded so the failure is NOT "missing epoch key" — it
        // must be the shared_access row that's missing.
        let epoch: u64 = 3;
        seed_backend_config(&db, &backend_id, "shared_from_space");
        seed_share_mapping(&db, &backend_id, &space_id);
        seed_mls_key(&db, &space_id, epoch, &random_epoch_key());
        seed_vault_owner(&db, member_did);
        // Deliberately do NOT insert the haex_s3_shared_access row.

        let cfg = serde_json::json!({
            "backendId": backend_id,
            "spaceId": space_id,
        });
        // `Arc<dyn SyncProvider>` isn't `Debug`, so `expect_err` on the Ok
        // arm won't compile — match by hand instead, mirroring the F3a
        // `wrap_helper_rejects_non_string_space_id` pattern.
        let err = match assemble_cloud_provider(
            &cfg,
            "rule-no-row",
            /* is_target */ false,
            DbConnection(db.0.clone()),
            slot_with(random_epoch_key()),
        )
        .await
        {
            Ok(_) => panic!("missing shared_access row must surface as a clear error"),
            Err(e) => e,
        };

        // Match on the specific variant + message shape — a
        // ProviderError for a bucket-list timeout would also match
        // `InvalidConfig(_) | ProviderError(_)` and hide the actual bug.
        match err {
            FileSyncCommandError::InvalidConfig(msg) => {
                assert!(
                    msg.contains("haex_s3_shared_access"),
                    "error must call out the missing shared_access row, got: {msg}"
                );
                assert!(
                    msg.contains(member_did),
                    "error must name the viewer DID that had no row, got: {msg}"
                );
            }
            other => panic!("expected InvalidConfig naming haex_s3_shared_access, got {other:?}"),
        }
    }

    /// Owner-only rule (no `spaceId`) must NOT install `ScopedProvider`.
    /// Regression guard against a future edit that unconditionally wraps
    /// — the F2 own-vault path stays observable as its own display_name.
    #[tokio::test]
    async fn assemble_cloud_owner_only_does_not_install_scoped_provider() {
        let db = setup_full_db();
        let backend_id = format!("backend-{}", Uuid::new_v4().simple());
        seed_backend_config(&db, &backend_id, "owned");

        let cfg = serde_json::json!({
            "backendId": backend_id,
            "prefix": "",
        });
        let provider = assemble_cloud_provider(
            &cfg,
            "rule-owner-only",
            /* is_target */ false,
            DbConnection(db.0.clone()),
            slot_with(random_epoch_key()),
        )
        .await
        .expect("owner-only path must build without a shared_access lookup");

        let name = provider.display_name();
        assert!(
            !name.starts_with("scoped("),
            "owner-only path must NOT wrap in ScopedProvider — got display_name={name:?}"
        );
    }
}

fn setup_db() -> DbConnection {
    let conn = Connection::open_in_memory().expect("in-memory DB");
    conn.execute_batch(
        "CREATE TABLE haex_shared_space_sync (
            id TEXT PRIMARY KEY,
            table_name TEXT NOT NULL,
            row_pks TEXT NOT NULL,
            space_id TEXT NOT NULL
        );
        CREATE TABLE haex_s3_backends (
            id TEXT PRIMARY KEY,
            origin_type TEXT NOT NULL DEFAULT 'owned'
        );",
    )
    .expect("schema setup");
    DbConnection(Arc::new(StdMutex::new(Some(conn))))
}

fn seed_backend(db: &DbConnection, backend_id: &str, origin_type: &str) {
    let guard = db.0.lock().expect("db lock");
    let conn = guard.as_ref().expect("db open");
    conn.execute(
        "INSERT INTO haex_s3_backends (id, origin_type) VALUES (?1, ?2)",
        params![backend_id, origin_type],
    )
    .expect("seed backend row");
}

fn seed_share_mapping(db: &DbConnection, backend_id: &str, space_id: &str) {
    let guard = db.0.lock().expect("db lock");
    let conn = guard.as_ref().expect("db open");
    let row_pks = serde_json::to_string(&vec![backend_id]).expect("serialize row_pks");
    conn.execute(
        "INSERT INTO haex_shared_space_sync (id, table_name, row_pks, space_id) \
         VALUES (?1, 'haex_s3_backends', ?2, ?3)",
        params![Uuid::new_v4().to_string(), row_pks, space_id],
    )
    .expect("seed share row");
}

/// Full shared-backend seed: the `haex_shared_space_sync` mapping *and* the
/// `haex_s3_backends` row carrying `origin_type = 'shared_from_space'`. The
/// production lookup requires both — a mapping alone doesn't prove provenance.
fn seed_share(db: &DbConnection, backend_id: &str, space_id: &str) {
    seed_backend(db, backend_id, "shared_from_space");
    seed_share_mapping(db, backend_id, space_id);
}

// Fresh UUIDs per test — no literal ids, and isolates each case's rows from
// every other test's in-memory DB (each gets its own connection anyway, but
// this also keeps assertions readable without magic strings).
fn fresh_id() -> String {
    Uuid::new_v4().to_string()
}

#[test]
fn owner_only_backend_without_space_id_is_allowed() {
    let db = setup_db();
    let backend_id = fresh_id();
    assert!(verify_cloud_space_binding(&backend_id, None, &db).is_ok());
}

#[test]
fn owner_only_backend_with_space_id_is_rejected() {
    let db = setup_db();
    let backend_id = fresh_id();
    let space_id = fresh_id();
    let err = verify_cloud_space_binding(&backend_id, Some(&space_id), &db)
        .expect_err("owner-only backend + spaceId must be rejected");
    assert!(matches!(err, FileSyncCommandError::InvalidConfig(_)));
}

#[test]
fn shared_backend_with_matching_space_id_is_allowed() {
    let db = setup_db();
    let backend_id = fresh_id();
    let space_id = fresh_id();
    seed_share(&db, &backend_id, &space_id);
    assert!(verify_cloud_space_binding(&backend_id, Some(&space_id), &db).is_ok());
}

#[test]
fn shared_backend_with_mismatched_space_id_is_rejected() {
    let db = setup_db();
    let backend_id = fresh_id();
    let bound_space = fresh_id();
    let other_space = fresh_id();
    seed_share(&db, &backend_id, &bound_space);
    let err = verify_cloud_space_binding(&backend_id, Some(&other_space), &db)
        .expect_err("mismatched spaceId must be rejected");
    assert!(matches!(err, FileSyncCommandError::InvalidConfig(_)));
}

#[test]
fn shared_backend_without_space_id_is_rejected() {
    let db = setup_db();
    let backend_id = fresh_id();
    let bound_space = fresh_id();
    seed_share(&db, &backend_id, &bound_space);
    let err = verify_cloud_space_binding(&backend_id, None, &db)
        .expect_err("missing spaceId against a shared backend must be rejected");
    assert!(matches!(err, FileSyncCommandError::InvalidConfig(_)));
}

/// A stale `haex_shared_space_sync` mapping pointing at an owned backend
/// must not be treated as a space binding — only `origin_type =
/// 'shared_from_space'` counts as authoritative provenance. So the backend
/// still looks owner-only and a `spaceId` against it is rejected.
#[test]
fn owned_backend_with_stale_mapping_is_treated_as_unshared() {
    let db = setup_db();
    let backend_id = fresh_id();
    let stale_space = fresh_id();
    seed_backend(&db, &backend_id, "owned");
    seed_share_mapping(&db, &backend_id, &stale_space);

    assert!(
        verify_cloud_space_binding(&backend_id, None, &db).is_ok(),
        "owned backend + stale mapping must still allow spaceId-less rules"
    );
    let err = verify_cloud_space_binding(&backend_id, Some(&stale_space), &db)
        .expect_err("owned backend must reject any spaceId, even one from a stale mapping");
    assert!(matches!(err, FileSyncCommandError::InvalidConfig(_)));
}

/// Two distinct `haex_shared_space_sync` rows binding the same shared backend
/// to different spaces is an inconsistent state — the lookup must refuse to
/// pick one silently.
#[test]
fn shared_backend_bound_to_multiple_spaces_is_rejected() {
    let db = setup_db();
    let backend_id = fresh_id();
    let space_a = fresh_id();
    let space_b = fresh_id();
    seed_backend(&db, &backend_id, "shared_from_space");
    seed_share_mapping(&db, &backend_id, &space_a);
    seed_share_mapping(&db, &backend_id, &space_b);

    let err = verify_cloud_space_binding(&backend_id, Some(&space_a), &db)
        .expect_err("multi-space binding must be rejected outright");
    assert!(matches!(err, FileSyncCommandError::InvalidConfig(_)));
}
