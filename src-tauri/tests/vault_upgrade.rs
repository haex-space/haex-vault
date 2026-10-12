//! Regression coverage for CRDT trigger upgrades on an existing vault.
//!
//! haex-crdt bakes the tracked-column list into the `AFTER UPDATE OF
//! <cols>` trigger at install time, and `ensure_triggers_initialized`
//! early-returns when the vault's stored trigger version already equals
//! the one the config passes. A migration that adds a column to a
//! CRDT-tracked table therefore leaves that column *untracked* on every
//! already-provisioned vault unless the trigger version is bumped in
//! lockstep — writes to it never mark the row dirty and never record a
//! per-column HLC, so the value silently fails to reach other devices.
//!
//! Migration `0009_models_add_tokenizer_repo` is exactly that case, and so
//! is `0018_models_add_capabilities`.

// These tests read raw vault state (counts, CRDT columns, the schema) that the
// CRDT write path does not expose.
#![allow(clippy::disallowed_methods)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use haex_crdt::{
    Database, DatabaseConfig, MigrationName, NoopSignatureProvider, SqlCipherKey,
    StaticMigrationSource, DEFAULT_TRIGGER_VERSION,
};

use holzi_lib::identity::{
    holzi_migration_source, installation_id_path, HolziBootstrap, HOLZI_TRIGGER_VERSION,
};
use holzi_lib::instances::vault_config::vault_config;

const PASSPHRASE: &str = "vault-upgrade-integration-test";
const TOKENIZER_MIGRATION: &str = "0009_models_add_tokenizer_repo";
const CAPABILITIES_MIGRATION: &str = "0018_models_add_capabilities";

/// Opens the vault under `dir` with `source` at `trigger_version`.
fn open_vault(
    dir: &Path,
    source: Arc<dyn haex_crdt::MigrationSource>,
    trigger_version: i32,
) -> Database {
    let db_path: PathBuf = dir.join("vault.db");
    let installation_id = installation_id_path(dir);
    Database::open(DatabaseConfig {
        path: db_path,
        key: SqlCipherKey::new(PASSPHRASE),
        create_if_missing: true,
        bootstrap: Arc::new(HolziBootstrap::new(installation_id).with_alias("test")),
        signature_provider: Arc::new(NoopSignatureProvider),
        migration_source: source,
        trigger_version,
        max_transaction_bytes: haex_crdt::MAX_CRDT_TRANSACTION_BYTES,
        max_value_bytes: haex_crdt::MAX_VALUE_BYTES,
    })
    .expect("vault open")
}

/// The production migration set truncated before `migration`, standing in for
/// a vault provisioned by an older release. Later migrations are excluded as
/// well because they may depend on the migration being modelled as absent.
fn source_without(migration: &str) -> Arc<StaticMigrationSource> {
    let full = holzi_migration_source();
    let m: BTreeMap<MigrationName, String> = full
        .0
        .iter()
        .filter(|(name, _)| name.as_str() < migration)
        .map(|(name, sql)| (name.clone(), sql.clone()))
        .collect();
    Arc::new(StaticMigrationSource(m))
}

fn pre_0009_source() -> Arc<StaticMigrationSource> {
    source_without(TOKENIZER_MIGRATION)
}

/// The SQL of the `models` UPDATE trigger as currently installed.
fn models_update_trigger_sql(db: &Database) -> String {
    db.with_connection(|conn| {
        let sql: String = conn.query_row(
            "SELECT sql FROM sqlite_master \
             WHERE type = 'trigger' AND name = 'z_dirty_models_update'",
            [],
            |r| r.get(0),
        )?;
        Ok(sql)
    })
    .expect("models update trigger must exist")
}

