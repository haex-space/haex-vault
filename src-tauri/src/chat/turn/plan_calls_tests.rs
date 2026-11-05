use std::sync::Arc;

use serde_json::json;

use super::{callable, denied_answer, unknown_tool_answer};
use crate::adapters::types::ToolSpec;
use crate::chat::tools::action_bridge::ActionBridge;
use crate::chat::tools::action_tool::{ActionTool, AgentActionDef};
use crate::chat::tools::cli::CliTool;
use crate::chat::tools::offer::tool_spec;
use crate::chat::tools::Tool;

fn spec(name: &str) -> ToolSpec {
    ToolSpec {
        name: name.to_owned(),
        description: String::new(),
        input_schema: json!({ "type": "object" }),
    }
}

fn action(action_id: &str) -> Arc<dyn Tool> {
    let def: AgentActionDef = serde_json::from_value(json!({
        "toolName": action_id.replace('.', "_"),
        "actionId": action_id,
        "description": "d",
        "inputSchema": { "type": "object" },
        "effect": "write",
    }))
    .expect("a valid definition");
    Arc::new(ActionTool::new(def, ActionBridge::default()))
}

#[test]
fn an_action_is_callable_only_when_the_step_offered_it() {
    let tool = action("contacts.update");
    assert!(!callable(tool.as_ref(), &[spec("contacts_search")]));
    assert!(callable(tool.as_ref(), &[tool_spec(tool.as_ref())]));
}

#[test]
fn the_command_tool_is_callable_without_being_offered() {
    assert!(callable(&CliTool::default(), &[]));
}

#[test]
fn an_unknown_tool_names_the_closest_tools_or_the_search() {
    let offered = [spec("x_mail_find_messages"), spec("find_actions")];
    let answer = unknown_tool_answer("x_mail_list_messages", &offered);
    assert_eq!(
        answer,
        "unknown tool: x_mail_list_messages. Call one of the closest offered tools instead: \
         x_mail_find_messages."
    );
    let answer = unknown_tool_answer("calendar_add", &offered);
    assert!(
        answer.ends_with("find_actions finds holzi's other actions."),
        "{answer}"
    );
}

#[test]
fn a_declined_call_keeps_the_marker_and_says_not_to_repeat_it() {
    let answer = denied_answer();
    assert!(answer.starts_with("denied_by_user: "), "{answer}");
    assert!(answer.contains("Do not propose it again"), "{answer}");
}
