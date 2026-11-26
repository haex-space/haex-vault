use std::sync::Arc;

use serde_json::json;

use super::{tool_names_line, with_names, HEAD};
use crate::chat::tools::action_bridge::ActionBridge;
use crate::chat::tools::action_tool::{ActionTool, AgentActionDef};
use crate::chat::tools::ask_user::AskUserTool;
use crate::chat::tools::find_actions::FindActionsTool;
use crate::chat::tools::ToolRegistry;
use crate::mcp_server::grants::{BrowserRights, MemoryLevel, Rights};

fn action(id: &str, core: bool) -> AgentActionDef {
    serde_json::from_value(json!({
        "toolName": id.replace('.', "_"),
        "actionId": id,
        "description": id,
        "inputSchema": { "type": "object" },
        "effect": "read",
        "core": core,
        "titles": { "de": "", "en": "" },
    }))
    .expect("test action definition")
}

fn registry() -> ToolRegistry {
    let defs = vec![
        action("wm.apps.list", true),
        action("memory.add", false),
        action("settings.models.list", false),
    ];
    let mut registry = ToolRegistry::new();
    for def in &defs {
        registry.register(Arc::new(ActionTool::new(
            def.clone(),
            ActionBridge::default(),
        )));
    }
    registry.register(Arc::new(FindActionsTool::new(defs)));
    registry.register(Arc::new(AskUserTool));
    registry
}

#[test]
fn the_line_names_what_a_search_can_add_in_the_registry_s_order() {
    assert_eq!(
        tool_names_line(&registry(), |_| true).as_deref(),
        Some(format!("{HEAD} memory_add, settings_models_list.").as_str())
    );
}

#[test]
fn the_line_leaves_out_what_the_turn_may_not_use() {
    assert_eq!(
        tool_names_line(&registry(), |tool| tool.name() != "memory_add").as_deref(),
        Some(format!("{HEAD} settings_models_list.").as_str())
    );
    assert_eq!(tool_names_line(&registry(), |_| false), None);

    let mut registry = registry();
    let mut screenshot = action("browser.page.screenshot", false);
    screenshot.scope = "browser".into();
    registry.register(Arc::new(ActionTool::new(
        screenshot,
        ActionBridge::default(),
    )));
    let rights = Rights {
        browser: Some(BrowserRights::default()),
        ..Rights::default()
    };
    let text_names = with_names(None, &registry, &rights, MemoryLevel::Off, false);
    assert!(text_names.is_none_or(|line| !line.contains("browser_page_screenshot")));
    let image_names = with_names(None, &registry, &rights, MemoryLevel::Off, true);
    assert!(image_names.is_some_and(|line| line.contains("browser_page_screenshot")));
}