#[test]
fn reopening_a_pre_0009_vault_tracks_tokenizer_repo() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db_path = dir.path().join("vault.db");
    let installation_id = installation_id_path(dir.path());

    // Provision the vault as an older holzi release would have: no 0009,
    // triggers installed against the pre-0009 `models` shape.
    let old = open_vault(dir.path(), pre_0009_source(), DEFAULT_TRIGGER_VERSION);
    assert!(
        !models_update_trigger_sql(&old).contains("tokenizer_repo"),
        "precondition: the pre-0009 trigger must not know the column yet"
    );
    drop(old);

    // Reopen through the *production* config — the same one
    // `open_instance` uses. Migration 0009 adds the column, and the
    // trigger version must be high enough that the triggers are rewritten
    // against the migrated schema.
    let upgraded = Database::open(vault_config(PASSPHRASE, &db_path, &installation_id, false))
        .expect("reopen upgraded vault");
    let sql = models_update_trigger_sql(&upgraded);
    assert!(
        sql.contains("tokenizer_repo"),
        "0009 added `tokenizer_repo` to the CRDT-tracked `models` table, but the \
         production open path left the trigger untracked — writes to it will not \
         sync. Bump HOLZI_TRIGGER_VERSION.\n{sql}"
    );
}

/// The negative control: reopening at the version the vault already
/// stores is precisely what leaves the column untracked. This is the bug
/// the bump fixes, and it documents why the constant must move whenever a
/// migration reshapes a CRDT-tracked table.
#[test]
fn reopening_without_a_version_bump_leaves_tokenizer_repo_untracked() {
    let dir = tempfile::tempdir().expect("tempdir");

    let old = open_vault(dir.path(), pre_0009_source(), DEFAULT_TRIGGER_VERSION);
    drop(old);

    let stale = open_vault(
        dir.path(),
        holzi_migration_source(),
        DEFAULT_TRIGGER_VERSION,
    );
    assert!(
        !models_update_trigger_sql(&stale).contains("tokenizer_repo"),
        "ensure_triggers_initialized is expected to early-return here; if this \
         now passes, haex-crdt changed its upgrade semantics and \
         HOLZI_TRIGGER_VERSION may no longer be needed"
    );
}

/// Same upgrade path for `0018_models_add_capabilities`: a vault provisioned
/// before it must have `capabilities_json` tracked after reopening through
/// the production config, or a capability refresh on one device would never
/// reach the user's other devices.
#[test]
fn reopening_a_pre_0018_vault_tracks_capabilities_json() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db_path = dir.path().join("vault.db");
    let installation_id = installation_id_path(dir.path());

    let old = open_vault(
        dir.path(),
        source_without(CAPABILITIES_MIGRATION),
        DEFAULT_TRIGGER_VERSION,
    );
    assert!(
        !models_update_trigger_sql(&old).contains("capabilities_json"),
        "precondition: the pre-0018 trigger must not know the column yet"
    );
    drop(old);

    let upgraded = Database::open(vault_config(PASSPHRASE, &db_path, &installation_id, false))
        .expect("reopen upgraded vault");
    let sql = models_update_trigger_sql(&upgraded);
    assert!(
        sql.contains("capabilities_json"),
        "0018 added `capabilities_json` to the CRDT-tracked `models` table, but the \
         production open path left the trigger untracked — capability records will \
         not sync. Bump HOLZI_TRIGGER_VERSION.\n{sql}"
    );
}

