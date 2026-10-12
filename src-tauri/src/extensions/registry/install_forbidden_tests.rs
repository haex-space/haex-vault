//! A manifest may never ask for encrypted folders (spec 048 FR-037, T057): refused at the preview,
//! the install, an update and when a development version loads, under every spelling; and no
//! extension, update or development version gets a grant the user gave another extension.

use std::sync::Arc;

use ed25519_dalek::SigningKey;
use serde_json::json;

use super::*;
use crate::extensions::commands::permissions::{set, PermissionSetArgs};
use crate::extensions::ids::{dev_extension_id, ExtensionName, PublicKey};
use crate::extensions::permissions::store::candidates;
use crate::extensions::permissions::PermissionKind;
use crate::passwords::test_support::open_test_vault;
use crate::storage::known_devices;
use crate::vault_gate::VaultGate;

const FOLDER: &str = "s1/aaaaaaaaaaaaaaaaaaaaaaaaaa";

fn key(seed: u8) -> SigningKey {
    SigningKey::from_bytes(&[seed; 32])
}

fn public(seed: u8) -> String {
    crate::sync::keys::hex(key(seed).verifying_key().as_bytes())
}

fn build(name: &str, version: &str, permissions: &str, seed: u8) -> Vec<u8> {
    let manifest = haex_bundle::jcs::parse_restricted(&format!(
        r#"{{"name":"{name}","version":"{version}","permissions":{permissions}}}"#
    ))
    .unwrap();
    let files = vec![haex_bundle::Entry {
        path: "index.html".into(),
        data: b"<!doctype html>".to_vec(),
    }];
    haex_bundle::build_archive(files, manifest, &key(seed)).unwrap()
}

struct Vault {
    _dir: tempfile::TempDir,
    vault: VaultDb,
    device: Uuid,
}

fn vault() -> Vault {
    let (dir, db) = open_test_vault();
    let vault = VaultGate::new().vault_db(Arc::new(db)).unwrap();
    let device = vault
        .read_blocking(|q| known_devices::list_devices(q))
        .unwrap()[0]
        .vault_device_uuid;
    Vault {
        _dir: dir,
        vault,
        device,
    }
}

impl Vault {
    fn preview(&self, bytes: Vec<u8>) -> InstallPreview {
        self.vault
            .read_blocking(move |q| install_preview(q, &bytes).map_err(Into::into))
            .unwrap()
    }

    fn install(&self, bytes: &[u8]) -> Result<Uuid> {
        install(&self.vault, bytes, vec![], true, self.device, 1_000).map(|i| i.ids.extension_id)
    }

    fn grant(&self, extension: Uuid) {
        set(
            &self.vault,
            self.device,
            PermissionSetArgs {
                extension_id: extension.to_string(),
                kind: "encryptedFolder".into(),
                action: "read".into(),
                target: FOLDER.into(),
                status: "granted".into(),
                replaces: None,
            },
            1,
        )
        .unwrap();
    }

    fn folder_grants(&self, extension: Uuid) -> usize {
        let device = self.device;
        self.vault
            .read_blocking(move |q| {
                candidates(q, extension, PermissionKind::EncryptedFolder, device)
                    .map_err(Into::into)
            })
            .unwrap()
            .len()
    }
}

fn reason(outcome: Result<impl std::fmt::Debug>) -> String {
    match outcome {
        Err(HolziError::ExtensionInstall { reason }) => reason,
        other => panic!("not refused: {other:?}"),
    }
}

#[test]
fn every_spelling_of_the_category_is_refused_at_preview_and_install() {
    let v = vault();
    for category in [
        "encryptedFolder",
        "encryptedFolders",
        "encrypted_folder",
        "Encrypted-Folders",
    ] {
        let permissions = format!(r#"{{"{category}":[{{"target":"*","operation":"read"}}]}}"#);
        let bytes = build("probe", "1.0.0", &permissions, 1);
        let preview = v.preview(bytes.clone());
        assert!(!preview.signature_valid);
        assert_eq!(
            preview.error,
            Some(BundleRejection::forbidden_permission(category))
        );
        assert_eq!(reason(v.install(&bytes)), "forbidden_permission");
    }
    // Even an empty declaration is refused: the category itself is the request.
    let empty = build("probe", "1.0.0", r#"{"encryptedFolder":[]}"#, 1);
    assert_eq!(reason(v.install(&empty)), "forbidden_permission");
    let installed: i64 = v
        .vault
        .read_blocking(|q| q.query_row("SELECT COUNT(*) FROM extensions", &[], |r| r.get(0)))
        .unwrap()
        .unwrap();
    assert_eq!(installed, 0, "nothing was written");
}

#[test]
fn an_update_that_asks_for_it_is_refused_and_the_installed_version_keeps_running() {
    let v = vault();
    let id = v.install(&build("probe", "1.0.0", "{}", 1)).unwrap();
    let update = build(
        "probe",
        "2.0.0",
        r#"{"encryptedFolder":[{"target":"*"}]}"#,
        1,
    );
    assert_eq!(reason(v.install(&update)), "forbidden_permission");
    let effective = v
        .vault
        .read_blocking(move |q| effective_bundle(q, id).map_err(Into::into))
        .unwrap()
        .unwrap();
    assert_eq!(effective.version, semver::Version::new(1, 0, 0));
}

#[test]
fn a_development_version_that_asks_for_it_does_not_load() {
    let v = vault();
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("haextension")).unwrap();
    std::fs::write(
        dir.path().join("haextension/manifest.json"),
        json!({
            "name": "draft",
            "version": "0.1.0",
            "publicKey": public(3),
            "permissions": { "encrypted_folders": [] }
        })
        .to_string(),
    )
    .unwrap();
    assert_eq!(
        reason(crate::extensions::dev::read_project(dir.path())),
        "forbidden_permission"
    );
    let device = v.device;
    v.vault
        .write_blocking(move |tx| {
            crate::extensions::dev::set_mode(tx, device, true).map_err(Into::into)
        })
        .unwrap();
    assert_eq!(
        reason(crate::extensions::dev::confirm(
            &v.vault,
            dir.path(),
            vec![],
            v.device,
            1
        )),
        "forbidden_permission"
    );
}

#[test]
fn a_grant_belongs_to_exactly_one_extension() {
    let v = vault();
    let probe = v.install(&build("probe", "1.0.0", "{}", 1)).unwrap();
    v.grant(probe);
    assert_eq!(v.folder_grants(probe), 1);

    // The same name from another publisher is another extension.
    let other = v.install(&build("probe", "1.0.0", "{}", 2)).unwrap();
    assert_ne!(other, probe);
    assert_eq!(v.folder_grants(other), 0);

    // A development version of the same key and name has its own id and table.
    let key = PublicKey::parse(&public(1)).unwrap();
    let dev = dev_extension_id(v.device, &key, &ExtensionName::parse("probe").unwrap());
    assert_ne!(dev, probe);
    assert_eq!(v.folder_grants(dev), 0);

    // An update of this very extension keeps what the user gave it; removing it drops it.
    v.install(&build("probe", "1.1.0", "{}", 1)).unwrap();
    assert_eq!(v.folder_grants(probe), 1);
    super::super::remove::remove(&v.vault, probe, true, 2).unwrap();
    assert_eq!(v.folder_grants(probe), 0);
}
