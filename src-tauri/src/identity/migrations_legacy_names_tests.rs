use haex_crdt::MigrationName;

use super::migrations::holzi_migration_source;
use super::migrations_legacy_names::{
    holzi_migration_source_with_legacy_names, is_legacy_migration_name,
};

fn names(legacy: &[&str]) -> Vec<String> {
    legacy.iter().map(|name| (*name).to_owned()).collect()
}

#[test]
fn knows_only_the_old_names() {
    assert!(is_legacy_migration_name("0035_file_icon_sets"));
    assert!(is_legacy_migration_name("0040_contacts_card_display_name"));
    assert!(is_legacy_migration_name("0045_sync_rules_decrypt"));
    assert!(!is_legacy_migration_name("0046_sync_rules_decrypt"));
    assert!(!is_legacy_migration_name("9999_from_a_newer_build"));
}

#[test]
fn replaces_a_migration_that_must_not_run_twice() {
    let current = holzi_migration_source();
    let legacy = holzi_migration_source_with_legacy_names(&names(&["0045_sync_rules_decrypt"]));

    assert!(!legacy
        .0
        .contains_key(&MigrationName::from("0046_sync_rules_decrypt")));
    assert_eq!(
        legacy
            .0
            .get(&MigrationName::from("0045_sync_rules_decrypt")),
        current
            .0
            .get(&MigrationName::from("0046_sync_rules_decrypt"))
    );
    assert!(legacy
        .0
        .contains_key(&MigrationName::from("0045_mcp_agents")));
}

#[test]
fn keeps_a_repeatable_migration_under_both_names() {
    let legacy =
        holzi_migration_source_with_legacy_names(&names(&["0040_contacts_card_display_name"]));

    assert!(legacy
        .0
        .contains_key(&MigrationName::from("0040_contacts_card_display_name")));
    assert!(legacy
        .0
        .contains_key(&MigrationName::from("0041_contacts_card_display_name")));
}

#[test]
fn applies_several_old_names_and_ignores_unknown_ones() {
    let current = holzi_migration_source();
    let legacy = holzi_migration_source_with_legacy_names(&names(&[
        "0035_file_icon_sets",
        "0045_sync_rules_decrypt",
        "9999_from_a_newer_build",
    ]));

    assert!(legacy
        .0
        .contains_key(&MigrationName::from("0035_file_icon_sets")));
    assert!(legacy
        .0
        .contains_key(&MigrationName::from("0045_sync_rules_decrypt")));
    assert!(!legacy
        .0
        .contains_key(&MigrationName::from("0036_file_icon_sets")));
    assert!(!legacy
        .0
        .contains_key(&MigrationName::from("9999_from_a_newer_build")));
    assert_eq!(legacy.0.len(), current.0.len());
}