/// `0022_passwords` introduces twelve CRDT-tracked tables: a vault provisioned before it must have
/// the `z_dirty_*` triggers on all of them after reopening through the production config (trigger
/// version 14), or nothing written to the password manager would ever sync.
#[test]
fn reopening_a_pre_0022_vault_installs_the_password_table_triggers() {
    const PASSWORD_TABLES: [&str; 12] = [
        "haex_passwords_item_details",
        "haex_passwords_item_key_values",
        "haex_passwords_groups",
        "haex_passwords_group_items",
        "haex_passwords_binaries",
        "haex_passwords_item_binaries",
        "haex_passwords_item_snapshots",
        "haex_passwords_snapshot_binaries",
        "haex_passwords_generator_presets",
        "haex_passwords_tags",
        "haex_passwords_item_tags",
        "haex_passwords_passkeys",
    ];
    let trigger_count = |db: &Database, table: &str| -> i64 {
        db.with_connection(|conn| {
            Ok(conn.query_row(
                "SELECT COUNT(*) FROM sqlite_master \
                 WHERE type = 'trigger' AND name LIKE 'z_dirty_' || ?1 || '_%'",
                [table],
                |r| r.get(0),
            )?)
        })
        .expect("count triggers")
    };
    let dir = tempfile::tempdir().expect("tempdir");
    let db_path = dir.path().join("vault.db");
    let installation_id = installation_id_path(dir.path());

    let old = open_vault(
        dir.path(),
        source_without("0022_passwords"),
        DEFAULT_TRIGGER_VERSION,
    );
    for table in PASSWORD_TABLES {
        assert_eq!(trigger_count(&old, table), 0, "{table} does not exist yet");
    }
    drop(old);

    let upgraded = Database::open(vault_config(PASSPHRASE, &db_path, &installation_id, false))
        .expect("reopen upgraded vault");
    for table in PASSWORD_TABLES {
        assert!(
            trigger_count(&upgraded, table) > 0,
            "{table} is CRDT-tracked, but the production open path left it without triggers — \
             its writes will not sync. Bump HOLZI_TRIGGER_VERSION."
        );
    }
}

/// `0023_extensions` (spec 017) introduces nine CRDT-tracked registry tables: a vault provisioned
/// before it must have the `z_dirty_*` triggers on all of them after reopening through the
/// production config (trigger version 15), or an installation would never reach the other devices.
#[test]
fn reopening_a_pre_0023_vault_installs_the_extension_table_triggers() {
    const EXTENSION_TABLES: [&str; 9] = [
        "extensions",
        "extension_bundles",
        "extension_bundle_files",
        "extension_blobs",
        "extension_migrations",
        "extension_permissions",
        "extension_limits",
        "extension_device_status",
        "extension_kv",
    ];
    let trigger_count = |db: &Database, table: &str| -> i64 {
        db.with_connection(|conn| {
            Ok(conn.query_row(
                "SELECT COUNT(*) FROM sqlite_master \
                 WHERE type = 'trigger' AND name LIKE 'z_dirty_' || ?1 || '_%'",
                [table],
                |r| r.get(0),
            )?)
        })
        .expect("count triggers")
    };
    let dir = tempfile::tempdir().expect("tempdir");
    let db_path = dir.path().join("vault.db");
    let installation_id = installation_id_path(dir.path());

    let old = open_vault(
        dir.path(),
        source_without("0023_extensions"),
        DEFAULT_TRIGGER_VERSION,
    );
    for table in EXTENSION_TABLES {
        assert_eq!(trigger_count(&old, table), 0, "{table} does not exist yet");
    }
    drop(old);

    let upgraded = Database::open(vault_config(PASSPHRASE, &db_path, &installation_id, false))
        .expect("reopen upgraded vault");
    for table in EXTENSION_TABLES {
        assert!(
            trigger_count(&upgraded, table) > 0,
            "{table} is CRDT-tracked, but the production open path left it without triggers — \
             its writes will not sync. Bump HOLZI_TRIGGER_VERSION."
        );
    }
}

