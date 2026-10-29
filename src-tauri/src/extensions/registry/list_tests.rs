use std::path::Path;
use std::sync::Arc;

use super::*;
use crate::extensions::registry::install::install;
use crate::passwords::test_support::open_test_vault;
use crate::storage::known_devices;
use crate::vault_gate::VaultGate;

fn vector(name: &str) -> Vec<u8> {
    std::fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/extension_bundles")
            .join(name),
    )
    .unwrap()
}

#[test]
fn installed_extensions_are_listed_with_title_version_and_icon() {
    let (_dir, db) = open_test_vault();
    let vault = VaultGate::new().vault_db(Arc::new(db.clone())).unwrap();
    let device = vault
        .read_blocking(|q| known_devices::list_devices(q))
        .unwrap()[0]
        .vault_device_uuid;
    let notes = install(
        &vault,
        &vector("good-notes-like.xt"),
        vec![],
        false,
        device,
        1,
    )
    .unwrap()
    .ids
    .extension_id;
    install(&vault, &vector("good-minimal.xt"), vec![], false, device, 1).unwrap();

    let list = vault
        .read_blocking(move |q| list(q, device).map_err(Into::into))
        .unwrap();
    assert_eq!(list.len(), 2);
    let entry = list.iter().find(|e| e.id == notes.to_string()).unwrap();
    assert_eq!(entry.title, "Notes");
    assert_eq!(entry.version.as_deref(), Some("1.2.0"));
    assert!(entry.enabled && entry.has_icon && !entry.single_instance);
    assert_eq!(entry.state, "installed");
    assert_eq!(entry.status_here, None);

    let icon = vault
        .read_blocking(move |q| icon_data_url(q, notes).map_err(Into::into))
        .unwrap()
        .unwrap();
    assert!(icon.starts_with("data:image/svg+xml;base64,"), "{icon}");
}

fn e2e_bundle(name: &str) -> Vec<u8> {
    std::fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/extension_e2e")
            .join(name),
    )
    .unwrap()
}

/// What `list` says of `id`'s openers.
fn listed_openers(
    vault: &crate::vault_gate::VaultDb,
    device: Uuid,
    id: Uuid,
) -> Option<Vec<String>> {
    vault
        .read_blocking(move |q| list(q, device).map_err(Into::into))
        .unwrap()
        .into_iter()
        .find(|e| e.id == id.to_string())
        .map(|e| e.openers)
}

#[test]
fn openers_are_listed_while_installed_and_enabled() {
    let (_dir, db) = open_test_vault();
    let vault = VaultGate::new().vault_db(Arc::new(db.clone())).unwrap();
    let device = vault
        .read_blocking(|q| known_devices::list_devices(q))
        .unwrap()[0]
        .vault_device_uuid;
    let mail = install(
        &vault,
        &e2e_bundle("mail-opener.xt"),
        vec![],
        false,
        device,
        1,
    )
    .unwrap()
    .ids
    .extension_id;
    let notes = install(
        &vault,
        &vector("good-notes-like.xt"),
        vec![],
        false,
        device,
        1,
    )
    .unwrap()
    .ids
    .extension_id;

    assert_eq!(
        listed_openers(&vault, device, mail),
        Some(vec!["mail.compose".to_owned()])
    );
    assert_eq!(
        listed_openers(&vault, device, notes),
        Some(Vec::new()),
        "no block, no opener"
    );
}

#[test]
fn a_disabled_extension_lists_no_openers() {
    let (_dir, db) = open_test_vault();
    let vault = VaultGate::new().vault_db(Arc::new(db.clone())).unwrap();
    let device = vault
        .read_blocking(|q| known_devices::list_devices(q))
        .unwrap()[0]
        .vault_device_uuid;
    let mail = install(
        &vault,
        &e2e_bundle("mail-opener.xt"),
        vec![],
        false,
        device,
        1,
    )
    .unwrap()
    .ids
    .extension_id;

    crate::extensions::registry::remove::set_enabled(&vault, mail, false, 2).unwrap();
    assert_eq!(listed_openers(&vault, device, mail), Some(Vec::new()));
    crate::extensions::registry::remove::set_enabled(&vault, mail, true, 3).unwrap();
    assert_eq!(
        listed_openers(&vault, device, mail),
        Some(vec!["mail.compose".to_owned()])
    );
}

#[test]
fn a_removed_extension_lists_no_openers() {
    let (_dir, db) = open_test_vault();
    let vault = VaultGate::new().vault_db(Arc::new(db.clone())).unwrap();
    let device = vault
        .read_blocking(|q| known_devices::list_devices(q))
        .unwrap()[0]
        .vault_device_uuid;
    let mail = install(
        &vault,
        &e2e_bundle("mail-opener.xt"),
        vec![],
        false,
        device,
        1,
    )
    .unwrap()
    .ids
    .extension_id;

    // Removed with its data kept: still listed, but it opens nothing.
    crate::extensions::registry::remove::remove(&vault, mail, false, 2).unwrap();
    let kept = listed_openers(&vault, device, mail);
    assert_eq!(kept, Some(Vec::new()));
    // Removed with its data: gone from the list.
    crate::extensions::registry::remove::purge_kept_data(&vault, mail, 3).unwrap();
    assert_eq!(listed_openers(&vault, device, mail), None);
}

#[test]
fn only_known_kinds_are_listed() {
    let mut bundle = crate::extensions::bundle::verify_bundle(&vector("good-minimal.xt")).unwrap();
    bundle.manifest.insert(
        "openers".into(),
        haex_bundle::jcs::parse_restricted(r#"["tel.dial","mail.compose","x.y"]"#).unwrap(),
    );
    let manifest = Manifest::from_verified(&bundle).unwrap();
    assert_eq!(manifest.known_openers(), ["mail.compose"]);
}
