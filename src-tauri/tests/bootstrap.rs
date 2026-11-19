//! Etappe 1 acceptance test: `HaexBootstrap` at the pinned haex-crdt 0.4.0
//! revision produces a stable identity across genesis → close → reopen, and
//! the `installation_uuid` PK is carried in `row_pks` (never as a mutable
//! column change).
//!
//! Mirrors the Etappe-0 wegwerf-crate assertions, but exercises the real
//! production code in `holzi_lib::identity::HaexBootstrap` + `MigrationSource`,
//! not a spike.

// These tests read raw vault state (counts, CRDT columns, the delete log) that
// the CRDT write path does not expose.
#![allow(clippy::disallowed_methods)]

use std::path::PathBuf;
use std::sync::{Arc, Barrier};
use std::thread;

use haex_crdt::crdt::columns::HLC_TIMESTAMP_COLUMN;
use haex_crdt::rusqlite::params;
use haex_crdt::{Database, DatabaseConfig, NoopSignatureProvider, ScanFilters, SqlCipherKey};

use holzi_lib::identity::{
    haex_migration_source, installation_id_path, HaexBootstrap, HAEX_TRIGGER_VERSION,
    VAULT_SCOPE_UUID,
};
use holzi_lib::storage::known_devices;

const PASSPHRASE: &str = "etappe1-integration-test";

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

/// Number of real (non-sentinel) `known_devices` rows.
fn count_real_known_devices(db: &Database) -> i64 {
    db.with_connection(|c| {
        c.query_row(
            "SELECT COUNT(*) FROM known_devices WHERE vault_device_uuid != ?1",
            params![VAULT_SCOPE_UUID.to_string()],
            |r| r.get::<_, i64>(0),
        )
        .map_err(Into::into)
    })
    .expect("count known_devices")
}

#[test]
/// Verifies that reopening a vault preserves its installation and device identities.
fn bootstrap_genesis_and_reopen_reuse_uuid() {
    let tmp = tempfile::tempdir().expect("tmp dir");
    let db_path = tmp.path().join("vault.db");
    let installation_id_file = installation_id_path(tmp.path());

    // ---- Genesis: fresh DB, bootstrap mints installation-id file, vault
    //      identity, and known_devices row.
    let db = Database::open(make_config(
        db_path.clone(),
        installation_id_file.clone(),
        true,
    ))
    .expect("genesis open");
    let vault_device_uuid_genesis = db.device_id();

    assert!(
        installation_id_file.exists(),
        "bootstrap must mint <AppLocalData>/installation-id"
    );

    // The bootstrap no longer mints an identity: it has no HLC yet. The vault
    // identity is published after open, with its first HLC (spec 024, T026).
    let ident_count: i64 = db
        .with_connection(|c| {
            c.query_row("SELECT COUNT(*) FROM vault_identity", [], |r| r.get(0))
                .map_err(Into::into)
        })
        .expect("vault_identity count");
    assert_eq!(
        ident_count, 0,
        "the bootstrap leaves vault_identity to the sync setup"
    );
    let installation =
        holzi_lib::identity::read_or_mint_installation_uuid(&installation_id_file).expect("id");
    let state =
        holzi_lib::sync::genesis::ensure_sync_state(&db, installation, true).expect("sync state");
    assert!(state.vault_pubkey.is_some() && state.is_main);
    let ident_count: i64 = db
        .with_connection(|c| {
            c.query_row(
                "SELECT COUNT(*) FROM vault_identity WHERE haex_hlc_no_sync IS NOT NULL",
                [],
                |r| r.get(0),
            )
            .map_err(Into::into)
        })
        .expect("vault_identity count");
    assert_eq!(ident_count, 1, "the published identity carries an HLC");

    // known_devices row for this installation present after genesis
    // (the sentinel row is filtered out — see spec 002 §"Vault Scope Sentinel").
    assert_eq!(
        count_real_known_devices(&db),
        1,
        "genesis must insert one real known_devices row (sentinel excluded)"
    );

    drop(db);

    // ---- Reopen: same installation-id file, no create. Bootstrap must find
    //      and reuse the existing row, returning the same vault_device_uuid.
    let db2 = Database::open(make_config(
        db_path.clone(),
        installation_id_file.clone(),
        false,
    ))
    .expect("reopen");
    let vault_device_uuid_reopen = db2.device_id();

    assert_eq!(
        vault_device_uuid_genesis, vault_device_uuid_reopen,
        "reopen must reuse the vault_device_uuid from genesis"
    );

    assert_eq!(
        count_real_known_devices(&db2),
        1,
        "reopen must not insert a second real known_devices row"
    );
}

