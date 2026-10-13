use std::sync::Arc;

use async_trait::async_trait;
use serde_json::json;
use uuid::Uuid;

use super::defs::ExtensionToolDef;
use super::registration::{register_extension_tools, touches_tools};
use super::slot::McpLink;
use super::tool::{Links, SOURCE};
use crate::chat::tools::ask_user::AskUserTool;
use crate::chat::tools::ToolRegistry;
use crate::extensions::bridge::frames::FrameSession;
use crate::extensions::bundle::{ManifestTool, ToolEffect};

struct NoFrame;

#[async_trait]
impl Links for NoFrame {
    async fn open(&self, _: Uuid) -> Result<(Arc<FrameSession>, Arc<McpLink>), String> {
        Err("none".to_owned())
    }
}

fn def(model_name: &str) -> ExtensionToolDef {
    ExtensionToolDef {
        extension_id: Uuid::new_v4(),
        extension_title: "Mail".to_owned(),
        tool: ManifestTool {
            name: "count".to_owned(),
            title_de: "Zählen".to_owned(),
            title_en: "Count".to_owned(),
            description: "Counts".to_owned(),
            input_schema: json!({ "type": "object" }),
            effect: ToolEffect::Read,
            examples_de: vec![],
            examples_en: vec![],
            supported: true,
        },
        model_name: model_name.to_owned(),
        always_ask: false,
        dev: false,
    }
}

fn names(registry: &ToolRegistry) -> Vec<String> {
    let mut names: Vec<_> = registry.iter().map(|t| t.name().to_owned()).collect();
    names.sort();
    names
}

/// FR-010: what the vault now says replaces the extension tools, and nothing else changes.
#[test]
fn the_extension_tools_are_replaced_and_other_tools_stay() {
    let mut registry = ToolRegistry::new();
    registry.register(Arc::new(AskUserTool));
    let links: Arc<dyn Links> = Arc::new(NoFrame);

    register_extension_tools(
        &mut registry,
        vec![def("x_mail_count"), def("x_mail_old")],
        &links,
    );
    assert_eq!(names(&registry), ["ask_user", "x_mail_count", "x_mail_old"]);

    register_extension_tools(&mut registry, vec![def("x_mail_count")], &links);
    assert_eq!(names(&registry), ["ask_user", "x_mail_count"]);
    assert_eq!(registry.get("x_mail_count").unwrap().source(), SOURCE);

    register_extension_tools(&mut registry, vec![], &links);
    assert_eq!(names(&registry), ["ask_user"]);
}

#[test]
fn only_changes_to_extensions_and_their_permissions_refresh_the_tools() {
    for table in [
        "extensions",
        "extension_bundles",
        "extension_device_status",
        "extension_permissions",
    ] {
        assert!(touches_tools(&[table.to_owned()]), "{table}");
    }
    assert!(!touches_tools(&["chat_messages".to_owned()]));
}