/// `0026_passwords_refs` (spec 036) introduces the CRDT-tracked `haex_passwords_passkey_links`. A
/// vault provisioned by the release before it already stores trigger version 15, so only the bump
/// to 16 makes the production open path install the table's triggers; without them a link would
/// never reach the other devices.
#[test]
fn reopening_a_pre_0026_vault_installs_the_passkey_link_triggers() {
    const TABLE: &str = "haex_passwords_passkey_links";
    let trigger_count = |db: &Database| -> i64 {
        db.with_connection(|conn| {
            Ok(conn.query_row(
                "SELECT COUNT(*) FROM sqlite_master \
                 WHERE type = 'trigger' AND name LIKE 'z_dirty_' || ?1 || '_%'",
                [TABLE],
                |r| r.get(0),
            )?)
        })
        .expect("count triggers")
    };
    let dir = tempfile::tempdir().expect("tempdir");
    let db_path = dir.path().join("vault.db");
    let installation_id = installation_id_path(dir.path());

    let old = open_vault(dir.path(), source_without("0026_passwords_refs"), 15);
    assert_eq!(trigger_count(&old), 0, "{TABLE} does not exist yet");
    drop(old);

    let upgraded = Database::open(vault_config(PASSPHRASE, &db_path, &installation_id, false))
        .expect("reopen upgraded vault");
    assert!(
        trigger_count(&upgraded) > 0,
        "{TABLE} is CRDT-tracked, but the production open path left it without triggers — its \
         writes will not sync. Bump HOLZI_TRIGGER_VERSION."
    );
    let installed = trigger_count(&upgraded);
    drop(upgraded);

    let again = Database::open(vault_config(PASSPHRASE, &db_path, &installation_id, false))
        .expect("reopen the vault a second time");
    assert_eq!(
        trigger_count(&again),
        installed,
        "a second open changes nothing"
    );
}

#[test]
fn reopening_a_pre_0027_vault_tracks_the_owner_of_a_password_entry() {
    let trigger_sql = |db: &Database| -> String {
        db.with_connection(|conn| {
            Ok(conn.query_row(
                "SELECT sql FROM sqlite_master \
                 WHERE type = 'trigger' AND name = 'z_dirty_haex_passwords_item_details_update'",
                [],
                |r| r.get(0),
            )?)
        })
        .expect("the update trigger of the entries must exist")
    };
    let dir = tempfile::tempdir().expect("tempdir");
    let db_path = dir.path().join("vault.db");
    let installation_id = installation_id_path(dir.path());

    let old = open_vault(dir.path(), source_without("0027_passwords_owner"), 16);
    assert!(
        !trigger_sql(&old).contains("owner"),
        "precondition: the pre-0027 trigger must not know the column yet"
    );
    drop(old);

    let upgraded = Database::open(vault_config(PASSPHRASE, &db_path, &installation_id, false))
        .expect("reopen upgraded vault");
    let sql = trigger_sql(&upgraded);
    assert!(
        sql.contains("owner"),
        "0027 added `owner` to the CRDT-tracked entries (spec 038, rule Z14), but the production \
         open path left it untracked — an owner set on one device would not reach the others. \
         Bump HOLZI_TRIGGER_VERSION.\n{sql}"
    );
}

/// `0028_storage_connections` (spec 038) introduces the CRDT-tracked storage connections and
/// storages. A vault from the release before it stores trigger version 17, so only the bump to 18
/// installs their triggers; without them a storage would stay on the device it was created on.
#[test]
fn reopening_a_pre_0028_vault_installs_the_storage_triggers() {
    let trigger_count = |db: &Database, table: &str| -> i64 {
        db.with_connection(|conn| {
            Ok(conn.query_row(
                "SELECT COUNT(*) FROM sqlite_master \
                 WHERE type = 'trigger' AND name LIKE 'z_dirty_' || ?1 || '_%'",
                [table],
                |r| r.get(0),
            )?)
        })
        .expect("count triggers")
    };
    let dir = tempfile::tempdir().expect("tempdir");
    let db_path = dir.path().join("vault.db");
    let installation_id = installation_id_path(dir.path());

    let old = open_vault(dir.path(), source_without("0028_storage_connections"), 17);
    drop(old);

    let upgraded = Database::open(vault_config(PASSPHRASE, &db_path, &installation_id, false))
        .expect("reopen upgraded vault");
    for table in ["haex_storage_connections", "haex_storages"] {
        assert!(
            trigger_count(&upgraded, table) > 0,
            "{table} is CRDT-tracked (spec 038 FR-006), but the production open path left it \
             without triggers — its writes will not sync. Bump HOLZI_TRIGGER_VERSION."
        );
    }
}

