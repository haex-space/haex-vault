//! Row-shape unit tests for `haex_s3_shared_access`.
//!
//! F1 stays deliberately small — the helpers write and read `JsonValue`
//! rows through `execute_with_crdt` / `select_with_crdt`, so once the
//! chokepoint contract is trusted (well-covered by the CRDT core tests),
//! what's left worth exercising in isolation is the row-shape
//! validation. The wire-up integration tests land in F2/F3 alongside
//! the callers.

use super::*;
use serde_json::json;

/// A well-formed row round-trips through `row_to_shared_access`.
#[test]
fn well_formed_row_parses_all_columns() {
    let row = vec![
        json!("id-1"),
        json!("space-alpha"),
        json!("backend-x"),
        json!("did:key:zAliceKey"),
        json!("<sealed-cred-base64>"),
        json!(42_u64),
        json!("2026-08-27T00:00:00Z"),
        json!("2026-08-27T00:00:01Z"),
    ];
    let parsed = row_to_shared_access(row).expect("well-formed row");
    assert_eq!(parsed.id, "id-1");
    assert_eq!(parsed.space_id, "space-alpha");
    assert_eq!(parsed.backend_id, "backend-x");
    assert_eq!(parsed.member_did, "did:key:zAliceKey");
    assert_eq!(parsed.encrypted_cred, "<sealed-cred-base64>");
    assert_eq!(parsed.epoch, 42);
    assert_eq!(parsed.expires_at.as_deref(), Some("2026-08-27T00:00:00Z"));
    assert_eq!(parsed.created_at, "2026-08-27T00:00:01Z");
}

/// A row with SQL NULL in `expires_at` decodes to `None` — this is the
/// no-STS-expiry case (long-lived scoped credential).
#[test]
fn null_expires_at_decodes_to_none() {
    let row = vec![
        json!("id-2"),
        json!("space-alpha"),
        json!("backend-x"),
        json!("did:key:zBobKey"),
        json!("<sealed>"),
        json!(1_u64),
        JsonValue::Null,
        json!("2026-08-27T00:00:00Z"),
    ];
    let parsed = row_to_shared_access(row).expect("null expires_at");
    assert!(parsed.expires_at.is_none());
}

/// Wrong number of columns is a hard error — surfaces a schema/query
/// drift rather than a silent partial parse.
#[test]
fn short_row_rejected() {
    let row = vec![json!("id"), json!("space"), json!("backend")];
    let err = row_to_shared_access(row).expect_err("short row must be rejected");
    let msg = format!("{err}");
    assert!(msg.contains("expected 8 columns"), "got: {err}");
}

/// A non-string in a text column is rejected — the SQL type shape and
/// our decoder must agree.
#[test]
fn non_string_text_column_rejected() {
    let row = vec![
        json!(123), // id — should be a string, is a number
        json!("space"),
        json!("backend"),
        json!("did"),
        json!("<sealed>"),
        json!(1_u64),
        JsonValue::Null,
        json!("2026-08-27T00:00:00Z"),
    ];
    let err = row_to_shared_access(row).expect_err("non-string id must be rejected");
    assert!(format!("{err}").contains("column `id`"), "got: {err}");
}

/// The `epoch` column must be an integer — non-integer JSON is a
/// column-type mismatch we want to surface early.
#[test]
fn non_integer_epoch_rejected() {
    let row = vec![
        json!("id"),
        json!("space"),
        json!("backend"),
        json!("did"),
        json!("<sealed>"),
        json!("not-a-number"),
        JsonValue::Null,
        json!("2026-08-27T00:00:00Z"),
    ];
    let err = row_to_shared_access(row).expect_err("non-integer epoch must be rejected");
    assert!(format!("{err}").contains("epoch"), "got: {err}");
}

