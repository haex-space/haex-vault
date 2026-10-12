//! Tests for Anthropic request-body construction. The pure cases call
//! [`build_messages_body`] directly; the two `wiremock` cases assert the
//! body as it actually leaves the adapter, which is the only way to prove
//! the streaming path serializes what these helpers build.

use wiremock::matchers::{body_json, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::anthropic::AnthropicAdapter;
use super::request::build_messages_body;
use super::types::{Attachment, AttachmentKind, ChatMessage, ChatRequest, ChatRole, ToolSpec};
use super::ProviderAdapter;
use crate::model_capabilities::{
    ModelCapabilities, ReasoningControl, ReasoningOption, ThinkingStyle,
};

fn sample_request(model: &str) -> ChatRequest {
    ChatRequest {
        sampling: Default::default(),
        model_id: model.to_string(),
        thread_id: None,
        system_prompt: None,
        messages: vec![ChatMessage {
            role: ChatRole::User,
            attachments: Vec::new(),
            content: "hi".to_string(),
        }],
        reasoning_requested: false,
        max_new_tokens: Some(128),
        tools: Vec::new(),
        autonomy_mode: Default::default(),
        reasoning_option: Default::default(),
        capabilities: Default::default(),
    }
}

fn capabilities(
    reasoning: Option<ReasoningControl>,
    thinking_style: Option<ThinkingStyle>,
) -> Option<ModelCapabilities> {
    Some(ModelCapabilities {
        reasoning,
        thinking_style,
        ..ModelCapabilities::default()
    })
}

fn presets(ids: &[&str]) -> Option<ReasoningControl> {
    Some(ReasoningControl::presets(
        ids.iter()
            .map(|id| ReasoningOption {
                id: id.to_string(),
                label: id.to_string(),
            })
            .collect(),
    ))
}

#[test]
fn a_manual_thinking_style_requests_a_budgeted_thinking_block() {
    let mut request = sample_request("claude-sonnet-4-20250514");
    request.capabilities = capabilities(
        Some(ReasoningControl::ModelManaged),
        Some(ThinkingStyle::Manual),
    );
    assert!(build_messages_body(&request).get("thinking").is_none());

    request.reasoning_requested = true;
    let body = build_messages_body(&request);

    assert_eq!(body["thinking"]["type"], "enabled");
    let budget = body["thinking"]["budget_tokens"]
        .as_u64()
        .expect("manual thinking has a budget");
    assert!(budget >= 1024);
    assert!(budget < body["max_tokens"].as_u64().unwrap());
}

#[test]
fn an_adaptive_thinking_style_requests_adaptive_thinking_without_a_budget() {
    let mut request = sample_request("claude-opus-4-7");
    request.reasoning_requested = true;
    request.capabilities = capabilities(presets(&["low", "high"]), Some(ThinkingStyle::Adaptive));

    let body = build_messages_body(&request);

    assert_eq!(body["thinking"]["type"], "adaptive");
    assert!(body["thinking"].get("budget_tokens").is_none());
}

#[test]
fn an_undetermined_thinking_style_sends_no_thinking_field_even_when_reasoning_is_requested() {
    for caps in [
        None,
        capabilities(Some(ReasoningControl::ModelManaged), None),
        capabilities(None, None),
    ] {
        let mut request = sample_request("claude-opus-5");
        request.reasoning_requested = true;
        request.capabilities = caps;

        assert!(build_messages_body(&request).get("thinking").is_none());
    }
}

#[test]
fn manual_thinking_raises_max_tokens_to_leave_room_for_the_budget() {
    let mut request = sample_request("claude-sonnet-4-20250514");
    request.reasoning_requested = true;
    request.max_new_tokens = Some(128);
    request.capabilities = capabilities(
        Some(ReasoningControl::ModelManaged),
        Some(ThinkingStyle::Manual),
    );

    let body = build_messages_body(&request);

    assert!(body["max_tokens"].as_u64().unwrap() > 1024);
}

#[test]
fn no_reasoning_option_means_no_output_config() {
    let mut request = sample_request("claude-sonnet-5");
    request.capabilities = capabilities(presets(&["low", "high"]), Some(ThinkingStyle::Adaptive));

    assert!(build_messages_body(&request).get("output_config").is_none());
}

#[test]
fn an_offered_reasoning_option_is_sent_as_its_own_id() {
    let mut request = sample_request("claude-sonnet-5");
    request.capabilities = capabilities(presets(&["low", "high", "max"]), None);
    request.reasoning_option = Some("max".to_string());

    assert_eq!(
        build_messages_body(&request)["output_config"]["effort"],
        "max"
    );
}

#[test]
fn an_option_the_model_does_not_offer_is_never_sent() {
    // A stale selection (the provider dropped `xhigh`) must not reach the
    // provider even if it slipped past the caller's own validation.
    let mut request = sample_request("claude-opus-4-6");
    request.capabilities = capabilities(presets(&["low", "medium", "high", "max"]), None);
    request.reasoning_option = Some("xhigh".to_string());

    assert!(build_messages_body(&request).get("output_config").is_none());
}

#[test]
fn a_reasoning_option_is_ignored_without_selectable_options() {
    for caps in [
        None,
        capabilities(Some(ReasoningControl::ModelManaged), None),
        capabilities(Some(ReasoningControl::Unavailable), None),
        capabilities(None, None),
    ] {
        let mut request = sample_request("claude-3-5-sonnet-20241022");
        request.reasoning_option = Some("low".to_string());
        request.capabilities = caps;

        assert!(build_messages_body(&request).get("output_config").is_none());
    }
}

#[test]
fn a_user_message_with_no_attachments_stays_a_plain_string() {
    let request = sample_request("claude-sonnet-5");
    let body = build_messages_body(&request);
    assert_eq!(body["messages"][0]["content"], "hi");
}

#[test]
fn an_image_attachment_becomes_a_base64_image_block_alongside_the_text() {
    let mut request = sample_request("claude-sonnet-5");
    request.messages[0].attachments.push(Attachment {
        name: "photo.png".to_string(),
        kind: AttachmentKind::Image,
        media_type: "image/png".to_string(),
        bytes: vec![1, 2, 3],
    });
    let body = build_messages_body(&request);
    let content = &body["messages"][0]["content"];
    assert_eq!(content[0]["type"], "text");
    assert_eq!(content[0]["text"], "hi");
    assert_eq!(content[1]["type"], "image");
    assert_eq!(content[1]["source"]["media_type"], "image/png");
    assert!(!content[1]["source"]["data"].as_str().unwrap().is_empty());
}

#[test]
fn a_tool_result_with_an_image_sends_its_text_and_the_image_as_blocks() {
    let mut request = sample_request("claude-sonnet-5");
    request.messages.push(ChatMessage {
        role: ChatRole::ToolCall {
            id: "call-1".to_string(),
            name: "files_read".to_string(),
            input: serde_json::json!({}),
        },
        content: String::new(),
        attachments: Vec::new(),
    });
    request.messages.push(ChatMessage {
        role: ChatRole::ToolResult {
            call_id: "call-1".to_string(),
            content: "{\"note\":\"The image follows.\"}".to_string(),
            is_error: false,
        },
        content: String::new(),
        attachments: vec![Attachment {
            name: "foto.jpg".to_string(),
            kind: AttachmentKind::Image,
            media_type: "image/jpeg".to_string(),
            bytes: vec![1, 2, 3],
        }],
    });
    let body = build_messages_body(&request);
    let result = &body["messages"][2]["content"][0];
    assert_eq!(result["type"], "tool_result");
    assert_eq!(result["content"][0]["type"], "text");
    assert_eq!(
        result["content"][0]["text"],
        "{\"note\":\"The image follows.\"}"
    );
    assert_eq!(result["content"][1]["type"], "image");
    assert_eq!(result["content"][1]["source"]["media_type"], "image/jpeg");
}

#[test]
fn a_text_attachment_is_inlined_as_an_extra_text_block() {
    let mut request = sample_request("claude-sonnet-5");
    request.messages[0].attachments.push(Attachment {
        name: "notes.txt".to_string(),
        kind: AttachmentKind::Text,
        media_type: "text/plain".to_string(),
        bytes: b"hello from a file".to_vec(),
    });
    let body = build_messages_body(&request);
    let content = &body["messages"][0]["content"];
    assert_eq!(content[1]["type"], "text");
    assert!(content[1]["text"].as_str().unwrap().contains("notes.txt"));
    assert!(content[1]["text"]
        .as_str()
        .unwrap()
        .contains("hello from a file"));
}

fn sse_body(events: &[(&str, serde_json::Value)]) -> String {
    let mut out = String::new();
    for (event, data) in events {
        out.push_str(&format!("event: {event}\n"));
        out.push_str(&format!("data: {}\n\n", data));
    }
    out
}

#[tokio::test]
async fn stream_chat_sends_request_body_with_model_and_stream_flag() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .and(body_json(serde_json::json!({
            "model": "claude-opus-5",
            "max_tokens": 128,
            "messages": [{"role": "user", "content": "hi"}],
            "stream": true,
        })))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(sse_body(&[(
                    "message_stop",
                    serde_json::json!({"type": "message_stop"}),
                )]))
                .insert_header("content-type", "text/event-stream"),
        )
        .expect(1)
        .mount(&server)
        .await;

    let adapter = AnthropicAdapter::new(server.uri(), "sk-any".to_string()).unwrap();
    let mut stream = adapter
        .stream_chat(sample_request("claude-opus-5"))
        .await
        .unwrap();
    while stream.next().await.is_some() {}
}

