use std::path::Path;
use std::sync::Arc;

use super::*;
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

fn default_of(vault: &VaultDb, ext: Uuid) -> Option<String> {
    vault
        .read_blocking(move |q| get(q, ext).map_err(|e| haex_crdt::Error::consumer(e.to_string())))
        .unwrap()
}

fn set_default(vault: &VaultDb, ext: Uuid, model: &'static str) -> Result<()> {
    vault.write_blocking(move |tx| set(tx, ext, model).map_err(Into::into))
}

#[test]
fn a_new_extension_has_no_default() {
    let (_dir, vault, ext) = installed();
    assert_eq!(default_of(&vault, ext), None);
}

#[test]
fn setting_the_default_replaces_the_one_before_so_there_is_at_most_one() {
    let (_dir, vault, ext) = installed();
    set_default(&vault, ext, "m-a").unwrap();
    assert_eq!(default_of(&vault, ext).as_deref(), Some("m-a"));
    set_default(&vault, ext, "m-b").unwrap();
    assert_eq!(default_of(&vault, ext).as_deref(), Some("m-b"));
    let rows: i64 = vault
        .read_blocking(|q| {
            Ok(
                q.query_map("SELECT COUNT(*) FROM extension_default_model", &[], |r| {
                    r.get::<_, i64>(0)
                })?[0],
            )
        })
        .unwrap();
    assert_eq!(rows, 1);
}

#[test]
fn clearing_takes_the_default_away_and_clear_if_only_its_own_model() {
    let (_dir, vault, ext) = installed();
    set_default(&vault, ext, "m-a").unwrap();
    vault
        .write_blocking(move |tx| clear_if(tx, ext, "m-other").map_err(Into::into))
        .unwrap();
    assert_eq!(default_of(&vault, ext).as_deref(), Some("m-a"));
    vault
        .write_blocking(move |tx| clear_if(tx, ext, "m-a").map_err(Into::into))
        .unwrap();
    assert_eq!(default_of(&vault, ext), None);
    set_default(&vault, ext, "m-a").unwrap();
    vault
        .write_blocking(move |tx| clear(tx, ext).map_err(Into::into))
        .unwrap();
    assert_eq!(default_of(&vault, ext), None);
}

#[test]
fn a_bad_model_id_is_refused_and_nothing_changes() {
    let (_dir, vault, ext) = installed();
    set_default(&vault, ext, "m-a").unwrap();
    for bad in ["*", "", " padded"] {
        assert!(
            matches!(
                set_default(&vault, ext, bad),
                Err(HolziError::InvalidInput { .. })
            ),
            "{bad:?} refused"
        );
    }
    assert_eq!(default_of(&vault, ext).as_deref(), Some("m-a"));
}

#[test]
fn only_an_installed_extension_has_a_default() {
    let (_dir, vault, ext) = installed();
    set_default(&vault, ext, "m-a").unwrap();
    remove(&vault, ext, false, 2).unwrap();
    assert_eq!(default_of(&vault, ext), None, "removal clears the default");
    assert!(matches!(
        set_default(&vault, ext, "m-a"),
        Err(HolziError::ExtensionNotFound)
    ));
}