/// A negative epoch is rejected at the row boundary — the upsert helper
/// takes `u64`, so a persisted or CRDT-provided `-1` must not decode as
/// a valid epoch and slip through.
#[test]
fn negative_epoch_rejected() {
    let row = vec![
        json!("id"),
        json!("space"),
        json!("backend"),
        json!("did"),
        json!("<sealed>"),
        json!(-1_i64),
        JsonValue::Null,
        json!("2026-08-27T00:00:00Z"),
    ];
    let err = row_to_shared_access(row).expect_err("negative epoch must be rejected");
    assert!(format!("{err}").contains("epoch"), "got: {err}");
}

/// A malformed (non-string, non-null) `expires_at` value is rejected —
/// SQL NULL or a string are the only valid states.
#[test]
fn non_string_non_null_expires_at_rejected() {
    let row = vec![
        json!("id"),
        json!("space"),
        json!("backend"),
        json!("did"),
        json!("<sealed>"),
        json!(1_u64),
        json!(42),
        json!("2026-08-27T00:00:00Z"),
    ];
    let err = row_to_shared_access(row).expect_err("integer expires_at must be rejected");
    assert!(format!("{err}").contains("expires_at"), "got: {err}");
}

// -------------------------------------------------------------------------
// Round F3b — `ScopedCred` seal/open helpers.
// -------------------------------------------------------------------------
//
// The row column tests above cover the storage seam.  These tests cover
// the sealing seam that Round F1 deferred: producing the base64
// `encrypted_cred` payload from a `ScopedCred` and recovering the
// credential on the receiver side.

#[test]
fn seal_open_roundtrip_returns_original_scoped_cred() {
    use crate::remote_storage::iam_adapter::ScopedCred;
    use crate::remote_storage::shared_access::crypto::{open_scoped_cred, seal_scoped_cred};

    let cred = ScopedCred {
        access_key_id: "AKIAEXAMPLE".into(),
        secret_access_key: "s3cret".into(),
        iam_user_name: "scoped-user".into(),
    };
    let key: [u8; 32] = rand::random();

    let sealed = seal_scoped_cred(&cred, &key, 1).expect("seal");
    let opened = open_scoped_cred(&sealed, &key).expect("open");

    assert_eq!(opened.access_key_id, cred.access_key_id);
    assert_eq!(opened.secret_access_key, cred.secret_access_key);
    assert_eq!(opened.iam_user_name, cred.iam_user_name);
}

#[test]
fn open_scoped_cred_rejects_wrong_key() {
    use crate::remote_storage::iam_adapter::ScopedCred;
    use crate::remote_storage::shared_access::crypto::{open_scoped_cred, seal_scoped_cred};

    let cred = ScopedCred {
        access_key_id: "AKIAEXAMPLE".into(),
        secret_access_key: "s3cret".into(),
        iam_user_name: "scoped-user".into(),
    };
    // Two independently drawn random keys — collision-safe at 32 bytes.
    let seal_key: [u8; 32] = rand::random();
    let wrong_key: [u8; 32] = rand::random();
    let sealed = seal_scoped_cred(&cred, &seal_key, 1).expect("seal");
    let err = open_scoped_cred(&sealed, &wrong_key).unwrap_err();
    assert!(matches!(err, super::SharedAccessError::Crypto { .. }));
}

#[test]
fn seal_scoped_cred_produces_distinct_ciphertexts_on_reseal() {
    use crate::remote_storage::iam_adapter::ScopedCred;
    use crate::remote_storage::shared_access::crypto::seal_scoped_cred;

    let cred = ScopedCred {
        access_key_id: "AKIAEXAMPLE".into(),
        secret_access_key: "s3cret".into(),
        iam_user_name: "scoped-user".into(),
    };
    let key: [u8; 32] = rand::random();
    let a = seal_scoped_cred(&cred, &key, 1).unwrap();
    let b = seal_scoped_cred(&cred, &key, 1).unwrap();
    assert_ne!(a, b, "nonces must be random; ciphertexts must differ");
}

