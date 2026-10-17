//! Chat-completions request construction for OpenAI-compatible servers (spec 055, research R7).
//!
//! Turns a provider-neutral [`ChatRequest`] into the body most servers share (OpenAI, Gemini's
//! compatibility layer, Ollama, vLLM, llama.cpp): `messages`, `stream`, optional token limit,
//! `reasoning_effort` and `tools`. Kept apart from `openai_compatible.rs` so the transport and the
//! SSE decoding stay readable on their own.

use base64::Engine;
use serde_json::{json, Value};

use super::types::{Attachment, AttachmentKind, ChatMessage, ChatRequest, ChatRole};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum TokenLimitField {
    MaxTokens,
    MaxCompletionTokens,
}

/// Serializes a provider-neutral chat request for `POST {base}/chat/completions`.
pub(super) fn build_chat_body(req: &ChatRequest) -> Value {
    build_chat_body_with_limit(req, TokenLimitField::MaxTokens)
}

/// Serializes a request using the token-limit field supported by the configured endpoint.
pub(super) fn build_chat_body_with_limit(
    req: &ChatRequest,
    token_limit_field: TokenLimitField,
) -> Value {
    let mut messages = Vec::new();
    if let Some(system) = req.system_prompt.as_ref() {
        messages.push(json!({"role": "system", "content": system}));
    }
    messages.extend(build_messages(&req.messages));

    // Gemini's OpenAI-compatible model listing can return `models/<id>`, while its
    // chat-completions endpoint expects the short model id. Keep the provider's
    // persisted id unchanged and normalize only the wire value.
    let model_id = req
        .model_id
        .strip_prefix("models/")
        .unwrap_or(&req.model_id);
    let mut body = json!({
        "model": model_id,
        "messages": messages,
        "stream": true,
        // Ask for the token counts in a last chunk; servers that do not know it ignore it.
        "stream_options": {"include_usage": true},
    });
    if let Some(max) = req.max_new_tokens {
        let token_limit = match token_limit_field {
            TokenLimitField::MaxTokens => "max_tokens",
            TokenLimitField::MaxCompletionTokens => "max_completion_tokens",
        };
        body[token_limit] = json!(max);
    }
    // Sent only when it is one of this model's own cached options (as for Anthropic): an option
    // the model does not offer must never reach the server.
    let offered_effort = req
        .capabilities
        .as_ref()
        .and_then(|c| c.reasoning.as_ref())
        .zip(req.reasoning_option.as_deref())
        .filter(|(control, id)| control.offers(id))
        .map(|(_, id)| id);
    if let Some(id) = offered_effort {
        body["reasoning_effort"] = json!(id);
    }
    if !req.tools.is_empty() {
        let tools: Vec<Value> = req
            .tools
            .iter()
            .map(|t| {
                json!({
                    "type": "function",
                    "function": {
                        "name": t.name,
                        "description": t.description,
                        "parameters": t.input_schema,
                    },
                })
            })
            .collect();
        body["tools"] = Value::Array(tools);
    }
    body
}

/// Regroups holzi's flat per-row history: consecutive `ToolCall` rows become one `assistant`
/// message with one entry in `tool_calls` each, every `ToolResult` row becomes one `tool` message.
fn build_messages(messages: &[ChatMessage]) -> Vec<Value> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < messages.len() {
        match &messages[i].role {
            ChatRole::User => {
                out.push(json!({"role": "user", "content": user_content(&messages[i])}));
                i += 1;
            }
            ChatRole::Assistant => {
                let text = messages[i].content.clone();
                i += 1;
                let calls = collect_tool_calls(messages, &mut i);
                out.push(assistant_message(text, calls));
            }
            ChatRole::ToolCall { .. } => {
                let calls = collect_tool_calls(messages, &mut i);
                out.push(assistant_message(String::new(), calls));
            }
            ChatRole::ToolResult { .. } => {
                let mut image_parts = Vec::new();
                while let Some(ChatMessage {
                    role:
                        ChatRole::ToolResult {
                            call_id,
                            content,
                            is_error: _,
                        },
                    attachments,
                    ..
                }) = messages.get(i)
                {
                    out.push(json!({"role": "tool", "tool_call_id": call_id, "content": content}));
                    // A tool message carries text only; its images follow as one user message.
                    image_parts.extend(attachments.iter().map(attachment_part));
                    i += 1;
                }
                if !image_parts.is_empty() {
                    out.push(json!({"role": "user", "content": image_parts}));
                }
            }
        }
    }
    out
}

fn collect_tool_calls(messages: &[ChatMessage], i: &mut usize) -> Vec<Value> {
    let mut calls = Vec::new();
    while let Some(ChatRole::ToolCall { id, name, input }) = messages.get(*i).map(|m| &m.role) {
        calls.push(json!({
            "id": id,
            "type": "function",
            "function": {"name": name, "arguments": input.to_string()},
        }));
        *i += 1;
    }
    calls
}

fn assistant_message(text: String, calls: Vec<Value>) -> Value {
    if calls.is_empty() {
        return json!({"role": "assistant", "content": text});
    }
    let content = if text.is_empty() {
        Value::Null
    } else {
        Value::String(text)
    };
    json!({"role": "assistant", "content": content, "tool_calls": calls})
}

/// A bare string for plain text, otherwise the text part followed by one part per attachment.
fn user_content(message: &ChatMessage) -> Value {
    if message.attachments.is_empty() {
        return Value::String(message.content.clone());
    }
    let mut parts = Vec::new();
    if !message.content.is_empty() {
        parts.push(json!({"type": "text", "text": message.content}));
    }
    parts.extend(message.attachments.iter().map(attachment_part));
    Value::Array(parts)
}

/// One content part for one attachment: an `image_url` data URI for images and a text part for the
/// rest. The chat-completions format has no common document part, so a document is named but not
/// sent rather than guessed at.
fn attachment_part(attachment: &Attachment) -> Value {
    match attachment.kind {
        AttachmentKind::Image => json!({
            "type": "image_url",
            "image_url": {"url": format!(
                "data:{};base64,{}",
                attachment.media_type,
                base64::engine::general_purpose::STANDARD.encode(&attachment.bytes),
            )},
        }),
        AttachmentKind::Document => json!({
            "type": "text",
            "text": format!(
                "[Attached document: {} (not sent: this server takes no documents)]",
                attachment.name
            ),
        }),
        AttachmentKind::Text => json!({
            "type": "text",
            "text": format!(
                "[Attached file: {}]\n{}",
                attachment.name,
                String::from_utf8_lossy(&attachment.bytes),
            ),
        }),
    }
}
