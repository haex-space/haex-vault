use std::sync::{Arc, Mutex};

use serde_json::{json, Value};
use uuid::Uuid;

use super::commands::{deliver, to_frame, MCP_MESSAGE};
use crate::extensions::agent_tools::test_support::serve_json;
use crate::extensions::bridge::dispatch::Emit;
use crate::extensions::bridge::frames::FrameRegistry;

#[derive(Clone, Default)]
struct Recorded(Arc<Mutex<Vec<(String, Value)>>>);

impl Emit for Recorded {
    fn emit(&self, event: &str, payload: Value) {
        self.0.lock().unwrap().push((event.to_owned(), payload));
    }
}

#[test]
fn holzis_messages_go_to_the_frame_as_events() {
    let recorded = Recorded::default();
    let send = to_frame(recorded.clone(), "frame-1".to_owned());
    send(json!({ "jsonrpc": "2.0", "id": 1, "method": "ping" }));
    assert_eq!(
        recorded.0.lock().unwrap().as_slice(),
        [(
            MCP_MESSAGE.to_owned(),
            json!({ "frame": "frame-1",
                    "message": { "jsonrpc": "2.0", "id": 1, "method": "ping" } })
        )]
    );
}

#[test]
fn a_message_for_an_unknown_frame_is_dropped() {
    let frames = FrameRegistry::default();
    assert!(!deliver(
        &frames,
        "nope",
        json!({ "jsonrpc": "2.0", "id": 1, "result": {} })
    ));
}

/// The extension's answers reach the link of their own frame session only.
#[tokio::test]
async fn the_extensions_answers_reach_the_link_of_its_frame() {
    let frames = Arc::new(FrameRegistry::default());
    let session = frames.open(Uuid::new_v4(), Uuid::new_v4(), "tab-1");
    let other = frames.open(Uuid::new_v4(), Uuid::new_v4(), "tab-2");
    let (relay, frame) = (Arc::clone(&frames), session.frame.clone());
    let to_extension = serve_json(move |message| {
        assert!(deliver(&relay, &frame, message));
    });

    let link = session
        .mcp
        .link(to_extension)
        .await
        .expect("the link opens");
    assert_eq!(link.tools(), ["echo", "slow", "fail"]);
    assert_eq!(other.mcp.calls_in_flight(), 0);
    assert!(
        tokio::time::timeout(
            std::time::Duration::from_millis(200),
            other.mcp.link(|_| {})
        )
        .await
        .is_err(),
        "the other frame got no answers"
    );
}

/// The test extension installed with its three supported tools ticked.
fn installed() -> (tempfile::TempDir, crate::vault_gate::VaultDb, Uuid, Uuid) {
    use crate::extensions::registry::install::{install, PermissionChoice};

    let (dir, db) = crate::passwords::test_support::open_test_vault();
    let vault = crate::vault_gate::VaultGate::new()
        .vault_db(Arc::new(db))
        .unwrap();
    let device = vault
        .read_blocking(|q| crate::storage::known_devices::list_devices(q))
        .unwrap()[0]
        .vault_device_uuid;
    let bytes = std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/extension_e2e/agent-tools.xt"),
    )
    .unwrap();
    let choices = ["count_entries", "add_entry", "slow_echo"]
        .into_iter()
        .map(|target| PermissionChoice {
            kind: "agentTool".to_owned(),
            action: if target == "add_entry" {
                "change"
            } else {
                "read"
            }
            .to_owned(),
            target: target.to_owned(),
            granted: true,
        })
        .collect();
    let extension = install(&vault, &bytes, choices, false, device, 1)
        .unwrap()
        .ids
        .extension_id;
    (dir, vault, device, extension)
}

fn offered(vault: &crate::vault_gate::VaultDb, device: Uuid) -> usize {
    vault
        .read_blocking(move |q| super::defs::load(q, device).map_err(Into::into))
        .unwrap()
        .len()
}

/// Spec 018 (US3, SC-006): switching an extension's tools off offers none of them, switching on
/// again offers them as confirmed; the extension itself is untouched.
#[test]
fn the_switch_takes_an_extensions_tools_from_the_agent_and_gives_them_back() {
    use super::commands::set_available;

    let (_dir, vault, device, extension) = installed();
    assert_eq!(offered(&vault, device), 3);

    set_available(&vault, extension, false, 2).unwrap();
    assert_eq!(offered(&vault, device), 0);
    let rows = vault
        .read_blocking(move |q| {
            crate::extensions::permissions::store::rows_of(q, extension).map_err(Into::into)
        })
        .unwrap();
    assert!(rows
        .iter()
        .any(|r| r.kind == "agentTool" && r.target == "*" && r.status == "denied" && !r.declared));

    set_available(&vault, extension, false, 3).unwrap();
    set_available(&vault, extension, true, 4).unwrap();
    assert_eq!(offered(&vault, device), 3);
    set_available(&vault, extension, true, 5).unwrap();
    assert_eq!(offered(&vault, device), 3);
}

/// The settings list an extension's tools with their titles and whether holzi can offer them.
#[test]
fn the_settings_list_the_tools_with_title_and_support() {
    use crate::extensions::commands::permissions::list;
    use crate::extensions::host::ExtensionHost;

    let (_dir, vault, _device, extension) = installed();
    let views = list(&vault, &ExtensionHost::default(), extension).unwrap();
    let tool = |name: &str| {
        views
            .iter()
            .find(|v| v.kind == "agentTool" && v.target == name)
            .unwrap_or_else(|| panic!("{name} listed"))
    };
    assert_eq!(
        tool("count_entries").title_de.as_deref(),
        Some("Einträge zählen")
    );
    assert_eq!(tool("count_entries").supported, Some(true));
    assert_eq!(tool("count_entries").status, "granted");
    assert_eq!(tool("optional_note").supported, Some(false));
    assert!(views
        .iter()
        .filter(|v| v.kind != "agentTool")
        .all(|v| v.title_de.is_none()));
}