/// Drift guard: constructs a [`ScopedCred`] via exhaustive struct-init
/// (no `..Default::default()`) and asserts round-trip via exhaustive
/// destructure on the opened value. If a field is added to `ScopedCred`,
/// this test fails to compile — forcing the wire mirror in `crypto.rs`
/// and this assertion set to be updated alongside.
#[test]
fn scoped_cred_wire_roundtrip_covers_every_field() {
    use crate::remote_storage::iam_adapter::ScopedCred;
    use crate::remote_storage::shared_access::crypto::{open_scoped_cred, seal_scoped_cred};

    let cred = ScopedCred {
        access_key_id: "AKIAROUNDTRIP".into(),
        secret_access_key: "s3cret-roundtrip".into(),
        iam_user_name: "roundtrip-user".into(),
    };
    let key: [u8; 32] = rand::random();
    let sealed = seal_scoped_cred(&cred, &key, 42).expect("seal");
    let opened = open_scoped_cred(&sealed, &key).expect("open");

    // Exhaustive destructure — a new field failing to round-trip surfaces
    // here rather than in an eq-derive.
    let ScopedCred {
        access_key_id,
        secret_access_key,
        iam_user_name,
    } = opened;
    assert_eq!(access_key_id, cred.access_key_id);
    assert_eq!(secret_access_key, cred.secret_access_key);
    assert_eq!(iam_user_name, cred.iam_user_name);
}

// -------------------------------------------------------------------------
// Round F3b Task 2 — owner-side `upsert_sealed_scoped_cred` wire-up.
// -------------------------------------------------------------------------

/// The `upsert_sealed_scoped_cred` helper writes a row whose
/// `encrypted_cred` opens with the same key that produced it, and whose
/// `epoch` column matches the seal epoch. The whole point of the combined
/// helper is that callers pass one `epoch` — this test proves that value
/// really threads into both the row column AND the sealed envelope
/// header.
#[test]
fn upsert_sealed_scoped_cred_persists_row_with_ciphertext_and_matching_epoch() {
    use crate::remote_storage::iam_adapter::ScopedCred;
    use crate::remote_storage::shared_access::crypto::open_scoped_cred;
    use crate::remote_storage::shared_access::{find_shared_access, upsert_sealed_scoped_cred};

    let (db, hlc, key_cache) = crdt_bootstrap::setup_test_db();
    let hlc_mutex = std::sync::Mutex::new(hlc);
    let hlc_guard = hlc_mutex.lock().unwrap();

    let cred = ScopedCred {
        access_key_id: "AKIATEST".into(),
        secret_access_key: "s3cret".into(),
        iam_user_name: "scoped-user".into(),
    };
    let epoch_key: [u8; 32] = rand::random();
    let epoch: u64 = 42;
    let row_id = uuid::Uuid::new_v4().to_string();

    upsert_sealed_scoped_cred(
        &db,
        &hlc_guard,
        &key_cache,
        &row_id,
        "space-A",
        "backend-1",
        "did:key:zRecipient",
        &cred,
        epoch,
        &epoch_key,
        None,
    )
    .expect("upsert");

    let row = find_shared_access(&db, "space-A", "backend-1", "did:key:zRecipient")
        .expect("find")
        .expect("row exists");
    assert_eq!(row.epoch, epoch, "row.epoch matches the seal epoch");
    assert!(!row.encrypted_cred.is_empty(), "ciphertext written");

    // Independent round-trip: the row must open under the same key that
    // produced it, proving the seal really used `epoch_key` (not a stale
    // derivation).
    let opened = open_scoped_cred(&row.encrypted_cred, &epoch_key).expect("open");
    assert_eq!(opened.access_key_id, cred.access_key_id);
    assert_eq!(opened.secret_access_key, cred.secret_access_key);
    assert_eq!(opened.iam_user_name, cred.iam_user_name);

    // Envelope-header epoch invariant: `open_scoped_cred` deliberately
    // discards the header (the row column is authoritative for key
    // lookup on open), so a buggy helper that forwarded `epoch` to the
    // row column while hardcoding `0` inside `seal_scoped_cred` would
    // pass the round-trip above. Parse the header directly so a
    // seal-side drift surfaces here rather than at some future audit.
    use crate::file_sync::crypto::content::open_bytes;
    use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
    let sealed_bytes = B64
        .decode(row.encrypted_cred.as_bytes())
        .expect("base64 decode");
    let (header, _plaintext) = open_bytes(&epoch_key, &sealed_bytes).expect("open_bytes");
    assert_eq!(
        header.epoch, epoch,
        "seal path must use the same epoch that lands in the row column"
    );
}

