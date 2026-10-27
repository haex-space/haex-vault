//! One tool call of an external agent (spec 021 FR-014, FR-015, contracts/mcp-surface.md): checked
//! again against the agent's rights, run by the window's action runner as caller
//! `externalAgent`, and turned into an MCP result. A refusal never says whether a target exists.

use rmcp::model::{CallToolResult, Tool, ToolAnnotations};
use serde_json::{json, Map, Value};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use super::grants::{self, Rights};
use crate::chat::tools::action_bridge::{ActionBridge, ActionCaller, ActionReply};
use crate::chat::tools::action_tool::{ActionEffect, AgentActionDef};

/// What the agent learns when its rights do not cover a call, or the tool does not exist.
pub const NOT_ALLOWED_MESSAGE: &str = "This tool is not available to this agent.";

/// The tool an agent sees for an action: the same name, description and schema as the built-in
/// agent gets (FR-013), with hints about its effect.
pub fn tool_of(def: &AgentActionDef) -> Tool {
    let schema = match &def.input_schema {
        Value::Object(map) => map.clone(),
        _ => Map::new(),
    };
    Tool::new(def.tool_name.clone(), def.description.clone(), schema).with_annotations(
        ToolAnnotations::new()
            .read_only(def.effect == ActionEffect::Read)
            .destructive(def.effect == ActionEffect::Destructive),
    )
}

/// A refusal in the shape of the contract.
pub fn error_result(code: &str, message: &str, extra: Option<(&str, Value)>) -> CallToolResult {
    let mut body = json!({ "code": code, "message": message });
    if let Some((key, value)) = extra {
        body[key] = value;
    }
    CallToolResult::structured_error(body)
}

pub fn not_allowed() -> CallToolResult {
    error_result("not_allowed", NOT_ALLOWED_MESSAGE, None)
}

/// Who calls, and what the call may reach.
pub struct Caller<'a> {
    pub agent: Uuid,
    pub rights: &'a Rights,
    pub changes_allowed: bool,
}

/// Runs `tool_name` for the caller when its rights cover it.
pub async fn call(
    bridge: &ActionBridge,
    catalog: &[AgentActionDef],
    caller: Caller<'_>,
    tool_name: &str,
    arguments: Option<Map<String, Value>>,
    cancel: &CancellationToken,
) -> CallToolResult {
    let Some(def) = grants::check(caller.rights, catalog, tool_name, caller.changes_allowed) else {
        return not_allowed();
    };
    let input = Value::Object(arguments.unwrap_or_default());
    let caller = ActionCaller::ExternalAgent {
        agent_id: caller.agent.to_string(),
    };
    let reply = bridge.call_as(caller, &def.action_id, input, cancel).await;
    into_result(reply)
}

/// The runner's reply as the agent sees it (contracts/mcp-surface.md, "Ergebnisse").
pub fn into_result(reply: ActionReply) -> CallToolResult {
    match reply {
        ActionReply::Ok { result } => CallToolResult::structured(result),
        ActionReply::NeedsChoice {
            field,
            message,
            options,
        } => {
            let options = serde_json::to_value(options).unwrap_or(Value::Array(Vec::new()));
            let mut body = json!({ "code": "needs_choice", "message": message, "field": field });
            body["options"] = options;
            CallToolResult::structured_error(body)
        }
        ActionReply::Err {
            code,
            field,
            message,
        } => {
            let (code, message) = match code.as_str() {
                // The runner refused the caller: the same answer as missing rights.
                "forbidden_for_agents" | "unknown_action" => {
                    ("not_allowed".to_owned(), NOT_ALLOWED_MESSAGE.to_owned())
                }
                "action_timeout" | "action_unavailable" | "tool_call_cancelled" => (
                    "unavailable".to_owned(),
                    "holzi could not run the action now.".to_owned(),
                ),
                _ => (code, message),
            };
            error_result(&code, &message, field.map(|f| ("field", Value::String(f))))
        }
    }
}