#[tokio::test]
/// Two ordered tool calls reconstruct as exactly one `assistant` message
/// with two `tool_use` blocks followed by one `user` message with the
/// matching `tool_result` blocks, in the same order and with matching ids
/// (data-model.md's two-call example).
async fn stream_chat_groups_ordered_tool_calls_and_results_into_two_messages() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .and(body_json(serde_json::json!({
            "model": "claude-opus-5",
            "max_tokens": 128,
            "messages": [
                {"role": "user", "content": "hi"},
                {"role": "assistant", "content": [
                    {"type": "text", "text": "I will check that."},
                    {"type": "tool_use", "id": "call-a", "name": "first", "input": {"x": 1}},
                    {"type": "tool_use", "id": "call-b", "name": "second", "input": {"y": 2}},
                ]},
                {"role": "user", "content": [
                    {"type": "tool_result", "tool_use_id": "call-a", "content": "result-a"},
                    {"type": "tool_result", "tool_use_id": "call-b", "content": "result-b", "is_error": true},
                ]},
            ],
            "stream": true,
        })))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(sse_body(&[(
                    "message_stop",
                    serde_json::json!({"type": "message_stop"}),
                )]))
                .insert_header("content-type", "text/event-stream"),
        )
        .expect(1)
        .mount(&server)
        .await;

    let adapter = AnthropicAdapter::new(server.uri(), "sk-any".to_string()).unwrap();
    let request = ChatRequest {
        sampling: Default::default(),
        model_id: "claude-opus-5".to_string(),
        thread_id: None,
        system_prompt: None,
        messages: vec![
            ChatMessage {
                role: ChatRole::User,
                attachments: Vec::new(),
                content: "hi".to_string(),
            },
            ChatMessage {
                role: ChatRole::Assistant,
                attachments: Vec::new(),
                content: "I will check that.".to_string(),
            },
            ChatMessage {
                role: ChatRole::ToolCall {
                    id: "call-a".to_string(),
                    name: "first".to_string(),
                    input: serde_json::json!({"x": 1}),
                },
                attachments: Vec::new(),
                content: String::new(),
            },
            ChatMessage {
                role: ChatRole::ToolCall {
                    id: "call-b".to_string(),
                    name: "second".to_string(),
                    input: serde_json::json!({"y": 2}),
                },
                attachments: Vec::new(),
                content: String::new(),
            },
            ChatMessage {
                role: ChatRole::ToolResult {
                    call_id: "call-a".to_string(),
                    content: "result-a".to_string(),
                    is_error: false,
                },
                attachments: Vec::new(),
                content: String::new(),
            },
            ChatMessage {
                role: ChatRole::ToolResult {
                    call_id: "call-b".to_string(),
                    content: "result-b".to_string(),
                    is_error: true,
                },
                attachments: Vec::new(),
                content: String::new(),
            },
        ],
        reasoning_requested: false,
        max_new_tokens: Some(128),
        tools: Vec::new(),
        autonomy_mode: Default::default(),
        reasoning_option: Default::default(),
        capabilities: Default::default(),
    };

    let mut stream = adapter.stream_chat(request).await.unwrap();
    while stream.next().await.is_some() {}
}

#[test]
fn a_tool_found_by_the_search_is_sent_in_the_anthropic_tool_format() {
    let mut request = sample_request("claude-sonnet-4-20250514");
    request.tools = vec![ToolSpec {
        name: "settings_sync_setEnabled".to_string(),
        description: "Turn sync on or off".to_string(),
        input_schema: serde_json::json!({ "type": "object", "properties": {} }),
    }];
    let body = build_messages_body(&request);
    assert_eq!(
        body["tools"],
        serde_json::json!([{
            "name": "settings_sync_setEnabled",
            "description": "Turn sync on or off",
            "input_schema": { "type": "object", "properties": {} },
        }])
    );
}
