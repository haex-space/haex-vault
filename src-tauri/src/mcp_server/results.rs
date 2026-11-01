//! What an external agent's tool call answers (spec 021 contracts/mcp-surface.md, "Ergebnisse"),
//! and how the agent log names it. A refusal never says whether a target exists.

use rmcp::model::CallToolResult;
use serde_json::{json, Value};

use super::agent_log::Outcome;
use crate::chat::tools::action_bridge::ActionReply;
use crate::chat::tools::native_action::{NativeError, NativeReply};

/// What the agent learns when its rights do not cover a call, or the tool does not exist.
pub const NOT_ALLOWED_MESSAGE: &str = "This tool is not available to this agent.";

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

pub(super) fn unavailable() -> CallToolResult {
    error_result("unavailable", "holzi could not run the action now.", None)
}

/// A native executor's answer as the agent sees it; a refusal of the caller reads as missing
/// rights.
pub(super) fn native_result(reply: Result<NativeReply, NativeError>) -> (CallToolResult, Outcome) {
    match reply {
        Ok(reply) => (CallToolResult::structured(reply.result), Outcome::Ok),
        Err(error) if error.code == "forbidden_for_agents" => {
            (not_allowed(), Outcome::DeniedRights)
        }
        Err(error) => {
            // A native action that asks the user itself reports how that ended (spec 065).
            let outcome = match error.code.as_str() {
                "site_not_allowed" => Outcome::DeniedSite,
                // The user stopped the agent's browser session (spec 065 FR-016).
                "user_declined" | "stopped" => Outcome::DeniedUser,
                "no_answer" => Outcome::NoAnswer,
                "paused" => Outcome::Paused,
                "not_allowed" => Outcome::DeniedRights,
                _ => Outcome::Error,
            };
            (
                error_result(
                    &error.code,
                    &error.message,
                    error.field.map(|f| ("field", Value::String(f))),
                ),
                outcome,
            )
        }
    }
}

/// The runner's reply as the agent sees it (contracts/mcp-surface.md, "Ergebnisse"), and how the
/// log names it.
pub fn into_result(reply: ActionReply) -> (CallToolResult, Outcome) {
    match reply {
        ActionReply::Ok { result } => (CallToolResult::structured(result), Outcome::Ok),
        ActionReply::NeedsChoice {
            field,
            message,
            options,
        } => {
            let options = serde_json::to_value(options).unwrap_or(Value::Array(Vec::new()));
            let mut body = json!({ "code": "needs_choice", "message": message, "field": field });
            body["options"] = options;
            // Nothing ran; the agent calls again with a choice.
            (CallToolResult::structured_error(body), Outcome::Error)
        }
        ActionReply::Err {
            code,
            field,
            message,
        } => match code.as_str() {
            // The runner refused the caller: the same answer as missing rights.
            "forbidden_for_agents" | "unknown_action" => (not_allowed(), Outcome::DeniedRights),
            "action_timeout" | "action_unavailable" | "tool_call_cancelled" => {
                (unavailable(), Outcome::Unavailable)
            }
            _ => (
                error_result(&code, &message, field.map(|f| ("field", Value::String(f)))),
                Outcome::Error,
            ),
        },
    }
}
