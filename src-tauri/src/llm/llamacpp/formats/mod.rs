//! Tool-call formats of the llama.cpp engine (spec 070; contract: `contracts/tool-call-formats.md`).
//!
//! A format knows how one model family writes a tool call. It is chosen from the model file's own chat
//! template (`detect`), never from the model's name, so a renamed or re-quantised file keeps working and a
//! template this engine does not know is refused instead of guessed at.

mod hermes;
mod holdback;
mod qwen_xml;

use serde_json::{json, Value};

use crate::adapters::types::{ChatMessage, ChatRole, ToolCall, ToolSpec};

pub use hermes::Hermes;
pub use holdback::Holdback;
pub use qwen_xml::QwenXml;

/// What a finished answer turns into.
#[derive(Debug, Clone, PartialEq)]
pub struct Parsed {
    pub reasoning: Option<String>,
    pub content: String,
    pub calls: Vec<ToolCall>,
}

/// One way a model family writes tool calls.
pub trait ChatFormat: Sync {
    /// Short name for messages and logs.
    fn name(&self) -> &'static str;

    /// True when `template` (the file's chat template text) carries this format's marks.
    fn fingerprint(&self, template: &str) -> bool;

    /// The string a call block starts with; streamed text is held back from its first character on.
    fn marker(&self) -> &'static str;

    /// Reads the call blocks out of `text`: returns the text without them and the calls, in order.
    fn parse_calls(&self, text: &str, tools: &[ToolSpec]) -> (String, Vec<ToolCall>);

    /// Turns holzi's flat history into the messages the template reads. The default fits the Qwen
    /// family; a format whose template reads another shape overrides it.
    fn shape_messages(&self, system_prompt: Option<&str>, history: &[ChatMessage]) -> Vec<Value> {
        default_messages(system_prompt, history)
    }

    /// Splits thinking off, then reads the calls out of what is left.
    fn parse(&self, text: &str, tools: &[ToolSpec]) -> Parsed {
        let (reasoning, text) = split_thinking(text);
        let (content, calls) = self.parse_calls(&text, tools);
        Parsed {
            reasoning,
            content,
            calls,
        }
    }
}

/// The formats this engine knows, in the order `detect` tries them.
static FORMATS: [&dyn ChatFormat; 2] = [&QwenXml, &Hermes];

/// The format whose marks the template carries, or `None` when the engine does not know the syntax.
pub fn detect(template: &str) -> Option<&'static dyn ChatFormat> {
    FORMATS.iter().copied().find(|f| f.fingerprint(template))
}

/// The tool definitions in the shape the templates read (`{"type": "function", "function": {…}}`).
pub fn template_tools(tools: &[ToolSpec]) -> Vec<Value> {
    tools
        .iter()
        .map(|t| {
            json!({"type": "function", "function": {
                "name": t.name, "description": t.description, "parameters": t.input_schema,
            }})
        })
        .collect()
}

/// The history as the Qwen templates read it: consecutive `ToolCall` rows become one assistant message
/// with `tool_calls` (preserving call IDs and arguments as objects), each `ToolResult` row its own
/// `tool` message.
pub fn default_messages(system_prompt: Option<&str>, history: &[ChatMessage]) -> Vec<Value> {
    let mut out = Vec::new();
    if let Some(system) = system_prompt {
        out.push(json!({"role": "system", "content": system}));
    }
    let mut i = 0;
    while i < history.len() {
        let message = &history[i];
        match &message.role {
            ChatRole::User => out.push(json!({"role": "user", "content": message.content})),
            ChatRole::Assistant => {
                out.push(json!({"role": "assistant", "content": message.content}))
            }
            ChatRole::ToolCall { .. } => {
                let mut calls = Vec::new();
                while let Some(ChatRole::ToolCall {
                    id, name, input, ..
                }) = history.get(i).map(|m| &m.role)
                {
                    calls.push(json!({
                        "type": "function",
                        "id": id,
                        "function": {"name": name, "arguments": input},
                    }));
                    i += 1;
                }
                out.push(json!({"role": "assistant", "content": "", "tool_calls": calls}));
                continue;
            }
            ChatRole::ToolResult {
                call_id, content, ..
            } => out.push(json!({"role": "tool", "content": content, "tool_call_id": call_id})),
        }
        i += 1;
    }
    out
}

/// Splits `<think>…</think>` off the front: the prompt may have opened the block (`enable_thinking`), so
/// everything before a `</think>` is thinking, without a leading `<think>`.
pub fn split_thinking(text: &str) -> (Option<String>, String) {
    match text.find("</think>") {
        Some(end) => {
            let reasoning = text[..end].trim_start_matches("<think>").trim().to_owned();
            let rest = text[end + "</think>".len()..].trim_start().to_owned();
            ((!reasoning.is_empty()).then_some(reasoning), rest)
        }
        None => (None, text.to_owned()),
    }
}

const OPEN: &str = "<tool_call>";
const CLOSE: &str = "</tool_call>";

/// Walks the `<tool_call>…</tool_call>` blocks of `text` and lets `parse_inner` read each block's inside.
/// A block that cannot be read stays in the text as it was written; a block with no closing tag (cut off
/// by the token limit) is read up to its end.
pub(crate) fn scan_tool_calls(
    text: &str,
    mut parse_inner: impl FnMut(&str) -> Option<(String, Value)>,
) -> (String, Vec<ToolCall>) {
    let mut content = String::new();
    let mut calls = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find(OPEN) {
        content.push_str(&rest[..start]);
        let after = &rest[start + OPEN.len()..];
        let (inner, next) = match after.find(CLOSE) {
            Some(end) => (&after[..end], &after[end + CLOSE.len()..]),
            None => (after, ""),
        };
        let inner = inner.trim();
        match parse_inner(inner) {
            Some((name, input)) => calls.push(ToolCall {
                id: format!("call_{}", calls.len()),
                name,
                input,
            }),
            None => content.push_str(&format!("{OPEN}{inner}{CLOSE}")),
        }
        rest = next;
    }
    content.push_str(rest);
    (content.trim().to_owned(), calls)
}

#[cfg(test)]
#[path = "formats_tests.rs"]
mod tests;
