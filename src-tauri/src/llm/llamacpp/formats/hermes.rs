//! Hermes JSON: `<tool_call>{"name": …, "arguments": {…}}</tool_call>` (Qwen3 and many others).

use serde_json::{json, Value};

use super::{scan_tool_calls, ChatFormat};
use crate::adapters::types::{ToolCall, ToolSpec};

pub struct Hermes;

impl ChatFormat for Hermes {
    fn name(&self) -> &'static str {
        "hermes-json"
    }

    fn fingerprint(&self, template: &str) -> bool {
        template.contains("<tool_call>")
            && template.contains("\"arguments\"")
            && !template.contains("<function=")
    }

    fn marker(&self) -> &'static str {
        "<tool_call>"
    }

    fn parse_calls(&self, text: &str, _tools: &[ToolSpec]) -> (String, Vec<ToolCall>) {
        scan_tool_calls(text, parse_json_call)
    }
}

/// The inside of one block: a JSON object with `name` and `arguments` (an object, or a string holding one).
fn parse_json_call(inner: &str) -> Option<(String, Value)> {
    let value: Value = serde_json::from_str(inner).ok()?;
    let name = value.get("name")?.as_str()?.to_owned();
    let arguments = match value.get("arguments") {
        Some(Value::String(s)) => serde_json::from_str(s).unwrap_or(Value::Null),
        Some(other) => other.clone(),
        None => json!({}),
    };
    Some((name, arguments))
}

#[cfg(test)]
#[path = "hermes_tests.rs"]
mod tests;