/// CRDT-enabled in-memory DB bootstrap for `haex_s3_shared_access`.
/// Collocated with the module it exercises (option A of the Task 2 brief).
/// Mirrors the pattern in `extension/spaces/tests.rs::setup_test_db` —
/// mounts the minimum CRDT chokepoint scaffolding (UDFs, dirty/deleted-row
/// tables, triggers on `haex_s3_shared_access`) plus a seeded `own`
/// identity for `space-A` so `SpaceKeyCache::populate_all` finds a signing
/// key. Row-shape tests above stay pure JSON — the bootstrap here is only
/// paid by tests that actually exercise `execute_with_crdt`.
mod crdt_bootstrap {
    use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
    use rusqlite::functions::FunctionFlags;
    use rusqlite::Connection;
    use std::sync::{Arc, Mutex};
    use uuid::Uuid;

    use crate::crdt::column_sig::key_cache::SpaceKeyCache;
    use crate::crdt::hlc::HlcService;
    use crate::crdt::trigger::{
        ensure_crdt_columns, setup_triggers_for_table, DELETED_ROWS_TABLE, UUID_FUNCTION_NAME,
    };
    use crate::database::connection_context::ConnectionContext;
    use crate::database::core::{install_tx_hlc_hooks, register_current_hlc_udf};
    use crate::database::DbConnection;
    use crate::table_names::{TABLE_CRDT_CONFIGS, TABLE_CRDT_DIRTY_TABLES, TABLE_S3_SHARED_ACCESS};

    pub(super) fn setup_test_db() -> (DbConnection, HlcService, SpaceKeyCache) {
        let conn = Connection::open_in_memory().expect("in-memory DB");

        // UDFs + tx-HLC hooks so triggers can emit into haex_deleted_rows.
        conn.create_scalar_function(
            UUID_FUNCTION_NAME,
            0,
            FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_INNOCUOUS,
            |_ctx| Ok(Uuid::new_v4().to_string()),
        )
        .unwrap();
        let hlc = HlcService::new_for_testing("test-device-f3b");
        let ctx = ConnectionContext::new();
        register_current_hlc_udf(&conn, hlc.clone(), ctx.clone()).unwrap();
        install_tx_hlc_hooks(&conn, ctx).unwrap();

        // CRDT chokepoint tables.
        conn.execute_batch(&format!(
            "CREATE TABLE {TABLE_CRDT_CONFIGS} (key TEXT PRIMARY KEY, type TEXT NOT NULL, value TEXT NOT NULL)"
        ))
        .unwrap();
        conn.execute(
            &format!(
                "INSERT INTO {TABLE_CRDT_CONFIGS} (key, type, value) VALUES ('triggers_enabled', 'system', '1')"
            ),
            [],
        )
        .unwrap();
        conn.execute_batch(&format!(
            "CREATE TABLE {TABLE_CRDT_DIRTY_TABLES} (table_name TEXT PRIMARY KEY, last_modified TEXT)"
        ))
        .unwrap();
        conn.execute_batch(&format!(
            "CREATE TABLE {DELETED_ROWS_TABLE} (
                id TEXT PRIMARY KEY NOT NULL,
                table_name TEXT NOT NULL,
                row_pks TEXT NOT NULL,
                haex_hlc TEXT,
                haex_column_hlcs TEXT NOT NULL DEFAULT '{{}}'
            )"
        ))
        .unwrap();

