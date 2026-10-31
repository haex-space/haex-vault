//! Gemma 4: `<|tool_call>call:NAME{key:value,…}<tool_call|>`, strings between `<|"|>` (spec 070 R4).
//!
//! The syntax follows the model's chat template and llama.cpp's own reader (`common/parsers/gemma4.cpp`,
//! `gemma4_to_json`). Thinking sits in `<|channel>thought…<channel|>`, and tool results go into the
//! assistant turn that made the calls.

use serde_json::{Map, Value};

use super::{default_messages, scan_blocks, ChatFormat, Parsed, Thinking};
use crate::adapters::types::{ChatMessage, ToolCall, ToolSpec};

pub struct Gemma4;

const CALL_OPEN: &str = "<|tool_call>";
const CALL_CLOSE: &str = "<tool_call|>";
const CHANNEL_OPEN: &str = "<|channel>";
const CHANNEL_CLOSE: &str = "<channel|>";
const THOUGHT: &str = "thought";
/// Opens and closes a string value.
const QUOTE: &str = "<|\"|>";

impl ChatFormat for Gemma4 {
    fn name(&self) -> &'static str {
        "gemma4"
    }

    fn fingerprint(&self, template: &str) -> bool {
        template.contains("<|tool_call>call:")
    }

    fn marker(&self) -> &'static str {
        CALL_OPEN
    }

    fn stray_markers(&self) -> &'static [&'static str] {
        // Gemma 4 may write an empty thought block, or a lone closing tag, after its answer.
        &[CHANNEL_OPEN, CHANNEL_CLOSE]
    }

    fn thinking(&self) -> Thinking {
        Thinking {
            open: "<|channel>thought",
            close: CHANNEL_CLOSE,
        }
    }

    fn parse_calls(&self, text: &str, _tools: &[ToolSpec]) -> (String, Vec<ToolCall>) {
        scan_blocks(text, (CALL_OPEN, CALL_CLOSE), parse_call)
    }

    fn shape_messages(&self, system_prompt: Option<&str>, history: &[ChatMessage]) -> Vec<Value> {
        merge_tool_responses(default_messages(system_prompt, history))
    }

    fn parse(&self, text: &str, tools: &[ToolSpec]) -> Parsed {
        let (reasoning, rest) = split_channels(text);
        let (content, calls) = self.parse_calls(&rest, tools);
        Parsed {
            reasoning,
            content,
            calls,
        }
    }
}

/// Takes every thought block out of `text`. Channel tags that open no thought, and lone closing tags, are
/// dropped. A block the prompt opened must carry its opening tag in `text` (the streamer puts it there).
fn split_channels(text: &str) -> (Option<String>, String) {
    let mut thoughts = Vec::new();
    let mut content = String::new();
    let mut rest = text;
    while let Some(open) = rest.find(CHANNEL_OPEN) {
        content.push_str(&rest[..open]);
        let after = &rest[open + CHANNEL_OPEN.len()..];
        let Some(thought) = after.strip_prefix(THOUGHT) else {
            rest = after;
            continue;
        };
        // A thought cut off by the token limit runs to the end.
        let (inside, next) = match thought.find(CHANNEL_CLOSE) {
            Some(end) => (&thought[..end], &thought[end + CHANNEL_CLOSE.len()..]),
            None => (thought, ""),
        };
        thoughts.push(inside.trim());
        rest = next;
    }
    content.push_str(rest);
    let reasoning = thoughts
        .into_iter()
        .filter(|t| !t.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n");
    (
        (!reasoning.is_empty()).then_some(reasoning),
        content.replace(CHANNEL_CLOSE, "").trim().to_owned(),
    )
}

/// The inside of one block: `call:NAME{…}`. A call without arguments may leave the braces off.
fn parse_call(inner: &str) -> Option<(String, Value)> {
    let body = inner.strip_prefix("call:")?;
    let (name, args) = match body.find('{') {
        Some(at) => (&body[..at], &body[at..]),
        None => (body, ""),
    };
    let name = name.trim();
    if name.is_empty() || name.contains(char::is_whitespace) {
        return None;
    }
    let input = if args.trim().is_empty() {
        Value::Object(Map::new())
    } else {
        Reader { rest: args }.dict()?
    };
    Some((name.to_owned(), input))
}

/// Reads Gemma 4's value syntax. Text after the outermost object is ignored.
struct Reader<'a> {
    rest: &'a str,
}

