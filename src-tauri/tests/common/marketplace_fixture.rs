//! A real vault for the tests of the marketplace store and its commands (spec 069): opened with the
//! holzi migrations, so `haex_marketplaces` is the real table with its CRDT triggers.

use std::sync::Arc;

use haex_crdt::{Database, DatabaseConfig, NoopSignatureProvider, SqlCipherKey};

use haex_lib::identity::{
    haex_migration_source, installation_id_path, HaexBootstrap, HAEX_TRIGGER_VERSION,
};
use haex_lib::vault_gate::{VaultDb, VaultGate};

pub struct TestVault {
    pub _dir: tempfile::TempDir,
    pub db: VaultDb,
}

/// A fresh vault. Nothing seeds a marketplace here: the store functions are tested on their own.
pub fn vault() -> TestVault {
    let dir = tempfile::tempdir().expect("tempdir");
    let database = Database::open(DatabaseConfig {
        path: dir.path().join("vault.db"),
        key: SqlCipherKey::new("marketplace"),
        create_if_missing: true,
        bootstrap: Arc::new(HaexBootstrap::new(installation_id_path(dir.path()))),
        signature_provider: Arc::new(NoopSignatureProvider),
        migration_source: haex_migration_source(),
        trigger_version: HAEX_TRIGGER_VERSION,
        max_transaction_bytes: haex_crdt::MAX_CRDT_TRANSACTION_BYTES,
        max_value_bytes: haex_crdt::MAX_VALUE_BYTES,
    })
    .expect("open the vault");
    let db = VaultGate::new()
        .vault_db(Arc::new(database))
        .expect("open the gate");
    TestVault { _dir: dir, db }
}
