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
        crate::identity::holzi_migration_source_with_legacy_icon_set_name();
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
