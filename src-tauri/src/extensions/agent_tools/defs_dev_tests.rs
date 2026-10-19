//! The tools of a development version (spec 018 US4, FR-020): offered while developer mode is on,
//! from the project's manifest as it reads now, with the developer's own confirmation and marked
//! as a development version.

use std::sync::Arc;

use serde_json::{json, Value};
use uuid::Uuid;

use super::defs::load;
use crate::extensions::dev::{confirm, set_mode, unload};
use crate::extensions::registry::install::PermissionChoice;
use crate::passwords::test_support::open_test_vault;
use crate::storage::known_devices;
use crate::vault_gate::{VaultDb, VaultGate};

/// A key only these tests use, derived from a public seed.
fn key() -> String {
    use sha2::{Digest, Sha256};
    let seed = Sha256::digest(b"holzi spec 018 development tool tests only");
    ed25519_dalek::SigningKey::from_bytes(&seed.into())
        .verifying_key()
        .as_bytes()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

struct Setup {
    _dir: tempfile::TempDir,
    vault: VaultDb,
    device: Uuid,
}

fn setup() -> Setup {
    let (dir, db) = open_test_vault();
    let vault = VaultGate::new().vault_db(Arc::new(db)).unwrap();
    let device = vault
        .read_blocking(|q| known_devices::list_devices(q))
        .unwrap()[0]
        .vault_device_uuid;
    Setup {
        _dir: dir,
        vault,
        device,
    }
}

impl Setup {
    fn mode(&self, enabled: bool) {
        let device = self.device;
        self.vault
            .write_blocking(move |tx| set_mode(tx, device, enabled).map_err(Into::into))
            .unwrap();
    }

    fn model_names(&self) -> Vec<(String, bool)> {
        let device = self.device;
        self.vault
            .read_blocking(move |q| load(q, device).map_err(Into::into))
            .unwrap()
            .into_iter()
            .map(|def| (def.model_name, def.dev))
            .collect()
    }
}

fn tool(name: &str, effect: &str) -> Value {
    json!({
        "name": name,
        "title": { "de": format!("{name} de"), "en": format!("{name} en") },
        "description": format!("Does {name}"),
        "inputSchema": { "type": "object" },
        "effect": effect,
        "examples": { "de": ["Mach es"], "en": ["Do it"] }
    })
}

/// A project folder with `haextension/manifest.json` that offers `tools`.
fn project(tools: &[Value]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("haextension")).unwrap();
    write_manifest(&dir, tools);
    dir
}

fn write_manifest(dir: &tempfile::TempDir, tools: &[Value]) {
    std::fs::write(
        dir.path().join("haextension/manifest.json"),
        json!({
            "name": "draft",
            "version": "0.1.0",
            "publicKey": key(),
            "displayName": "Draft",
            "tools": tools,
        })
        .to_string(),
    )
    .unwrap();
}

fn choice(name: &str, effect: &str, granted: bool) -> PermissionChoice {
    PermissionChoice {
        kind: "agentTool".into(),
        action: effect.into(),
        target: name.into(),
        granted,
    }
}

#[test]
fn a_development_version_offers_its_confirmed_tools_marked_as_such() {
    let s = setup();
    s.mode(true);
    let dir = project(&[
        tool("peek", "read"),
        tool("edit", "change"),
        tool("wipe", "destructive"),
    ]);
    confirm(
        &s.vault,
        dir.path(),
        vec![
            choice("peek", "read", true),
            choice("edit", "change", false),
        ],
        s.device,
        1,
    )
    .unwrap();

    let names = s.model_names();

    // `peek` is confirmed; `edit` and `wipe` are not ticked, so they are on "ask": offered, and
    // every call of them needs the developer's consent.
    assert_eq!(
        names,
        [
            ("x_draft_peek".to_owned(), true),
            ("x_draft_edit".to_owned(), true),
            ("x_draft_wipe".to_owned(), true)
        ]
    );
}