        // haex_s3_shared_access — schema mirrors migration 0020 so the SQL
        // in `mod.rs` binds against the real column layout.
        conn.execute_batch(&format!(
            "CREATE TABLE {TABLE_S3_SHARED_ACCESS} (
                id TEXT PRIMARY KEY NOT NULL,
                space_id TEXT NOT NULL,
                backend_id TEXT NOT NULL,
                member_did TEXT NOT NULL,
                encrypted_cred TEXT NOT NULL,
                epoch INTEGER NOT NULL,
                expires_at TEXT,
                created_at TEXT DEFAULT (CURRENT_TIMESTAMP) NOT NULL
            )"
        ))
        .unwrap();
        conn.execute_batch(&format!(
            "CREATE UNIQUE INDEX haex_s3_shared_access_space_backend_did_uniq \
             ON {TABLE_S3_SHARED_ACCESS} (space_id, backend_id, member_did)"
        ))
        .unwrap();

        // Per-space delete-log (register-DELETE fanout trigger writes here).
        conn.execute_batch(
            "CREATE TABLE haex_shared_space_deleted_rows (
                id TEXT PRIMARY KEY NOT NULL,
                space_id TEXT NOT NULL,
                table_name TEXT NOT NULL,
                row_pks TEXT NOT NULL,
                haex_hlc TEXT,
                haex_column_hlcs TEXT NOT NULL DEFAULT '{}',
                haex_column_sigs TEXT NOT NULL DEFAULT '{}'
            )",
        )
        .unwrap();

        // Identity + membership so SpaceKeyCache picks up a signing key
        // for space-A. Column signing runs on every execute_with_crdt on a
        // space-scoped table (haex_s3_shared_access IS space-scoped, see
        // scanner.rs::SPACE_SCOPED_CRDT_TABLES).
        conn.execute_batch(
            "CREATE TABLE haex_identities (
                id TEXT PRIMARY KEY NOT NULL,
                did TEXT NOT NULL,
                name TEXT NOT NULL,
                source TEXT NOT NULL DEFAULT 'contact',
                private_key TEXT
            );
            CREATE TABLE haex_space_members (
                id TEXT PRIMARY KEY NOT NULL,
                space_id TEXT NOT NULL,
                identity_id TEXT NOT NULL
            )",
        )
        .unwrap();

        {
            let tx = conn.unchecked_transaction().unwrap();
            ensure_crdt_columns(&tx, TABLE_S3_SHARED_ACCESS).unwrap();
            setup_triggers_for_table(&tx, TABLE_S3_SHARED_ACCESS, false).unwrap();
            tx.commit().unwrap();
        }

        // Well-formed PKCS8 Ed25519 blob (16-byte prefix + 32-byte seed).
        // Random seed keeps the fixture out of CodeQL's hardcoded-credential net.
        let pkcs8_prefix: [u8; 16] = [
            0x30, 0x2e, 0x02, 0x01, 0x00, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x04, 0x22,
            0x04, 0x20,
        ];
        let seed: [u8; 32] = rand::random();
        let mut der = Vec::with_capacity(48);
        der.extend_from_slice(&pkcs8_prefix);
        der.extend_from_slice(&seed);
        let pkcs8_b64 = BASE64.encode(&der);

        conn.execute(
            "INSERT INTO haex_identities (id, did, name, source, private_key) \
             VALUES ('id-owner', 'did:key:zSpaceAOwner', 'Owner', 'own', ?1)",
            [pkcs8_b64],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO haex_space_members (id, space_id, identity_id) \
             VALUES ('mem-owner', 'space-A', 'id-owner')",
            [],
        )
        .unwrap();

        let cache = SpaceKeyCache::new();
        cache.populate_all(&conn).expect("populate cache");

        let db = DbConnection(Arc::new(Mutex::new(Some(conn))));
        (db, hlc, cache)
    }
}
