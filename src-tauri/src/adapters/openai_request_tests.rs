//! Tests for the chat-completions request body of the OpenAI-compatible adapter (spec 055 R7).

use serde_json::json;

use super::openai_request::build_chat_body;
use super::types::{Attachment, AttachmentKind, ChatMessage, ChatRequest, ChatRole, ToolSpec};
use crate::model_capabilities::{ModelCapabilities, ReasoningControl, ReasoningOption};

fn message(role: ChatRole, content: &str) -> ChatMessage {
    ChatMessage {
        role,
        content: content.to_string(),
        attachments: Vec::new(),
    }
}

fn request(messages: Vec<ChatMessage>) -> ChatRequest {
    ChatRequest {
        sampling: Default::default(),
        model_id: "qwen3-vl".to_string(),
        thread_id: None,
        system_prompt: None,
        messages,
        reasoning_requested: false,
        max_new_tokens: None,
        tools: Vec::new(),
        autonomy_mode: Default::default(),
        reasoning_option: None,
        capabilities: None,
    }
}

fn image(bytes: &[u8]) -> Attachment {
    Attachment {
        name: "page.jpg".to_string(),
        kind: AttachmentKind::Image,
        media_type: "image/jpeg".to_string(),
        bytes: bytes.to_vec(),
    }
}

fn presets(ids: &[&str]) -> ReasoningControl {
    ReasoningControl::presets(
        ids.iter()
            .map(|id| ReasoningOption {
                id: id.to_string(),
                label: id.to_string(),
            })
            .collect(),
    )
}

#[test]
fn a_plain_request_streams_and_asks_for_the_token_counts() {
    let body = build_chat_body(&request(vec![message(ChatRole::User, "hi")]));

    assert_eq!(body["model"], "qwen3-vl");
    assert_eq!(body["stream"], true);
    assert_eq!(body["stream_options"], json!({"include_usage": true}));
    assert_eq!(body["messages"], json!([{"role": "user", "content": "hi"}]));
    // Nothing optional unless asked for.
    for key in ["max_tokens", "reasoning_effort", "tools"] {
        assert!(body.get(key).is_none(), "{key} must be absent");
    }
}

#[test]
fn a_gemini_models_prefix_is_removed_only_from_the_wire_model_id() {
    let mut req = request(vec![message(ChatRole::User, "hi")]);
    req.model_id = "models/gemini-3.8-flash".to_string();

    assert_eq!(build_chat_body(&req)["model"], "gemini-3.8-flash");
    assert_eq!(req.model_id, "models/gemini-3.8-flash");
}

#[test]
fn the_system_prompt_leads_and_the_cap_is_sent() {
    let mut req = request(vec![message(ChatRole::User, "hi")]);
    req.system_prompt = Some("Be brief.".to_string());
    req.max_new_tokens = Some(256);
    let body = build_chat_body(&req);

    assert_eq!(
        body["messages"][0],
        json!({"role": "system", "content": "Be brief."})
    );
    assert_eq!(body["messages"][1]["role"], "user");
    assert_eq!(body["max_tokens"], 256);
}

#[test]
fn an_openai_o_series_model_uses_the_completion_token_field() {
    let mut req = request(vec![message(ChatRole::User, "hi")]);
    req.model_id = "o3-mini".to_string();
    req.max_new_tokens = Some(256);

    let body = build_chat_body(&req);

    assert_eq!(body["max_completion_tokens"], 256);
    assert!(body.get("max_tokens").is_none());
}

#[test]
fn an_image_goes_as_a_data_uri_part_after_the_text() {
    let mut user = message(ChatRole::User, "Read this page");
    user.attachments.push(image(&[1, 2, 3]));
    let body = build_chat_body(&request(vec![user]));

    assert_eq!(
        body["messages"][0]["content"],
        json!([
            {"type": "text", "text": "Read this page"},
            {"type": "image_url", "image_url": {"url": "data:image/jpeg;base64,AQID"}},
        ])
    );
}

#[test]
fn a_text_file_is_inlined_and_a_document_is_named_but_not_sent() {
    let mut user = message(ChatRole::User, "");
    user.attachments.push(Attachment {
        name: "notes.txt".to_string(),
        kind: AttachmentKind::Text,
        media_type: "text/plain".to_string(),
        bytes: b"hello".to_vec(),
    });
    user.attachments.push(Attachment {
        name: "sheet.pdf".to_string(),
        kind: AttachmentKind::Document,
        media_type: "application/pdf".to_string(),
        bytes: vec![0, 1],
    });
    let parts = build_chat_body(&request(vec![user]))["messages"][0]["content"].clone();

    assert_eq!(parts[0]["text"], "[Attached file: notes.txt]\nhello");
    let document = parts[1]["text"].as_str().expect("text part");
    assert!(document.contains("sheet.pdf"), "{document}");
    assert!(document.contains("not sent"), "{document}");
}

