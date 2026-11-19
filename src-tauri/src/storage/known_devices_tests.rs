//! Tests for `known_devices::list_devices` (spec 023-settings-app, FR-022, research R12). They
//! open a real vault, so the bootstrap rows (this installation and the vault-scope row) exist as
//! in the app.

use std::sync::Arc;

use haex_crdt::rusqlite::params;
use haex_crdt::{Database, DatabaseConfig, NoopSignatureProvider, SqlCipherKey};
use uuid::Uuid;

use super::known_devices::{self, KnownDevice};
use crate::identity::{
    haex_migration_source, installation_id_path, read_or_mint_installation_uuid, HaexBootstrap,
    HAEX_TRIGGER_VERSION, VAULT_SCOPE_UUID,
};
use crate::storage::query;

fn open_test_vault() -> (tempfile::TempDir, Database, Uuid) {
    let dir = tempfile::tempdir().expect("tempdir");
    let install_path = installation_id_path(dir.path());
    let db = Database::open(DatabaseConfig {
        path: dir.path().join("vault.db"),
        key: SqlCipherKey::new("known-devices-test"),
        create_if_missing: true,
        bootstrap: Arc::new(HaexBootstrap::new(install_path.clone())),
        signature_provider: Arc::new(NoopSignatureProvider),
        migration_source: haex_migration_source(),
        trigger_version: HAEX_TRIGGER_VERSION,
        max_transaction_bytes: haex_crdt::MAX_CRDT_TRANSACTION_BYTES,
        max_value_bytes: haex_crdt::MAX_VALUE_BYTES,
    })
    .expect("open test vault");
    let installation_uuid =
        read_or_mint_installation_uuid(&install_path).expect("installation uuid");
    (dir, db, installation_uuid)
}

fn list(db: &Database) -> Vec<KnownDevice> {
    query::read(db, |r| known_devices::list_devices(r)).expect("list devices")
}

#[test]
fn lists_this_installation_without_the_vault_scope_row() {
    let (_dir, db, installation_uuid) = open_test_vault();

    let devices = list(&db);

    assert_eq!(devices.len(), 1, "{devices:?}");
    assert_eq!(devices[0].installation_uuid, installation_uuid);
    assert_ne!(devices[0].vault_device_uuid, VAULT_SCOPE_UUID);
    assert!(devices
        .iter()
        .all(|d| d.installation_uuid != VAULT_SCOPE_UUID));
}

#[test]
fn lists_another_device_and_its_missing_name() {
    let (_dir, db, installation_uuid) = open_test_vault();
    db.write(|tx| {
        known_devices::update_alias(tx, installation_uuid, "Laptop")?;
        tx.execute(
            "INSERT INTO known_devices (installation_uuid, vault_device_uuid, alias, first_seen) \
             VALUES (?1, ?2, NULL, 1)",
            params![Uuid::new_v4().to_string(), Uuid::new_v4().to_string()],
        )?;
        Ok(())
    })
    .expect("seed devices");

    let devices = list(&db);

    assert_eq!(devices.len(), 2, "{devices:?}");
    let this = devices
        .iter()
        .find(|d| d.installation_uuid == installation_uuid)
        .expect("this installation");
    assert_eq!(this.alias.as_deref(), Some("Laptop"));
    let other = devices
        .iter()
        .find(|d| d.installation_uuid != installation_uuid)
        .expect("the other device");
    assert_eq!(other.alias, None);
}
