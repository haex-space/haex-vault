//! Tools of the manifest at install and update (spec 018 US2, T015): each is an `agentTool`
//! permission with its effect and titles; an update puts only new tools and changed effects
//! before the user (FR-002), and a dropped tool's row goes.

use std::sync::Arc;

use ed25519_dalek::SigningKey;

use super::*;
use crate::passwords::test_support::open_test_vault;
use crate::storage::known_devices;
use crate::vault_gate::VaultGate;

/// Returns the fixed signing key shared by all bundle versions in these tests.
fn key() -> SigningKey {
    SigningKey::from_bytes(&[7; 32])
}

/// Builds a tool's manifest JSON with the supplied name, effect and input schema.
fn tool(name: &str, effect: &str, schema: &str) -> String {
    format!(
        r#"{{"name":"{name}","title":{{"de":"{name} de","en":"{name} en"}},"description":"Does {name}.","inputSchema":{schema},"effect":"{effect}","examples":{{"de":["{name}?"],"en":["{name}?"]}}}}"#
    )
}

const OBJECT: &str = r#"{"type":"object"}"#;

/// Builds a signed mail extension bundle with the given version and tool declarations.
fn build(version: &str, tools: &[String]) -> Vec<u8> {
    let manifest = haex_bundle::jcs::parse_restricted(&format!(
        r#"{{"name":"mail","version":"{version}","permissions":{{}},"tools":[{}]}}"#,
        tools.join(",")
    ))
    .unwrap();
    let files = vec![haex_bundle::Entry {
        path: "index.html".into(),
        data: version.as_bytes().to_vec(),
    }];
    haex_bundle::build_archive(files, manifest, &key()).unwrap()
}

struct Vault {
    _dir: tempfile::TempDir,
    vault: VaultDb,
    device: Uuid,
}

/// Opens a temporary test vault and retrieves its bootstrapped device UUID.
fn vault() -> Vault {
    let (dir, db) = open_test_vault();
    let vault = VaultGate::new().vault_db(Arc::new(db)).unwrap();
    let device = vault
        .read_blocking(|q| known_devices::list_devices(q))
        .unwrap()
        .first()
        .expect("the bootstrap registers this device")
        .vault_device_uuid;
    Vault {
        _dir: dir,
        vault,
        device,
    }
}

impl Vault {
    /// Reads an install or update preview from bundle bytes without installing them.
    fn preview(&self, bytes: &[u8]) -> InstallPreview {
        let bytes = bytes.to_vec();
        self.vault
            .read_blocking(move |q| install_preview(q, &bytes).map_err(Into::into))
            .unwrap()
    }

    /// Installs a bundle with the supplied permission choices and returns its extension UUID.
    fn install(&self, bytes: &[u8], choices: Vec<PermissionChoice>) -> Uuid {
        install(&self.vault, bytes, choices, false, self.device, 1_000)
            .unwrap()
            .ids
            .extension_id
    }

    /// `(action, target, status)` of every `agentTool` row, sorted.
    fn tool_rows(&self, extension: Uuid) -> Vec<(String, String, String)> {
        let mut rows: Vec<_> = self
            .vault
            .read_blocking(move |q| permission_store::rows_of(q, extension).map_err(Into::into))
            .unwrap()
            .into_iter()
            .filter(|r| r.kind == "agentTool")
            .map(|r| (r.action, r.target, r.status))
            .collect();
        rows.sort();
        rows
    }
}

/// Returns the action and target of each permission requiring confirmation in an update.
fn new_tools(preview: &InstallPreview) -> Vec<(String, String)> {
    preview
        .existing
        .as_ref()
        .expect("an update")
        .new_permissions
        .iter()
        .map(|p| (p.action.clone(), p.target.clone()))
        .collect()
}

/// Creates a granted choice for the named tool and effect.
fn ticked(effect: &str, name: &str) -> PermissionChoice {
    PermissionChoice {
        kind: "agentTool".into(),
        action: effect.into(),
        target: name.into(),
        granted: true,
    }
}

/// Checks that install previews expose tool names, effects, localized titles and vault scope.
#[test]
fn the_preview_lists_each_tool_with_its_effect_and_titles() {
    let v = vault();
    let preview = v.preview(&build(
        "1.0.0",
        &[
            tool(
                "count_unread",
                "read",
                r#"{"type":"object","properties":{"role":{"enum":["inbox","all"]}}}"#,
            ),
            tool("delete_all", "destructive", OBJECT),
        ],
    ));
    let rows: Vec<_> = preview
        .declared
        .iter()
        .map(|d| {
            (
                d.kind.as_str(),
                d.action.as_str(),
                d.target.as_str(),
                d.title_de.as_deref(),
                d.title_en.as_deref(),
                d.device_scoped,
                d.supported,
            )
        })
        .collect();
    assert_eq!(
        rows,
        [
            (
                "agentTool",
                "read",
                "count_unread",
                Some("count_unread de"),
                Some("count_unread en"),
                false,
                Some(false),
            ),
            (
                "agentTool",
                "destructive",
                "delete_all",
                Some("delete_all de"),
                Some("delete_all en"),
                false,
                Some(true)
            ),
        ]
    );
}

/// Checks selected tools are granted while unselected tools retain the ask status.
#[test]
fn ticked_tools_are_granted_and_unticked_ones_ask() {
    let v = vault();
    let ext = v.install(
        &build(
            "1.0.0",
            &[
                tool("count_unread", "read", OBJECT),
                tool("add_entry", "change", OBJECT),
            ],
        ),
        vec![ticked("read", "count_unread")],
    );
    assert_eq!(
        v.tool_rows(ext),
        [
            ("change".into(), "add_entry".into(), "ask".into()),
            ("read".into(), "count_unread".into(), "granted".into()),
        ]
    );
}

/// Checks updates request new tools and changed effects, preserve schema-only grants and
/// remove permissions for dropped tools or obsolete effects.
#[test]
fn an_update_asks_only_for_new_tools_and_changed_effects() {
    let v = vault();
    let ext = v.install(
        &build(
            "1.0.0",
            &[
                tool("count_unread", "read", OBJECT),
                tool("add_entry", "read", OBJECT),
                tool("old_tool", "read", OBJECT),
            ],
        ),
        vec![
            ticked("read", "count_unread"),
            ticked("read", "add_entry"),
            ticked("read", "old_tool"),
        ],
    );

    let with_input = r#"{"type":"object","properties":{"role":{"type":"string"}}}"#;
    let update = build(
        "1.1.0",
        &[
            tool("count_unread", "read", with_input),
            tool("add_entry", "change", OBJECT),
            tool("new_tool", "read", OBJECT),
        ],
    );
    let mut asked = new_tools(&v.preview(&update));
    asked.sort();
    assert_eq!(
        asked,
        [
            ("change".into(), "add_entry".into()),
            ("read".into(), "new_tool".into()),
        ],
        "a changed input asks nothing (FR-002)"
    );

    v.install(&update, vec![ticked("read", "new_tool")]);
    assert_eq!(
        v.tool_rows(ext),
        [
            ("change".into(), "add_entry".into(), "ask".into()),
            ("read".into(), "count_unread".into(), "granted".into()),
            ("read".into(), "new_tool".into(), "granted".into()),
        ],
        "the dropped tool and the old effect are gone"
    );
}
