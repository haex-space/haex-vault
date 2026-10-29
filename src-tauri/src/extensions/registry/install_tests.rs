// These tests read registry rows directly to check what the install wrote.
#![allow(clippy::disallowed_methods)]

use std::path::Path;
use std::sync::Arc;

use ed25519_dalek::SigningKey;
use haex_crdt::rusqlite::params;
use haex_crdt::Database;

use super::*;
use crate::extensions::permissions::VAULT_WIDE;
use crate::passwords::test_support::open_test_vault;
use crate::storage::known_devices;
use crate::vault_gate::VaultGate;

/// Test-only publisher keys.
fn key(seed: u8) -> SigningKey {
    SigningKey::from_bytes(&[seed; 32])
}

fn build(name: &str, version: &str, display: &str, permissions: &str, seed: u8) -> Vec<u8> {
    let manifest = haex_bundle::jcs::parse_restricted(&format!(
        r#"{{"name":"{name}","version":"{version}","displayName":"{display}","permissions":{permissions}}}"#
    ))
    .unwrap();
    let files = vec![haex_bundle::Entry {
        path: "index.html".into(),
        data: format!("<!doctype html><title>{version}</title>").into_bytes(),
    }];
    haex_bundle::build_archive(files, manifest, &key(seed)).unwrap()
}

const PERMS_V1: &str = r#"{"filesystem":[{"target":"/home/a/docs","operation":"read"}],"http":[{"target":"https://a.example/*"}]}"#;

struct Vault {
    _dir: tempfile::TempDir,
    db: Database,
    vault: VaultDb,
    device: Uuid,
}

fn vault() -> Vault {
    let (dir, db) = open_test_vault();
    let vault = VaultGate::new().vault_db(Arc::new(db.clone())).unwrap();
    let device = vault
        .read_blocking(|q| known_devices::list_devices(q))
        .unwrap()
        .first()
        .expect("the bootstrap registers this device")
        .vault_device_uuid;
    Vault {
        _dir: dir,
        db,
        vault,
        device,
    }
}

impl Vault {
    fn preview(&self, bytes: &[u8]) -> InstallPreview {
        let bytes = bytes.to_vec();
        self.vault
            .read_blocking(move |q| install_preview(q, &bytes).map_err(Into::into))
            .unwrap()
    }

    fn install(
        &self,
        bytes: &[u8],
        choices: Vec<PermissionChoice>,
        confirm: bool,
    ) -> Result<Installed> {
        install(&self.vault, bytes, choices, confirm, self.device, 1_000)
    }

    fn permissions(&self, extension: Uuid) -> Vec<(String, String, String, bool, Uuid)> {
        let mut rows: Vec<_> = self
            .vault
            .read_blocking(move |q| permission_store::rows_of(q, extension).map_err(Into::into))
            .unwrap()
            .into_iter()
            .map(|r| (r.kind, r.target, r.status, r.declared, r.vault_device_uuid))
            .collect();
        rows.sort();
        rows
    }

    fn effective(&self, extension: Uuid) -> String {
        self.vault
            .read_blocking(move |q| effective_bundle(q, extension).map_err(Into::into))
            .unwrap()
            .unwrap()
            .version
            .to_string()
    }
}

fn choice(kind: &str, action: &str, target: &str, granted: bool) -> PermissionChoice {
    PermissionChoice {
        kind: kind.into(),
        action: action.into(),
        target: target.into(),
        granted,
    }
}

#[test]
fn the_preview_of_a_new_bundle_lists_its_permissions_and_writes_nothing() {
    let v = vault();
    let preview = v.preview(&build("demo", "1.0.0", "Demo", PERMS_V1, 1));
    assert!(preview.signature_valid);
    assert_eq!(preview.name.as_deref(), Some("demo"));
    assert_eq!(preview.display_name.as_deref(), Some("Demo"));
    assert_eq!(preview.declared.len(), 2);
    let fs = preview
        .declared
        .iter()
        .find(|d| d.kind == "filesystem")
        .unwrap();
    assert!(!fs.device_scoped, "the file system holds on every device");
    assert_eq!(preview.existing, None);
    assert!(!preview.same_name_other_publisher);
    let fingerprint = preview.publisher_fingerprint.unwrap();
    assert_eq!(fingerprint.split(' ').count(), 9, "{fingerprint}");
    let rows: i64 = v
        .db
        .with_connection(|c| Ok(c.query_row("SELECT COUNT(*) FROM extensions", [], |r| r.get(0))?))
        .unwrap();
    assert_eq!(rows, 0);
}