impl Reader<'_> {
    fn skip_space(&mut self) {
        self.rest = self.rest.trim_start();
    }

    fn eat(&mut self, token: &str) -> bool {
        self.skip_space();
        match self.rest.strip_prefix(token) {
            Some(after) => {
                self.rest = after;
                true
            }
            None => false,
        }
    }

    fn value(&mut self) -> Option<Value> {
        self.skip_space();
        if self.rest.starts_with(QUOTE) {
            return self.string().map(Value::String);
        }
        match self.rest.chars().next()? {
            '{' => self.dict(),
            '[' => self.array(),
            _ => self.bare(),
        }
    }

    /// `<|"|>…<|"|>`; the text in between is taken as it is, commas and braces included.
    fn string(&mut self) -> Option<String> {
        let inside = self.rest.strip_prefix(QUOTE)?;
        let end = inside.find(QUOTE)?;
        self.rest = &inside[end + QUOTE.len()..];
        Some(inside[..end].to_owned())
    }

    fn dict(&mut self) -> Option<Value> {
        let mut map = Map::new();
        if !self.eat("{") {
            return None;
        }
        if self.eat("}") {
            return Some(Value::Object(map));
        }
        loop {
            let key = self.key()?;
            let value = self.value()?;
            map.insert(key, value);
            if self.eat(",") {
                continue;
            }
            return self.eat("}").then_some(Value::Object(map));
        }
    }

    /// A bare key up to `:`; a key in string quotes is read as well.
    fn key(&mut self) -> Option<String> {
        self.skip_space();
        let key = if self.rest.starts_with(QUOTE) {
            self.string()?
        } else {
            let end = self.rest.find([':', '}'])?;
            let key = self.rest[..end].trim().to_owned();
            self.rest = &self.rest[end..];
            key
        };
        (!key.is_empty() && self.eat(":")).then_some(key)
    }

    fn array(&mut self) -> Option<Value> {
        let mut items = Vec::new();
        if !self.eat("[") {
            return None;
        }
        if self.eat("]") {
            return Some(Value::Array(items));
        }
        loop {
            items.push(self.value()?);
            if self.eat(",") {
                continue;
            }
            return self.eat("]").then_some(Value::Array(items));
        }
    }

    /// A number, `true`, `false` or `null`. A bare word the model wrote without string quotes is kept as
    /// a string.
    fn bare(&mut self) -> Option<Value> {
        let end = self.rest.find([',', '}', ']']).unwrap_or(self.rest.len());
        let word = self.rest[..end].trim();
        if word.is_empty() || end == self.rest.len() {
            // Nothing, or a value cut off before its object closed.
            return None;
        }
        self.rest = &self.rest[end..];
        Some(serde_json::from_str(word).unwrap_or_else(|_| Value::String(word.to_owned())))
    }
}

/// Moves each run of `tool` messages into the assistant message that made the calls, as `tool_responses`,
/// together with the answer that followed them; the shape Gemma 4 was trained on and the one older Gemma 4
/// templates only read (llama.cpp `convert_tool_responses_gemma4` does the same).
fn merge_tool_responses(messages: Vec<Value>) -> Vec<Value> {
    let mut out = Vec::with_capacity(messages.len());
    let mut iter = messages.into_iter().peekable();
    while let Some(mut message) = iter.next() {
        let Some(calls) = message.get("tool_calls").and_then(Value::as_array).cloned() else {
            out.push(message);
            continue;
        };
        let mut responses = Vec::new();
        while let Some(tool) = iter.next_if(|m| m["role"] == "tool") {
            let id = &tool["tool_call_id"];
            let name = calls
                .iter()
                .find(|c| &c["id"] == id)
                .or_else(|| calls.get(responses.len()))
                .and_then(|c| c["function"]["name"].as_str())
                .unwrap_or("unknown")
                .to_owned();
            let response = match &tool["content"] {
                Value::String(text) => {
                    serde_json::from_str(text).unwrap_or_else(|_| Value::String(text.clone()))
                }
                other => other.clone(),
            };
            responses.push(serde_json::json!({"name": name, "response": response}));
        }
        if !responses.is_empty() {
            message["tool_responses"] = Value::Array(responses);
            let answer = iter.next_if(|m| {
                m["role"] == "assistant"
                    && m.get("tool_calls").is_none()
                    && m["content"].as_str().is_some_and(|c| !c.is_empty())
            });
            if let Some(answer) = answer {
                message["content"] = answer["content"].clone();
            }
        }
        out.push(message);
    }
    out
}

#[cfg(test)]
#[path = "gemma4_tests.rs"]
mod tests;
