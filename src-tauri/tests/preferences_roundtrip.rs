//! Integration coverage for the `preferences` table introduced by
//! spec 002 (`docs/adr/0001-device-scoped-data-convention.md`).
//!
//! Covers the storage-wrapper roundtrip, the vault-scope sentinel row
//! (idempotent bootstrap insertion, sentinel-uses-nil-UUID) and the
//! ON-DELETE-CASCADE from `known_devices` down into `preferences` — the
//! forward-looking contract in US5 that guarantees a later Retire
//! operation cleans up per-device preferences without touching this
//! module's code.
//!
//! Sync-apply order (child-before-parent and parent-before-child of
//! `preferences → known_devices`) is intentionally NOT re-tested here;
//! that invariant is a haex-crdt property covered by
//! `apply/tests/policy/insert_constraint.rs` upstream and by the
//! `foreign_key_check` pass haex-crdt runs before commit. Adding a
//! duplicate here would only re-exercise the crate we depend on.

// These tests read raw vault state (counts, CRDT columns, the schema) that the
// CRDT write path does not expose.
#![allow(clippy::disallowed_methods)]

use std::path::PathBuf;
use std::sync::Arc;

use haex_crdt::crdt::columns::HLC_TIMESTAMP_COLUMN;
use haex_crdt::rusqlite::params;
use haex_crdt::{Database, DatabaseConfig, NoopSignatureProvider, SqlCipherKey};
use uuid::Uuid;

use holzi_lib::identity::{
    haex_migration_source, installation_id_path, HaexBootstrap, HAEX_TRIGGER_VERSION,
    VAULT_SCOPE_UUID,
};
use holzi_lib::storage::preferences::{self, PrefScope};
use holzi_lib::storage::query;

const PASSPHRASE: &str = "preferences-roundtrip";

/// Builds a database configuration using the production bootstrap providers.
fn make_config(
    db_path: PathBuf,
    installation_id: PathBuf,
    create_if_missing: bool,
) -> DatabaseConfig {
    DatabaseConfig {
        path: db_path,
        key: SqlCipherKey::new(PASSPHRASE),
        create_if_missing,
        bootstrap: Arc::new(HaexBootstrap::new(installation_id).with_alias("test")),
        signature_provider: Arc::new(NoopSignatureProvider),
        migration_source: haex_migration_source(),
        trigger_version: HAEX_TRIGGER_VERSION,
        max_transaction_bytes: haex_crdt::MAX_CRDT_TRANSACTION_BYTES,
        max_value_bytes: haex_crdt::MAX_VALUE_BYTES,
    }
}

/// Inserts an additional (non-sentinel, non-bootstrap) `known_devices`
/// row so FK-cascade tests can delete it without touching the row the
/// bootstrap wrote for the current installation.
fn insert_extra_known_device(db: &Database, alias: &str) -> Uuid {
    let vault_device_uuid = Uuid::new_v4();
    let installation_uuid = Uuid::new_v4();
    db.with_connection(|conn| {
        // Use the storage HLC convention: real device rows fire the
        // sync trigger via `current_hlc()`.
        let sql = format!(
            "INSERT INTO known_devices \
               (installation_uuid, vault_device_uuid, alias, first_seen, {HLC_TIMESTAMP_COLUMN}) \
             VALUES (?1, ?2, ?3, 0, current_hlc())"
        );
        conn.execute(
            &sql,
            params![
                installation_uuid.to_string(),
                vault_device_uuid.to_string(),
                alias,
            ],
        )
        .map_err(Into::into)
    })
    .expect("insert extra known_devices row");
    vault_device_uuid
}

/// Counts the sentinel rows in `known_devices`. Should always be
/// exactly one after any bootstrap has run — repeated bootstraps must
/// stay idempotent via `INSERT OR IGNORE`.
fn count_sentinel_rows(db: &Database) -> i64 {
    db.with_connection(|conn| {
        conn.query_row(
            "SELECT COUNT(*) FROM known_devices WHERE vault_device_uuid = ?1",
            params![VAULT_SCOPE_UUID.to_string()],
            |r| r.get::<_, i64>(0),
        )
        .map_err(Into::into)
    })
    .expect("count sentinel rows")
}

