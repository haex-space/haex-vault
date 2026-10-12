//! Anthropic Messages-API request construction.
//!
//! Turns a provider-neutral [`ChatRequest`] into the vendor's wire body:
//! the `max_tokens`/`thinking` interplay, the adaptive-vs-manual
//! thinking decision (read from the model's cached capabilities), and the regrouping of
//! holzi's flat per-row history into Anthropic's block shape. Kept apart
//! from `anthropic.rs` so the transport and SSE-decoding side of the
//! adapter stays readable on its own.

use base64::Engine;
use serde_json::Value;

use super::types::{Attachment, AttachmentKind, ChatRequest, ChatRole};
use crate::model_capabilities::ThinkingStyle;

/// Fallback `max_tokens` cap sent when the request does not carry one.
/// Anthropic Messages API requires the field, so we substitute a
/// conservative default rather than refusing the request.
const DEFAULT_MAX_TOKENS: u32 = 4096;
const MIN_MANUAL_THINKING_MAX_TOKENS: u32 = 1025;
const MIN_MANUAL_THINKING_BUDGET: u32 = 1024;
const MAX_MANUAL_THINKING_BUDGET: u32 = 16_384;

/// Serializes a provider-neutral chat request for Anthropic's Messages API.
pub(super) fn build_messages_body(req: &ChatRequest) -> Value {
    let requested_max_tokens = req
        .max_new_tokens
        .map(|n| n as u32)
        .unwrap_or(DEFAULT_MAX_TOKENS);
    // How this model wants thinking requested comes from its cached
    // capabilities (spec 012), not from its id. A model whose style is not
    // determined is sent no `thinking` field at all — never a shape it might
    // reject.
    let capabilities = req.capabilities.as_ref();
    let thinking_style = capabilities.and_then(|c| c.thinking_style);
    let adaptive_thinking =
        req.reasoning_requested && thinking_style == Some(ThinkingStyle::Adaptive);
    let manual_thinking = req.reasoning_requested && thinking_style == Some(ThinkingStyle::Manual);
    // Anthropic requires a manual thinking budget to be >= 1024 and strictly
    // lower than max_tokens. Preserve the caller's cap whenever possible,
    // reserving one output token for small reasoning requests.
    let max_tokens = if manual_thinking {
        requested_max_tokens.max(MIN_MANUAL_THINKING_MAX_TOKENS)
    } else {
        requested_max_tokens
    };
    let mut body = serde_json::json!({
        "model": req.model_id,
        "max_tokens": max_tokens,
        "messages": build_messages(&req.messages),
        "stream": true,
    });
    if let Some(sys) = req.system_prompt.as_ref() {
        body["system"] = Value::String(sys.clone());
    }
    if adaptive_thinking {
        body["thinking"] = serde_json::json!({"type": "adaptive"});
    } else if manual_thinking {
        body["thinking"] = serde_json::json!({
            "type": "enabled",
            "budget_tokens": (max_tokens - 1)
                .clamp(MIN_MANUAL_THINKING_BUDGET, MAX_MANUAL_THINKING_BUDGET),
        });
    }
    // The selected reasoning option, independent of `thinking` above (effort
    // works with or without thinking). Sent only when it is one of this
    // model's own cached options — `send_message` already validated it, and
    // an option the model does not offer must never reach the provider.
    let offered_effort = capabilities
        .and_then(|c| c.reasoning.as_ref())
        .zip(req.reasoning_option.as_deref())
        .filter(|(control, id)| control.offers(id))
        .map(|(_, id)| id);
    if let Some(id) = offered_effort {
        body["output_config"] = serde_json::json!({"effort": id});
    }
    if !req.tools.is_empty() {
        let tools: Vec<Value> = req
            .tools
            .iter()
            .map(|t| {
                serde_json::json!({
                    "name": t.name,
                    "description": t.description,
                    "input_schema": t.input_schema,
                })
            })
            .collect();
        body["tools"] = Value::Array(tools);
    }
    body
}