#[test]
/// Verifies that runtime updates expose the immutable primary key through `row_pks`.
fn known_devices_row_syncs_via_row_pks() {
    let tmp = tempfile::tempdir().expect("tmp dir");
    let db_path = tmp.path().join("vault.db");
    let installation_id_file = installation_id_path(tmp.path());

    let db = Database::open(make_config(
        db_path.clone(),
        installation_id_file.clone(),
        true,
    ))
    .expect("genesis open");

    // Bootstrap-inserted row is sync-invisible until an UPDATE fires the
    // trigger (HLC not initialised inside the bootstrap transaction — see
    // contract §"Vault identity and device model" and Etappe-0 finding).
    let installation_uuid =
        holzi_lib::identity::read_or_mint_installation_uuid(&installation_id_file)
            .expect("read installation id");

    db.write(|tx| known_devices::update_alias(tx, installation_uuid, "renamed"))
        .expect("alias update");

    let changes = db
        .scan_table_for_local_changes("known_devices", None, ScanFilters::default())
        .expect("scan known_devices");
    assert!(
        !changes.is_empty(),
        "post-UPDATE scan must surface at least one column change"
    );

    // The scanner emits column changes; the PK (installation_uuid) rides in
    // `row_pks`, not as a `column_name`. Assert both properties.
    let scanned_columns: std::collections::BTreeSet<&str> =
        changes.iter().map(|c| c.column_name.as_str()).collect();
    assert!(
        scanned_columns.contains("alias"),
        "alias must appear as a scanned column change; got {:?}",
        scanned_columns
    );
    assert!(
        !scanned_columns.contains("installation_uuid"),
        "installation_uuid is a PK — it must ride in row_pks, not as a column change; got {:?}",
        scanned_columns
    );

    // Sanity: the CRDT-injecting update actually populated the row HLC.
    let hlc_after: Option<String> = db
        .with_connection(|c| {
            c.query_row(
                &format!(
                    "SELECT {HLC_TIMESTAMP_COLUMN} FROM known_devices \
                     WHERE installation_uuid = ?1"
                ),
                params![installation_uuid.to_string()],
                |r| r.get(0),
            )
            .map_err(Into::into)
        })
        .ok();
    assert!(
        hlc_after.is_some(),
        "row-level HLC must be populated after a CRDT-injecting UPDATE"
    );
}

#[test]
fn concurrent_installation_uuid_mint_uses_first_writer() {
    let tmp = tempfile::tempdir().expect("tmp dir");
    let installation_id_file = installation_id_path(tmp.path());
    let start = Arc::new(Barrier::new(16));

    let handles: Vec<_> = (0..16)
        .map(|_| {
            let start = Arc::clone(&start);
            let path = installation_id_file.clone();
            thread::spawn(move || {
                start.wait();
                holzi_lib::identity::read_or_mint_installation_uuid(&path)
            })
        })
        .collect();

    let uuids: Vec<_> = handles
        .into_iter()
        .map(|handle| {
            handle
                .join()
                .expect("mint thread")
                .expect("installation id")
        })
        .collect();

    assert!(uuids.iter().all(|uuid| *uuid == uuids[0]));
    assert_eq!(
        holzi_lib::identity::read_or_mint_installation_uuid(&installation_id_file)
            .expect("persisted installation id"),
        uuids[0]
    );
}