/// `0031_storage_protocols` (spec 050) adds the protocol and the host key to the CRDT-tracked
/// connections. A vault from before it stores trigger version 19, so only the bump to 20 tracks the
/// new columns; without it a confirmed host key would stay on the device that confirmed it.
#[test]
fn reopening_a_pre_0031_vault_tracks_the_protocol_of_a_connection() {
    let trigger_sql = |db: &Database| -> String {
        db.with_connection(|conn| {
            Ok(conn.query_row(
                "SELECT sql FROM sqlite_master \
                 WHERE type = 'trigger' AND name = 'z_dirty_haex_storage_connections_update'",
                [],
                |r| r.get(0),
            )?)
        })
        .expect("the update trigger of the connections must exist")
    };
    let dir = tempfile::tempdir().expect("tempdir");
    let db_path = dir.path().join("vault.db");
    let installation_id = installation_id_path(dir.path());

    let old = open_vault(dir.path(), source_without("0031_storage_protocols"), 19);
    assert!(
        !trigger_sql(&old).contains("host_key"),
        "precondition: the pre-0031 trigger must not know the columns yet"
    );
    drop(old);

    let upgraded = Database::open(vault_config(PASSPHRASE, &db_path, &installation_id, false))
        .expect("reopen upgraded vault");
    let sql = trigger_sql(&upgraded);
    assert!(
        sql.contains("protocol") && sql.contains("host_key"),
        "0031 added `protocol` and `host_key` to the CRDT-tracked connections (spec 050 FR-009), \
         but the production open path left them untracked. Bump HOLZI_TRIGGER_VERSION.\n{sql}"
    );
}

#[test]
fn a_vault_from_before_spec_024_gets_its_derived_identity_and_first_device_list() {
    let dir = tempfile::tempdir().expect("tempdir");
    let placeholder = [21u8; 32];
    {
        let old = open_vault(
            dir.path(),
            source_without("0021_own_device_sync"),
            DEFAULT_TRIGGER_VERSION,
        );
        old.with_connection(|conn| {
            conn.execute(
                "INSERT INTO vault_identity (id, pubkey, privkey) VALUES (1, ?1, ?2)",
                haex_crdt::rusqlite::params![[3u8; 33].as_slice(), placeholder.as_slice()],
            )?;
            Ok(())
        })
        .expect("placeholder identity");
        old.write(|tx| {
            holzi_lib::storage::preferences::insert_or_update(
                tx,
                holzi_lib::storage::preferences::PrefScope::Vault,
                "chat.permission_mode",
                "auto",
            )
        })
        .expect("old data");
    }

    let db = open_vault(dir.path(), holzi_migration_source(), HOLZI_TRIGGER_VERSION);
    holzi_lib::sync::genesis::run_after_open(&db, &installation_id_path(dir.path()), true);

    let derived = holzi_lib::sync::keys::derive_vault_identity(&placeholder);
    let expected = holzi_lib::sync::signing::xonly_public_key(&derived).expect("public key");
    let pubkey = holzi_lib::storage::query::read(&db, |r| holzi_lib::sync::keys::vault_pubkey(r))
        .expect("read identity")
        .expect("published identity");
    assert_eq!(pubkey, expected);

    let rows = holzi_lib::storage::query::read(&db, |r| holzi_lib::sync::device_list::load_all(r))
        .expect("device lists");
    let valid = holzi_lib::sync::device_list::valid_lists(&rows, &pubkey);
    let effective = holzi_lib::sync::device_list::effective(&valid).expect("first list");
    assert_eq!(effective.list.generation, 1);
    assert_eq!(effective.list.devices[0].vault_device_uuid, db.device_id());

    let kept = holzi_lib::storage::query::read(&db, |r| {
        holzi_lib::storage::preferences::get(
            r,
            holzi_lib::storage::preferences::PrefScope::Vault,
            "chat.permission_mode",
        )
    })
    .expect("read preference");
    assert_eq!(
        kept.as_deref(),
        Some("auto"),
        "the old data survives the upgrade"
    );
}
