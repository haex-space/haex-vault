use std::sync::Arc;

use haex_crdt::{Database, MigrationName, StaticMigrationSource};

use crate::identity::installation_id_path;
use crate::instances::open::open_existing_database;
use crate::instances::vault_config::vault_config;

#[test]
fn opens_a_vault_with_the_legacy_file_icon_migration_name() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db_path = dir.path().join("legacy.db");
    let installation_id = installation_id_path(dir.path());
    let passphrase = "legacy-migration-passphrase";

    let mut legacy_config = vault_config(passphrase, &db_path, &installation_id, true);
    legacy_config.migration_source =
        crate::identity::holzi_migration_source_with_legacy_names(&["0035_file_icon_sets".into()]);
    Database::open(legacy_config).expect("create a pre-renumbering vault");

    let opened = open_existing_database(passphrase, &db_path, &installation_id);
    assert!(opened.is_ok(), "legacy migration name must be accepted");
}

#[test]
fn opens_a_vault_with_the_legacy_contacts_display_name_migration_name() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db_path = dir.path().join("legacy-contacts.db");
    let installation_id = installation_id_path(dir.path());
    let passphrase = "legacy-contacts-migration-passphrase";

    let current = crate::identity::holzi_migration_source();
    let mut legacy_migrations = current.0.clone();
    legacy_migrations.remove(&MigrationName::from("0040_device_peer_schema"));
    legacy_migrations.remove(&MigrationName::from("0041_contacts_card_display_name"));
    legacy_migrations.insert(
        MigrationName::from("0040_contacts_card_display_name"),
        "UPDATE haex_contacts SET display_name = NULL \
         WHERE origin_kind = 'card' AND display_name LIKE 'npub1%';"
            .to_owned(),
    );

    let mut legacy_config = vault_config(passphrase, &db_path, &installation_id, true);
    legacy_config.migration_source = Arc::new(StaticMigrationSource(legacy_migrations));
    Database::open(legacy_config).expect("create a pre-peer-schema vault");

    let opened = open_existing_database(passphrase, &db_path, &installation_id);
    assert!(
        opened.is_ok(),
        "legacy contacts migration name must be accepted"
    );
}

/// Creates a vault whose journal holds the given old names, from before the MCP agents
/// migration, and checks that it opens twice: the first open applies what is pending.
fn opens_a_vault_with_old_names(file: &str, legacy: &[&str]) {
    let dir = tempfile::tempdir().expect("tempdir");
    let db_path = dir.path().join(file);
    let installation_id = installation_id_path(dir.path());
    let passphrase = "legacy-names-passphrase";

    let legacy: Vec<String> = legacy.iter().map(|name| (*name).to_owned()).collect();
    let mut legacy_migrations = crate::identity::holzi_migration_source_with_legacy_names(&legacy)
        .0
        .clone();
    legacy_migrations.remove(&MigrationName::from("0045_mcp_agents"));

    let mut legacy_config = vault_config(passphrase, &db_path, &installation_id, true);
    legacy_config.migration_source = Arc::new(StaticMigrationSource(legacy_migrations));
    Database::open(legacy_config).expect("create a vault with old migration names");

    for _ in 0..2 {
        let opened = open_existing_database(passphrase, &db_path, &installation_id);
        assert!(
            opened.is_ok(),
            "old migration names {legacy:?} must be accepted"
        );
    }
}

#[test]
fn opens_a_vault_with_the_legacy_sync_rules_decrypt_migration_name() {
    opens_a_vault_with_old_names("legacy-decrypt.db", &["0045_sync_rules_decrypt"]);
}

#[test]
fn opens_a_vault_with_several_legacy_migration_names() {
    opens_a_vault_with_old_names(
        "legacy-several.db",
        &["0035_file_icon_sets", "0045_sync_rules_decrypt"],
    );
}

#[test]
fn refuses_a_vault_that_holds_a_migration_this_build_does_not_know() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db_path = dir.path().join("newer.db");
    let installation_id = installation_id_path(dir.path());
    let passphrase = "newer-vault-passphrase";

    let current = crate::identity::holzi_migration_source();
    let mut newer_migrations = current.0.clone();
    newer_migrations.insert(
        MigrationName::from("9999_from_a_newer_build"),
        "CREATE TABLE newer_build_marker_no_sync (id TEXT PRIMARY KEY NOT NULL);".to_owned(),
    );

    let mut newer_config = vault_config(passphrase, &db_path, &installation_id, true);
    newer_config.migration_source = Arc::new(StaticMigrationSource(newer_migrations));
    Database::open(newer_config).expect("create a vault of a newer build");

    let opened = open_existing_database(passphrase, &db_path, &installation_id);
    assert!(
        matches!(
            opened,
            Err(crate::error::HolziError::MigrationMissingFromSource { .. })
        ),
        "a vault of a newer build must report the unknown migration"
    );
}