#[test]
fn a_development_version_is_offered_only_while_developer_mode_is_on() {
    let s = setup();
    s.mode(true);
    let dir = project(&[tool("peek", "read")]);
    confirm(
        &s.vault,
        dir.path(),
        vec![choice("peek", "read", true)],
        s.device,
        1,
    )
    .unwrap();
    assert_eq!(s.model_names().len(), 1);

    s.mode(false);
    assert!(s.model_names().is_empty());
    s.mode(true);
    assert_eq!(s.model_names().len(), 1);
}

#[test]
fn the_tools_are_read_from_the_project_as_it_is_now_and_a_missing_project_offers_none() {
    let s = setup();
    s.mode(true);
    let dir = project(&[tool("peek", "read"), tool("look", "read")]);
    confirm(
        &s.vault,
        dir.path(),
        vec![choice("peek", "read", true), choice("look", "read", true)],
        s.device,
        1,
    )
    .unwrap();
    assert_eq!(s.model_names().len(), 2);

    // The developer removed a tool from the manifest: it is gone from the offer at once.
    write_manifest(&dir, &[tool("peek", "read")]);
    assert_eq!(s.model_names(), [("x_draft_peek".to_owned(), true)]);

    // The folder disappeared: nothing is offered, and nothing fails.
    let path = dir.path().to_path_buf();
    drop(dir);
    assert!(!path.exists());
    assert!(s.model_names().is_empty());
}

#[test]
fn the_switch_off_row_and_unloading_withdraw_the_tools() {
    let s = setup();
    s.mode(true);
    let dir = project(&[tool("peek", "read")]);
    let id = confirm(
        &s.vault,
        dir.path(),
        vec![choice("peek", "read", true)],
        s.device,
        1,
    )
    .unwrap();

    let switch = Uuid::new_v4().to_string();
    let row = switch.clone();
    s.vault
        .write_blocking(move |tx| {
            tx.execute(
                "INSERT INTO dev_extension_permissions_no_sync \
                 (id, extension_id, kind, action, target, status, declared, vault_device_uuid, \
                  updated_at) \
                 VALUES (?2, ?1, 'agentTool', 'read', '*', 'denied', 0, \
                  '00000000-0000-0000-0000-000000000000', 1)",
                haex_crdt::rusqlite::params![id.to_string(), row],
            )
            .map(drop)
        })
        .unwrap();
    assert!(s.model_names().is_empty(), "denied before granted");

    s.vault
        .write_blocking(move |tx| {
            tx.execute(
                "DELETE FROM dev_extension_permissions_no_sync WHERE id = ?1",
                haex_crdt::rusqlite::params![switch],
            )
            .map(drop)
        })
        .unwrap();
    assert_eq!(s.model_names().len(), 1);

    unload(&s.vault, id).unwrap();
    assert!(s.model_names().is_empty());
}

#[test]
fn installed_tools_and_development_tools_are_offered_together_with_their_own_marks() {
    use crate::extensions::registry::install::install;

    let s = setup();
    s.mode(true);
    let bytes = std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/extension_e2e/agent-tools.xt"),
    )
    .unwrap();
    let choices = ["count_entries", "add_entry", "slow_echo"]
        .into_iter()
        .map(|name| {
            choice(
                name,
                if name == "add_entry" {
                    "change"
                } else {
                    "read"
                },
                true,
            )
        })
        .collect();
    install(&s.vault, &bytes, choices, false, s.device, 1).unwrap();
    let dir = project(&[tool("peek", "read")]);
    confirm(
        &s.vault,
        dir.path(),
        vec![choice("peek", "read", true)],
        s.device,
        2,
    )
    .unwrap();

    let names = s.model_names();

    assert_eq!(names.iter().filter(|(_, dev)| !dev).count(), 3, "{names:?}");
    assert_eq!(
        names.iter().filter(|(_, dev)| *dev).collect::<Vec<_>>(),
        [&("x_draft_peek".to_owned(), true)]
    );
}