#[test]
fn the_effort_is_sent_only_when_the_model_offers_it() {
    let capabilities = |ids: &[&str]| {
        Some(ModelCapabilities {
            reasoning: Some(presets(ids)),
            ..ModelCapabilities::default()
        })
    };
    let mut req = request(vec![message(ChatRole::User, "hi")]);
    req.reasoning_option = Some("high".to_string());

    req.capabilities = capabilities(&["low", "high"]);
    assert_eq!(build_chat_body(&req)["reasoning_effort"], "high");

    req.capabilities = capabilities(&["low", "medium"]);
    assert!(build_chat_body(&req).get("reasoning_effort").is_none());

    req.capabilities = None;
    assert!(build_chat_body(&req).get("reasoning_effort").is_none());
}

#[test]
fn tools_become_function_definitions() {
    let mut req = request(vec![message(ChatRole::User, "hi")]);
    req.tools.push(ToolSpec {
        name: "lookup".to_string(),
        description: "Looks a word up".to_string(),
        input_schema: json!({"type": "object", "properties": {"word": {"type": "string"}}}),
    });
    let body = build_chat_body(&req);

    assert_eq!(
        body["tools"],
        json!([{
            "type": "function",
            "function": {
                "name": "lookup",
                "description": "Looks a word up",
                "parameters": {"type": "object", "properties": {"word": {"type": "string"}}},
            },
        }])
    );
}

#[test]
fn tool_calls_and_their_results_are_regrouped() {
    let history = vec![
        message(ChatRole::User, "weather?"),
        message(ChatRole::Assistant, "Let me look."),
        message(
            ChatRole::ToolCall {
                id: "c1".to_string(),
                name: "weather".to_string(),
                input: json!({"city": "Bonn"}),
            },
            "",
        ),
        message(
            ChatRole::ToolCall {
                id: "c2".to_string(),
                name: "time".to_string(),
                input: json!({}),
            },
            "",
        ),
        message(
            ChatRole::ToolResult {
                call_id: "c1".to_string(),
                content: "sunny".to_string(),
                is_error: false,
            },
            "",
        ),
        message(
            ChatRole::ToolResult {
                call_id: "c2".to_string(),
                content: "noon".to_string(),
                is_error: false,
            },
            "",
        ),
    ];
    let messages = build_chat_body(&request(history))["messages"].clone();

    assert_eq!(messages.as_array().expect("array").len(), 4);
    assert_eq!(messages[1]["role"], "assistant");
    assert_eq!(messages[1]["content"], "Let me look.");
    assert_eq!(
        messages[1]["tool_calls"].as_array().expect("calls").len(),
        2
    );
    assert_eq!(messages[1]["tool_calls"][0]["id"], "c1");
    assert_eq!(messages[1]["tool_calls"][0]["type"], "function");
    // The arguments travel as a JSON string, as the format wants.
    assert_eq!(
        messages[1]["tool_calls"][0]["function"]["arguments"],
        "{\"city\":\"Bonn\"}"
    );
    assert_eq!(
        messages[2],
        json!({"role": "tool", "tool_call_id": "c1", "content": "sunny"})
    );
    assert_eq!(messages[3]["tool_call_id"], "c2");
}

#[test]
fn tool_calls_without_text_have_null_content() {
    let history = vec![
        message(ChatRole::User, "go"),
        message(
            ChatRole::ToolCall {
                id: "c1".to_string(),
                name: "weather".to_string(),
                input: json!({}),
            },
            "",
        ),
    ];
    let messages = build_chat_body(&request(history))["messages"].clone();

    assert_eq!(messages[1]["role"], "assistant");
    assert!(messages[1]["content"].is_null());
}

#[test]
fn the_images_of_a_tool_result_follow_as_a_user_message() {
    let mut result = message(
        ChatRole::ToolResult {
            call_id: "c1".to_string(),
            content: "screenshot taken".to_string(),
            is_error: false,
        },
        "",
    );
    result.attachments.push(image(&[9]));
    let messages = build_chat_body(&request(vec![result]))["messages"].clone();

    assert_eq!(messages[0]["role"], "tool");
    assert_eq!(messages[1]["role"], "user");
    assert_eq!(messages[1]["content"][0]["type"], "image_url");
}