/// Groups the flat, per-row `messages` history into Anthropic's wire
/// shape: consecutive `ToolCall` rows become one `assistant` message with
/// one `tool_use` block per call, and consecutive `ToolResult` rows become
/// one `user` message with one `tool_result` block per call, in the same
/// order (data-model.md's two-call example).
fn build_messages(messages: &[super::types::ChatMessage]) -> Vec<Value> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < messages.len() {
        match &messages[i].role {
            ChatRole::User => {
                let content = if messages[i].attachments.is_empty() {
                    Value::String(messages[i].content.clone())
                } else {
                    Value::Array(user_content_blocks(
                        &messages[i].content,
                        &messages[i].attachments,
                    ))
                };
                out.push(serde_json::json!({"role": "user", "content": content}));
                i += 1;
            }
            ChatRole::Assistant => {
                let text = messages[i].content.clone();
                i += 1;

                let mut blocks = Vec::new();
                if !text.is_empty() {
                    blocks.push(serde_json::json!({
                        "type": "text",
                        "text": text,
                    }));
                }
                while let Some(ChatRole::ToolCall { id, name, input }) =
                    messages.get(i).map(|m| &m.role)
                {
                    blocks.push(serde_json::json!({
                        "type": "tool_use",
                        "id": id,
                        "name": name,
                        "input": input,
                    }));
                    i += 1;
                }
                if blocks.is_empty() {
                    continue;
                }
                if blocks.len() == 1 && !text.is_empty() {
                    out.push(serde_json::json!({"role": "assistant", "content": text}));
                } else {
                    out.push(serde_json::json!({"role": "assistant", "content": blocks}));
                }
            }
            ChatRole::ToolCall { .. } => {
                let mut blocks = Vec::new();
                while let Some(ChatRole::ToolCall { id, name, input }) =
                    messages.get(i).map(|m| &m.role)
                {
                    blocks.push(serde_json::json!({
                        "type": "tool_use",
                        "id": id,
                        "name": name,
                        "input": input,
                    }));
                    i += 1;
                }
                out.push(serde_json::json!({"role": "assistant", "content": blocks}));
            }
            ChatRole::ToolResult { .. } => {
                let mut blocks = Vec::new();
                while let Some((
                    ChatRole::ToolResult {
                        call_id,
                        content,
                        is_error,
                    },
                    attachments,
                )) = messages.get(i).map(|m| (&m.role, &m.attachments))
                {
                    // Images of a tool (spec 044 research R2) go with its text as blocks.
                    let content = if attachments.is_empty() {
                        Value::String(content.clone())
                    } else {
                        Value::Array(user_content_blocks(content, attachments))
                    };
                    let mut block = serde_json::json!({
                        "type": "tool_result",
                        "tool_use_id": call_id,
                        "content": content,
                    });
                    if *is_error {
                        block["is_error"] = Value::Bool(true);
                    }
                    blocks.push(block);
                    i += 1;
                }
                out.push(serde_json::json!({"role": "user", "content": blocks}));
            }
        }
    }
    out
}

/// Builds a `user` message's `content` block array (research.md §3):
/// the message's own text, if any, followed by one block per attachment.
/// Only called when at least one attachment is present — a plain-text
/// user message with no attachments stays a bare string (`build_messages`
/// above), matching the wire shape every other adapter/test already
/// expects for the common case.
fn user_content_blocks(text: &str, attachments: &[Attachment]) -> Vec<Value> {
    let mut blocks = Vec::new();
    if !text.is_empty() {
        blocks.push(serde_json::json!({"type": "text", "text": text}));
    }
    for attachment in attachments {
        blocks.push(attachment_block(attachment));
    }
    blocks
}

/// One Anthropic content block for a single attachment (research.md §3):
/// base64 `image`/`document` blocks for binary kinds, and a plain
/// appended `text` block for `Text` — Anthropic's Messages API has no
/// dedicated "plain text attachment" block, and inlining is simpler and
/// cheaper than wrapping it as a base64 `document` just to unwrap it again
/// on the model's side.
fn attachment_block(attachment: &Attachment) -> Value {
    match attachment.kind {
        AttachmentKind::Image => serde_json::json!({
            "type": "image",
            "source": {
                "type": "base64",
                "media_type": attachment.media_type,
                "data": base64::engine::general_purpose::STANDARD.encode(&attachment.bytes),
            },
        }),
        AttachmentKind::Document => serde_json::json!({
            "type": "document",
            "source": {
                "type": "base64",
                "media_type": attachment.media_type,
                "data": base64::engine::general_purpose::STANDARD.encode(&attachment.bytes),
            },
        }),
        AttachmentKind::Text => serde_json::json!({
            "type": "text",
            "text": format!(
                "[Attached file: {}]\n{}",
                attachment.name,
                String::from_utf8_lossy(&attachment.bytes),
            ),
        }),
    }
}
