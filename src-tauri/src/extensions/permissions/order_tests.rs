use std::path::Path;
use std::sync::Arc;

use super::*;
use crate::extensions::ids::model_order_id;
use crate::extensions::registry::install::install;
use crate::extensions::registry::remove::remove;
use crate::passwords::test_support::open_test_vault;
use crate::storage::known_devices;
use crate::vault_gate::{VaultDb, VaultGate};

fn installed() -> (tempfile::TempDir, VaultDb, Uuid) {
    let (dir, db) = open_test_vault();
    let vault = VaultGate::new().vault_db(Arc::new(db)).unwrap();
    let device = vault
        .read_blocking(|q| known_devices::list_devices(q))
        .unwrap()[0]
        .vault_device_uuid;
    let bytes = std::fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/extension_bundles/good-notes-like.xt"),
    )
    .unwrap();
    let ext = install(&vault, &bytes, vec![], false, device, 1)
        .unwrap()
        .ids
        .extension_id;
    (dir, vault, ext)
}

fn ids(list: &[&str]) -> Vec<String> {
    list.iter().map(|id| (*id).to_owned()).collect()
}

fn ranked(vault: &VaultDb, ext: Uuid) -> Vec<String> {
    vault
        .read_blocking(move |q| {
            ordered(q, ext).map_err(|e| haex_crdt::Error::consumer(e.to_string()))
        })
        .unwrap()
}

fn set(vault: &VaultDb, ext: Uuid, list: Vec<String>) -> Result<()> {
    vault.write_blocking(move |tx| set_order(tx, ext, &list).map_err(Into::into))
}

fn add(vault: &VaultDb, ext: Uuid, model: &'static str) -> Result<()> {
    vault.write_blocking(move |tx| append(tx, ext, model).map_err(Into::into))
}

#[test]
fn without_a_ranking_nothing_is_ranked() {
    let (_dir, vault, ext) = installed();
    assert!(ranked(&vault, ext).is_empty());
}

#[test]
fn a_new_grant_comes_last_and_a_ranked_model_keeps_its_place() {
    let (_dir, vault, ext) = installed();
    add(&vault, ext, "m-b").unwrap();
    add(&vault, ext, "m-a").unwrap();
    add(&vault, ext, "m-c").unwrap();
    assert_eq!(ranked(&vault, ext), ["m-b", "m-a", "m-c"]);

    add(&vault, ext, "m-b").unwrap();
    assert_eq!(ranked(&vault, ext), ["m-b", "m-a", "m-c"]);
}

#[test]
fn the_user_replaces_the_whole_ranking_and_an_unlisted_model_loses_its_place() {
    let (_dir, vault, ext) = installed();
    set(&vault, ext, ids(&["m-a", "m-b", "m-c"])).unwrap();
    assert_eq!(ranked(&vault, ext), ["m-a", "m-b", "m-c"]);

    set(&vault, ext, ids(&["m-c", "m-a", "m-d"])).unwrap();
    assert_eq!(ranked(&vault, ext), ["m-c", "m-a", "m-d"]);

    set(&vault, ext, Vec::new()).unwrap();
    assert!(ranked(&vault, ext).is_empty());
}

#[test]
fn a_bad_list_is_refused_and_nothing_changes() {
    let (_dir, vault, ext) = installed();
    set(&vault, ext, ids(&["m-a", "m-b"])).unwrap();
    let too_many: Vec<String> = (0..=MAX_RANKED).map(|n| format!("m-{n}")).collect();
    for bad in [
        ids(&["m-a", "m-a"]),
        ids(&["m-a", "*"]),
        ids(&["m-a", ""]),
        ids(&["m-a", " padded"]),
        too_many,
    ] {
        assert!(
            matches!(set(&vault, ext, bad), Err(HolziError::InvalidInput { .. })),
            "refused"
        );
    }
    assert_eq!(ranked(&vault, ext), ["m-a", "m-b"]);
    assert!(matches!(
        add(&vault, ext, "*"),
        Err(HolziError::InvalidInput { .. })
    ));
}

#[test]
fn only_an_installed_extension_has_a_ranking() {
    let (_dir, vault, ext) = installed();
    set(&vault, ext, ids(&["m-a"])).unwrap();
    remove(&vault, ext, false, 2).unwrap();
    assert!(ranked(&vault, ext).is_empty(), "removal clears the ranking");
    assert!(matches!(
        set(&vault, ext, ids(&["m-a"])),
        Err(HolziError::ExtensionNotFound)
    ));
    assert!(matches!(
        add(&vault, ext, "m-a"),
        Err(HolziError::ExtensionNotFound)
    ));
}

#[test]
fn equal_positions_read_in_the_order_of_the_model_id() {
    // Two devices that appended at once write the same position.
    let (_dir, vault, ext) = installed();
    vault
        .write_blocking(move |tx| {
            for model in ["m-z", "m-a"] {
                tx.execute(
                    "INSERT INTO extension_model_order (id, extension_id, model_id, position) \
                     VALUES (?1, ?2, ?3, 4)",
                    haex_crdt::rusqlite::params![
                        model_order_id(ext, model).to_string(),
                        ext.to_string(),
                        model
                    ],
                )?;
            }
            Ok(())
        })
        .unwrap();
    assert_eq!(ranked(&vault, ext), ["m-a", "m-z"]);
}

#[test]
fn the_row_of_a_model_is_one_per_extension_and_model() {
    let one = Uuid::from_u128(1);
    let other = Uuid::from_u128(2);
    assert_eq!(model_order_id(one, "m"), model_order_id(one, "m"));
    assert_ne!(model_order_id(one, "m"), model_order_id(other, "m"));
    assert_ne!(model_order_id(one, "m"), model_order_id(one, "n"));
}
