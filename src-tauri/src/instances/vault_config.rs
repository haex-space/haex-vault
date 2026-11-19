//! The single `DatabaseConfig` every vault open goes through.
//!
//! `create_instance` and `open_instance` differ only in
//! `create_if_missing`; everything else — migration set, bootstrap,
//! signature provider and above all the CRDT trigger version — must be
//! identical, or the two paths provision subtly different vaults. Having
//! two hand-written copies is how `trigger_version` drifts, so there is
//! exactly one copy here.

use std::path::Path;
use std::sync::Arc;

use haex_crdt::{DatabaseConfig, NoopSignatureProvider, SqlCipherKey, MAX_CRDT_TRANSACTION_BYTES};

use crate::identity::{haex_migration_source, HaexBootstrap, HAEX_TRIGGER_VERSION};

/// Builds the vault `DatabaseConfig` shared by create and open.
///
/// `create_if_missing` is the only knob: `true` on the create path,
/// `false` on the open path so unlocking a missing vault fails instead of
/// silently minting an empty one.
pub fn vault_config(
    passphrase: &str,
    db_path: &Path,
    installation_id_file: &Path,
    create_if_missing: bool,
) -> DatabaseConfig {
    DatabaseConfig {
        path: db_path.to_path_buf(),
        key: SqlCipherKey::new(passphrase),
        create_if_missing,
        bootstrap: Arc::new(HaexBootstrap::new(installation_id_file.to_path_buf())),
        signature_provider: Arc::new(NoopSignatureProvider),
        migration_source: haex_migration_source(),
        trigger_version: HAEX_TRIGGER_VERSION,
        max_transaction_bytes: MAX_CRDT_TRANSACTION_BYTES,
        max_value_bytes: haex_crdt::MAX_VALUE_BYTES,
    }
}
