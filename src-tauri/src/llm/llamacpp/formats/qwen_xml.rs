//! Qwen XML: `<tool_call><function=NAME><parameter=KEY>VALUE</parameter>…</function></tool_call>` (Qwen3.5).

use serde_json::Value;

use super::{scan_tool_calls, ChatFormat};
use crate::adapters::types::{ToolCall, ToolSpec};

pub struct QwenXml;

impl ChatFormat for QwenXml {
    fn name(&self) -> &'static str {
        "qwen-xml"
    }

    fn fingerprint(&self, template: &str) -> bool {
        template.contains("<function=")
    }

    fn marker(&self) -> &'static str {
        "<tool_call>"
    }

    fn parse_calls(&self, text: &str, tools: &[ToolSpec]) -> (String, Vec<ToolCall>) {
        scan_tool_calls(text, |inner| parse_xml_call(inner, tools))
    }
}

/// The JSON type the tool's schema gives one parameter.
fn param_type<'a>(tools: &'a [ToolSpec], tool: &str, param: &str) -> Option<&'a str> {
    tools
        .iter()
        .find(|t| t.name == tool)?
        .input_schema
        .get("properties")?
        .get(param)?
        .get("type")?
        .as_str()
}

/// The inside of one block. A value loses exactly one leading and one trailing newline; its type comes
/// from the tool's schema: `string` stays a string, anything else is read as JSON and stays a string when
/// it is not JSON. A parameter cut off before its closing tag is dropped.
fn parse_xml_call(inner: &str, tools: &[ToolSpec]) -> Option<(String, Value)> {
    const FUNCTION: &str = "<function=";
    const PARAMETER: &str = "<parameter=";
    const PARAMETER_END: &str = "</parameter>";

    let rest = &inner[inner.find(FUNCTION)? + FUNCTION.len()..];
    let name_end = rest.find('>')?;
    let name = rest[..name_end].trim().to_owned();
    let mut args = serde_json::Map::new();
    let mut body = &rest[name_end + 1..];
    while let Some(at) = body.find(PARAMETER) {
        let after = &body[at + PARAMETER.len()..];
        let key_end = after.find('>')?;
        let key = after[..key_end].trim().to_owned();
        let value_text = &after[key_end + 1..];
        let Some(value_end) = value_text.find(PARAMETER_END) else {
            break;
        };
        let raw = &value_text[..value_end];
        let raw = raw.strip_prefix('\n').unwrap_or(raw);
        let raw = raw.strip_suffix('\n').unwrap_or(raw);
        let value = match param_type(tools, &name, &key) {
            Some("string") => Value::String(raw.to_owned()),
            _ => serde_json::from_str(raw).unwrap_or_else(|_| Value::String(raw.to_owned())),
        };
        args.insert(key, value);
        body = &value_text[value_end + PARAMETER_END.len()..];
    }
    Some((name, Value::Object(args)))
}

#[cfg(test)]
#[path = "qwen_xml_tests.rs"]
mod tests;