#[test]
/// Two bootstraps on the same vault yield exactly one sentinel row.
fn bootstrap_sentinel_is_idempotent() {
    let tmp = tempfile::tempdir().expect("tmp dir");
    let db_path = tmp.path().join("vault.db");
    let installation_id_file = installation_id_path(tmp.path());

    let db = Database::open(make_config(
        db_path.clone(),
        installation_id_file.clone(),
        true,
    ))
    .expect("genesis open");
    assert_eq!(
        count_sentinel_rows(&db),
        1,
        "genesis bootstrap must insert exactly one sentinel"
    );
    drop(db);

    let db2 = Database::open(make_config(
        db_path.clone(),
        installation_id_file.clone(),
        false,
    ))
    .expect("reopen");
    assert_eq!(
        count_sentinel_rows(&db2),
        1,
        "reopen bootstrap must not duplicate the sentinel"
    );
}

#[test]
/// The sentinel row uses the nil UUID for both installation and device.
fn sentinel_row_uses_nil_uuid_for_both_columns() {
    let tmp = tempfile::tempdir().expect("tmp dir");
    let db_path = tmp.path().join("vault.db");
    let installation_id_file = installation_id_path(tmp.path());
    let db = Database::open(make_config(db_path, installation_id_file, true)).expect("genesis");

    let (installation, alias, first_seen): (String, Option<String>, i64) = db
        .with_connection(|conn| {
            conn.query_row(
                "SELECT installation_uuid, alias, first_seen FROM known_devices \
                 WHERE vault_device_uuid = ?1",
                params![VAULT_SCOPE_UUID.to_string()],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .map_err(Into::into)
        })
        .expect("read sentinel");
    assert_eq!(installation, VAULT_SCOPE_UUID.to_string());
    assert!(alias.is_none());
    assert_eq!(first_seen, 0);
}

#[test]
/// Preferences round-trip: set → get, update overwrites, delete
/// idempotently, and get returns None for unknown keys.
fn preferences_set_get_update_delete_roundtrip() {
    let tmp = tempfile::tempdir().expect("tmp dir");
    let db_path = tmp.path().join("vault.db");
    let installation_id_file = installation_id_path(tmp.path());
    let db = Database::open(make_config(db_path, installation_id_file, true)).expect("genesis");

    let scope = PrefScope::Device(db.device_id());
    let key = "chat.default_model_id";

    // Insert.
    db.write(|tx| {
        assert!(preferences::get(tx, scope, key).unwrap().is_none());
        preferences::insert_or_update(tx, scope, key, "model-A").unwrap();
        Ok(())
    })
    .unwrap();
    query::read(&db, |r| {
        assert_eq!(
            preferences::get(r, scope, key).unwrap().as_deref(),
            Some("model-A")
        );
        Ok(())
    })
    .unwrap();

    // Update to a new value.
    db.write(|tx| {
        preferences::insert_or_update(tx, scope, key, "model-B").unwrap();
        Ok(())
    })
    .unwrap();
    query::read(&db, |r| {
        assert_eq!(
            preferences::get(r, scope, key).unwrap().as_deref(),
            Some("model-B")
        );
        Ok(())
    })
    .unwrap();

    // Delete, then delete again (idempotent).
    db.write(|tx| {
        preferences::delete(tx, scope, key).unwrap();
        preferences::delete(tx, scope, key).unwrap();
        assert!(preferences::get(tx, scope, key).unwrap().is_none());
        Ok(())
    })
    .unwrap();
}

#[test]
/// Device- and vault-scoped rows for the same key are isolated: a
/// device read does not see the vault row, and vice-versa.
fn scope_isolation_between_device_and_vault_rows() {
    let tmp = tempfile::tempdir().expect("tmp dir");
    let db_path = tmp.path().join("vault.db");
    let installation_id_file = installation_id_path(tmp.path());
    let db = Database::open(make_config(db_path, installation_id_file, true)).expect("genesis");

    let device_scope = PrefScope::Device(db.device_id());
    let vault_scope = PrefScope::Vault;
    let key = "chat.default_model_id";

    db.write(|tx| {
        preferences::insert_or_update(tx, device_scope, key, "device-model").unwrap();
        preferences::insert_or_update(tx, vault_scope, key, "vault-model").unwrap();

        assert_eq!(
            preferences::get(tx, device_scope, key).unwrap().as_deref(),
            Some("device-model")
        );
        assert_eq!(
            preferences::get(tx, vault_scope, key).unwrap().as_deref(),
            Some("vault-model")
        );

        // Deleting the vault row leaves the device row untouched.
        preferences::delete(tx, vault_scope, key).unwrap();
        assert!(preferences::get(tx, vault_scope, key).unwrap().is_none());
        assert_eq!(
            preferences::get(tx, device_scope, key).unwrap().as_deref(),
            Some("device-model")
        );
        Ok(())
    })
    .unwrap();
}

#[test]
/// A SQL-NULL value collapses to `None` on read — the wrapper does not
/// surface it as an empty string or a distinct present-but-null state.
fn null_stored_value_folds_to_absent_on_get() {
    let tmp = tempfile::tempdir().expect("tmp dir");
    let db_path = tmp.path().join("vault.db");
    let installation_id_file = installation_id_path(tmp.path());
    let db = Database::open(make_config(db_path, installation_id_file, true)).expect("genesis");

    let scope = PrefScope::Device(db.device_id());
    let key = "chat.default_model_id";

    db.write(|tx| {
        tx.execute(
            "INSERT INTO preferences (vault_device_uuid, key, value) VALUES (?1, ?2, NULL)",
            params![scope.to_uuid().to_string(), key],
        )
        .unwrap();

        assert!(preferences::get(tx, scope, key).unwrap().is_none());
        Ok(())
    })
    .unwrap();
}

#[test]
/// US5 forward-looking contract (spec 002 §FR-017): deleting a
/// `known_devices` row cascades into `preferences`, every row scoped
/// to that device disappears, and other devices' rows plus the vault-
/// wide row stay intact. Formalises T060 — a later Retire operation
/// can just `DELETE FROM known_devices` and the FK cascade handles the
/// per-device preferences without the Retire code knowing this module.
fn deleting_known_device_cascades_into_preferences() {
    let tmp = tempfile::tempdir().expect("tmp dir");
    let db_path = tmp.path().join("vault.db");
    let installation_id_file = installation_id_path(tmp.path());
    let db = Database::open(make_config(db_path, installation_id_file, true)).expect("genesis");

    let extra_device = insert_extra_known_device(&db, "second-device");
    let extra_scope = PrefScope::Device(extra_device);
    let own_scope = PrefScope::Device(db.device_id());
    let vault_scope = PrefScope::Vault;

    db.write(|tx| {
        preferences::insert_or_update(tx, extra_scope, "chat.default_model_id", "extra-A").unwrap();
        preferences::insert_or_update(tx, extra_scope, "chat.last_active_model_id", "extra-B")
            .unwrap();
        preferences::insert_or_update(tx, own_scope, "chat.default_model_id", "own-A").unwrap();
        preferences::insert_or_update(tx, vault_scope, "chat.default_model_id", "vault-A").unwrap();

        let before: i64 = tx
            .query_row(
                "SELECT COUNT(*) FROM preferences WHERE vault_device_uuid = ?1",
                params![extra_device.to_string()],
                |r| r.get(0),
            )
            .unwrap()
            .unwrap_or(0);
        assert_eq!(
            before, 2,
            "extra device must have 2 preferences before delete"
        );

        tx.execute(
            "DELETE FROM known_devices WHERE vault_device_uuid = ?1",
            params![extra_device.to_string()],
        )
        .unwrap();

        let after: i64 = tx
            .query_row(
                "SELECT COUNT(*) FROM preferences WHERE vault_device_uuid = ?1",
                params![extra_device.to_string()],
                |r| r.get(0),
            )
            .unwrap()
            .unwrap_or(0);
        assert_eq!(
            after, 0,
            "FK cascade must drop the extra device's preferences"
        );

        // Other scopes untouched.
        assert_eq!(
            preferences::get(tx, own_scope, "chat.default_model_id")
                .unwrap()
                .as_deref(),
            Some("own-A"),
            "own device preference must survive the extra-device delete"
        );
        assert_eq!(
            preferences::get(tx, vault_scope, "chat.default_model_id")
                .unwrap()
                .as_deref(),
            Some("vault-A"),
            "vault-wide preference must survive the extra-device delete"
        );
        Ok(())
    })
    .unwrap();
}

/// Compile-time guard: bumping the trigger version is the ONLY way a
/// new CRDT-tracked table (like `preferences`) gets its triggers
/// installed on already-provisioned vaults (see identity/migrations.rs
/// doc). Regressing this below 4 silently strips those triggers on
/// reopen — the const-assert catches that at build time.
const _: () = assert!(HAEX_TRIGGER_VERSION >= 4);