#[test]
fn a_refused_bundle_is_a_preview_with_its_error_and_an_install_error() {
    let v = vault();
    let mut bytes = build("demo", "1.0.0", "Demo", "{}", 1);
    let last = bytes.len() - 30;
    bytes[last] ^= 0xff;
    let preview = v.preview(&bytes);
    assert!(!preview.signature_valid);
    assert!(preview.error.is_some());
    assert!(matches!(
        v.install(&bytes, vec![], false),
        Err(HolziError::ExtensionInstall { .. })
    ));
}

#[test]
fn ticked_permissions_are_granted_unticked_ask_and_hold_on_every_device() {
    let v = vault();
    let installed = v
        .install(
            &build("demo", "1.0.0", "Demo", PERMS_V1, 1),
            vec![
                choice("filesystem", "read", "/home/a/docs", true),
                choice("web", "*", "https://a.example/*", false),
            ],
            false,
        )
        .unwrap();
    let rows = v.permissions(installed.ids.extension_id);
    assert_eq!(
        rows,
        vec![
            (
                "filesystem".into(),
                "/home/a/docs".into(),
                "granted".into(),
                true,
                VAULT_WIDE
            ),
            (
                "web".into(),
                "https://a.example/*".into(),
                "ask".into(),
                true,
                VAULT_WIDE
            ),
        ]
    );
}

#[test]
fn only_a_shell_permission_is_remembered_for_this_device() {
    let v = vault();
    let perms = r#"{"filesystem":[{"target":"/home/a/docs","operation":"read"}],"shell":[{"target":"/usr/bin/git"}]}"#;
    let preview = v.preview(&build("demo", "1.0.0", "Demo", perms, 1));
    let scoped: Vec<(&str, bool)> = preview
        .declared
        .iter()
        .map(|d| (d.kind.as_str(), d.device_scoped))
        .collect();
    assert_eq!(scoped, vec![("filesystem", false), ("shell", true)]);
    let installed = v
        .install(
            &build("demo", "1.0.0", "Demo", perms, 1),
            vec![
                choice("filesystem", "read", "/home/a/docs", true),
                choice("shell", "execute", "/usr/bin/git", true),
            ],
            false,
        )
        .unwrap();
    let scopes: Vec<(String, Uuid)> = v
        .permissions(installed.ids.extension_id)
        .into_iter()
        .map(|row| (row.0, row.4))
        .collect();
    assert_eq!(
        scopes,
        vec![
            ("filesystem".into(), VAULT_WIDE),
            ("shell".into(), v.device)
        ]
    );
}

#[test]
fn an_update_deletes_dropped_declarations_keeps_runtime_rows_and_puts_only_new_ones_before_the_user(
) {
    let v = vault();
    let first = v
        .install(&build("demo", "1.0.0", "Demo", PERMS_V1, 1), vec![], false)
        .unwrap();
    let ext = first.ids.extension_id;
    // A permission remembered at run time, not declared.
    let vault = v.vault.clone();
    vault
        .write_blocking(move |tx| {
            permission_store::put(
                tx,
                ext,
                &NewPermission {
                    kind: "notifications",
                    action: "show",
                    target: "*",
                    status: "denied",
                    declared: false,
                    vault_device_uuid: VAULT_WIDE,
                },
                5,
            )
            .map(drop)
            .map_err(Into::into)
        })
        .unwrap();

    let v2 = build(
        "demo",
        "1.1.0",
        "Demo 2",
        r#"{"http":[{"target":"https://a.example/*"},{"target":"https://b.example/*"}],"notifications":[{"target":"*","operation":"show"}]}"#,
        1,
    );
    let preview = v.preview(&v2);
    let existing = preview.existing.unwrap();
    assert_eq!(existing.version, "1.0.0");
    assert!(!existing.is_downgrade);
    let new: Vec<_> = existing
        .new_permissions
        .iter()
        .map(|p| p.target.as_str())
        .collect();
    assert_eq!(new, ["https://b.example/*"]);

    v.install(
        &v2,
        vec![choice("web", "*", "https://b.example/*", true)],
        false,
    )
    .unwrap();
    assert_eq!(
        v.permissions(ext),
        vec![
            (
                "notifications".into(),
                "*".into(),
                "denied".into(),
                true,
                VAULT_WIDE
            ),
            (
                "web".into(),
                "https://a.example/*".into(),
                "ask".into(),
                true,
                VAULT_WIDE
            ),
            (
                "web".into(),
                "https://b.example/*".into(),
                "granted".into(),
                true,
                VAULT_WIDE
            ),
        ],
        "filesystem was dropped, the runtime row became declared with its state"
    );
    assert_eq!(v.effective(ext), "1.1.0");
    let display: Option<String> =
        v.db.with_connection(|c| {
            Ok(c.query_row(
                "SELECT display_name FROM extensions WHERE id = ?1",
                params![ext.to_string()],
                |r| r.get(0),
            )?)
        })
        .unwrap();
    assert_eq!(display.as_deref(), Some("Demo 2"));
}

