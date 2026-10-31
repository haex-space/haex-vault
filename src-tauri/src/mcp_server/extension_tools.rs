//! The tools of extensions as external agents see them (spec 021 US4): their entry in the tool list
//! and their answers in the shape of the contract.

use rmcp::model::{CallToolResult, Tool, ToolAnnotations};
use serde_json::{json, Map, Value};

use super::agent_log::Outcome;
use super::dispatch::{error_result, unavailable};
use crate::chat::tools::action_bridge::ACTION_FAILED_MESSAGE;
use crate::chat::tools::{RiskClass, Tool as ChatTool, ToolResult};

/// The tool an agent sees for a tool of an extension, or for the command tool: the name, description
/// and schema the built-in agent gets (US4, US5).
pub fn tool_of(tool: &dyn ChatTool) -> Tool {
    let schema = match tool.input_schema() {
        Value::Object(map) => map,
        _ => Map::new(),
    };
    let risk = tool.risk_class();
    Tool::new(
        tool.name().to_owned(),
        tool.description().to_owned(),
        schema,
    )
    .with_annotations(
        ToolAnnotations::new()
            .read_only(risk == RiskClass::Safe)
            .destructive(risk == RiskClass::Risky),
    )
}

/// An extension tool's answer as the agent sees it (spec 018 `tool.rs`): its data on success; a
/// connection holzi could not make, a stop or a timeout is `unavailable`; an error of the extension
/// itself keeps the extension's data, which the agent reads as data, never as instructions.
pub fn result(result: ToolResult) -> (CallToolResult, Outcome) {
    let body = serde_json::from_str::<Value>(&result.content)
        .unwrap_or_else(|_| json!({ "data": result.content }));
    if !result.is_error {
        return (CallToolResult::structured(body), Outcome::Ok);
    }
    match body["error"].as_str() {
        Some("extension_unavailable" | "tool_unavailable" | "cancelled" | "timeout") => {
            (unavailable(), Outcome::Unavailable)
        }
        Some(_) => (
            error_result("failed", ACTION_FAILED_MESSAGE, None),
            Outcome::Error,
        ),
        None => {
            let mut answer = json!({ "code": "failed", "message": ACTION_FAILED_MESSAGE });
            answer["extension"] = body["extension"].clone();
            answer["data"] = body["data"].clone();
            (CallToolResult::structured_error(answer), Outcome::Error)
        }
    }
}