#[test]
fn a_downgrade_needs_a_confirmation_and_then_retires_the_newer_bundle() {
    let v = vault();
    let ext = v
        .install(&build("demo", "2.0.0", "Two", "{}", 1), vec![], false)
        .unwrap()
        .ids
        .extension_id;
    let older = build("demo", "1.0.0", "One", "{}", 1);
    assert!(v.preview(&older).existing.unwrap().is_downgrade);
    assert!(matches!(
        v.install(&older, vec![], false),
        Err(HolziError::ExtensionInstall { ref reason }) if reason == "downgrade_not_confirmed"
    ));
    assert_eq!(v.effective(ext), "2.0.0");

    v.install(&older, vec![], true).unwrap();
    assert_eq!(v.effective(ext), "1.0.0");

    // Installing 2.0.0 again brings it back (retired = 0).
    v.install(&build("demo", "2.0.0", "Two", "{}", 1), vec![], false)
        .unwrap();
    assert_eq!(v.effective(ext), "2.0.0");
}

#[test]
fn the_same_name_from_another_publisher_is_another_extension_with_a_hint() {
    let v = vault();
    let first = v
        .install(&build("demo", "1.0.0", "Demo", "{}", 1), vec![], false)
        .unwrap();
    let other = build("demo", "1.0.0", "Demo", "{}", 2);
    let preview = v.preview(&other);
    assert!(preview.same_name_other_publisher);
    assert_eq!(preview.existing, None);
    let second = v.install(&other, vec![], false).unwrap();
    assert_ne!(first.ids.extension_id, second.ids.extension_id);
}

#[test]
fn the_fingerprint_shows_the_first_and_last_four_groups() {
    let key = "3614253f84ba66a8faa168d317a6979992979a3e7ad15ae83ca2823f5ca97d34";
    assert_eq!(
        publisher_fingerprint(key),
        "3614 253f 84ba 66a8 … 3ca2 823f 5ca9 7d34"
    );
}

fn effective_id(v: &Vault, extension: Uuid) -> Uuid {
    v.vault
        .read_blocking(move |q| effective_bundle(q, extension).map_err(Into::into))
        .unwrap()
        .unwrap()
        .bundle_id
}

#[test]
fn the_same_version_with_other_content_replaces_only_after_a_confirmation() {
    let v = vault();
    let first = v
        .install(&build("demo", "1.0.0", "First", "{}", 1), vec![], false)
        .unwrap()
        .ids;
    let same = build("demo", "1.0.0", "First", "{}", 1);
    assert!(
        !v.preview(&same).replaces_same_version,
        "the same bundle again"
    );
    let other = build("demo", "1.0.0", "Other", "{}", 1);
    assert!(v.preview(&other).replaces_same_version);
    assert!(matches!(
        v.install(&other, vec![], false),
        Err(HolziError::ExtensionInstall { ref reason }) if reason == "replace_not_confirmed"
    ));
    assert_eq!(effective_id(&v, first.extension_id), first.bundle_id);

    let replaced = v.install(&other, vec![], true).unwrap().ids;
    assert_eq!(effective_id(&v, first.extension_id), replaced.bundle_id);
    assert!(
        !v.preview(&other).replaces_same_version,
        "the first one is retired"
    );
}

#[test]
fn after_a_removal_the_same_version_is_a_fresh_install() {
    let v = vault();
    let first = v
        .install(&build("demo", "1.0.0", "First", "{}", 1), vec![], false)
        .unwrap()
        .ids;
    super::super::remove::remove(&v.vault, first.extension_id, false, 2_000).unwrap();
    let other = build("demo", "1.0.0", "Other", "{}", 1);
    assert!(
        !v.preview(&other).replaces_same_version,
        "removing deletes the bundles"
    );
    let installed = v.install(&other, vec![], false).unwrap().ids;
    assert_eq!(effective_id(&v, first.extension_id), installed.bundle_id);
}

#[test]
fn the_preview_names_the_kinds_of_link_the_extension_opens() {
    let v = vault();
    let mail = std::fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/extension_e2e/mail-opener.xt"),
    )
    .unwrap();
    let preview = v.preview(&mail);
    assert_eq!(preview.openers, ["mail.compose"]);
    assert!(
        preview.declared.is_empty(),
        "opening links is no permission (FR-010)"
    );

    assert!(v
        .preview(&build("plain", "1.0.0", "Plain", "{}", 1))
        .openers
        .is_empty());
}
