//! Tests for the Anthropic streaming path: SSE decoding, malformed and
//! truncated streams, and the transient-vs-terminal classification the
//! retry budget in `chat::commands` depends on.

use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::anthropic::AnthropicAdapter;
use super::types::{ChatMessage, ChatRequest, ChatRole, StreamChunk, StreamError};
use super::{AdapterError, ProviderAdapter};

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

fn sse_body(events: &[(&str, serde_json::Value)]) -> String {
    let mut out = String::new();
    for (event, data) in events {
        out.push_str(&format!("event: {event}\n"));
        out.push_str(&format!("data: {}\n\n", data));
    }
    out
}

#[tokio::test]
async fn stream_chat_emits_deltas_and_done_with_token_counts() {
    let server = MockServer::start().await;
    let body = sse_body(&[
        (
            "message_start",
            serde_json::json!({
                "type": "message_start",
                "message": {
                    "id": "msg_1",
                    "usage": {"input_tokens": 12, "output_tokens": 0}
                }
            }),
        ),
        (
            "content_block_delta",
            serde_json::json!({
                "type": "content_block_delta",
                "index": 0,
                "delta": {"type": "text_delta", "text": "Hello"}
            }),
        ),
        (
            "content_block_delta",
            serde_json::json!({
                "type": "content_block_delta",
                "index": 0,
                "delta": {"type": "text_delta", "text": " world"}
            }),
        ),
        (
            "message_delta",
            serde_json::json!({
                "type": "message_delta",
                "delta": {"stop_reason": "end_turn"},
                "usage": {"output_tokens": 3}
            }),
        ),
        ("message_stop", serde_json::json!({"type": "message_stop"})),
    ]);
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .and(header("x-api-key", "sk-test"))
        .and(header("anthropic-version", "2023-06-01"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(body)
                .insert_header("content-type", "text/event-stream"),
        )
        .expect(1)
        .mount(&server)
        .await;

    let adapter = AnthropicAdapter::new(server.uri(), "sk-test".to_string()).unwrap();
    let mut stream = adapter
        .stream_chat(sample_request("claude-opus-5"))
        .await
        .expect("stream_chat starts");

    let mut deltas: Vec<String> = Vec::new();
    let mut finished: Option<(Option<usize>, Option<usize>, Option<String>)> = None;
    while let Some(chunk) = stream.next().await {
        match chunk.expect("no error frame") {
            StreamChunk::Delta { content, reasoning } => {
                assert!(reasoning.is_none(), "no thinking in this fixture");
                if !content.is_empty() {
                    deltas.push(content);
                }
            }
            StreamChunk::Done {
                prompt_tokens,
                completion_tokens,
                finish_reason,
                ..
            } => {
                finished = Some((prompt_tokens, completion_tokens, finish_reason));
                break;
            }
            StreamChunk::ToolCalls(_) => panic!("this fixture emits no tool_use blocks"),
            StreamChunk::AgentActivity { .. } => {
                panic!("the direct Anthropic API adapter never emits agent activity")
            }
        }
    }

    assert_eq!(deltas.join(""), "Hello world");
    let (pt, ct, fr) = finished.expect("Done frame arrived");
    assert_eq!(pt, Some(12));
    assert_eq!(ct, Some(3));
    assert_eq!(fr.as_deref(), Some("end_turn"));
}

#[tokio::test]
/// A stream that ends after a delta without `message_stop` is truncated.
async fn stream_chat_reports_unexpected_end_after_delta() {
    let server = MockServer::start().await;
    let body = sse_body(&[(
        "content_block_delta",
        serde_json::json!({
            "type": "content_block_delta",
            "delta": {"type": "text_delta", "text": "partial"}
        }),
    )]);
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(body)
                .insert_header("content-type", "text/event-stream"),
        )
        .mount(&server)
        .await;

    let adapter = AnthropicAdapter::new(server.uri(), "sk-any".to_string()).unwrap();
    let mut request = sample_request("claude-opus-5");
    request.reasoning_requested = true;
    let mut stream = adapter.stream_chat(request).await.unwrap();
    let mut saw_unexpected_end = false;
    while let Some(chunk) = stream.next().await {
        if matches!(chunk, Err(StreamError::UnexpectedEnd)) {
            saw_unexpected_end = true;
            break;
        }
    }
    assert!(saw_unexpected_end, "truncated stream must not complete");
}

#[tokio::test]
async fn stream_chat_surfaces_thinking_delta_as_reasoning() {
    let server = MockServer::start().await;
    let body = sse_body(&[
        (
            "message_start",
            serde_json::json!({
                "type": "message_start",
                "message": {"id": "m", "usage": {"input_tokens": 4, "output_tokens": 0}}
            }),
        ),
        (
            "content_block_delta",
            serde_json::json!({
                "type": "content_block_delta",
                "index": 0,
                "delta": {"type": "thinking_delta", "thinking": "let me think"}
            }),
        ),
        (
            "content_block_delta",
            serde_json::json!({
                "type": "content_block_delta",
                "index": 0,
                "delta": {"type": "text_delta", "text": "42"}
            }),
        ),
        ("message_stop", serde_json::json!({"type": "message_stop"})),
    ]);
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(body)
                .insert_header("content-type", "text/event-stream"),
        )
        .mount(&server)
        .await;

    let adapter = AnthropicAdapter::new(server.uri(), "sk-any".to_string()).unwrap();
    let mut request = sample_request("claude-opus-5");
    request.reasoning_requested = true;
    let mut stream = adapter.stream_chat(request).await.unwrap();

    let mut reasoning: Vec<String> = Vec::new();
    let mut content: Vec<String> = Vec::new();
    while let Some(chunk) = stream.next().await {
        match chunk.unwrap() {
            StreamChunk::Delta {
                content: c,
                reasoning: r,
            } => {
                if let Some(r) = r {
                    reasoning.push(r);
                }
                if !c.is_empty() {
                    content.push(c);
                }
            }
            StreamChunk::Done { .. } => break,
            StreamChunk::ToolCalls(_) => panic!("this fixture emits no tool_use blocks"),
            StreamChunk::AgentActivity { .. } => {
                panic!("the direct Anthropic API adapter never emits agent activity")
            }
        }
    }
    assert_eq!(reasoning, vec!["let me think"]);
    assert_eq!(content.join(""), "42");
}

#[tokio::test]
async fn stream_chat_maps_401_to_invalid_credentials() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ResponseTemplate::new(401).set_body_json(serde_json::json!({
            "type": "error",
            "error": {"type": "authentication_error", "message": "bad key"}
        })))
        .mount(&server)
        .await;

    let adapter = AnthropicAdapter::new(server.uri(), "sk-bad".to_string()).unwrap();
    let result = adapter.stream_chat(sample_request("claude-opus-5")).await;
    match result {
        Err(AdapterError::InvalidCredentials) => {}
        Err(other) => panic!("expected InvalidCredentials, got {other:?}"),
        Ok(_) => panic!("expected error, got Ok"),
    }
}

async fn stream_chat_sse_error(error_type: &str, message: &str) -> StreamError {
    let server = MockServer::start().await;
    let body = sse_body(&[
        (
            "message_start",
            serde_json::json!({
                "type": "message_start",
                "message": {"id": "m", "usage": {"input_tokens": 1, "output_tokens": 0}}
            }),
        ),
        (
            "error",
            serde_json::json!({
                "type": "error",
                "error": {"type": error_type, "message": message}
            }),
        ),
    ]);
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(body)
                .insert_header("content-type", "text/event-stream"),
        )
        .mount(&server)
        .await;

    let adapter = AnthropicAdapter::new(server.uri(), "sk-any".to_string()).unwrap();
    let mut stream = adapter
        .stream_chat(sample_request("claude-opus-5"))
        .await
        .unwrap();
    let mut saw_error: Option<StreamError> = None;
    while let Some(chunk) = stream.next().await {
        match chunk {
            Ok(_) => continue,
            Err(e) => {
                saw_error = Some(e);
                break;
            }
        }
    }
    saw_error.expect("stream must deliver the SSE error event")
}

/// T034: Anthropic's own transient categories (overloaded/rate-limited/
/// internal 5xx-equivalent) must classify as retry-eligible.
#[tokio::test]
async fn stream_chat_maps_overloaded_error_to_transient() {
    let error = stream_chat_sse_error("overloaded_error", "overloaded").await;
    match &error {
        StreamError::Transient(msg) => assert!(msg.contains("overloaded"), "{msg}"),
        other => panic!("expected Transient error, got {other:?}"),
    }
    assert!(error.is_transient());
}

#[tokio::test]
async fn stream_chat_maps_rate_limit_error_to_transient() {
    let error = stream_chat_sse_error("rate_limit_error", "rate limited").await;
    assert!(matches!(error, StreamError::Transient(_)));
    assert!(error.is_transient());
}

#[tokio::test]
async fn stream_chat_maps_api_error_to_transient() {
    let error = stream_chat_sse_error("api_error", "internal server error").await;
    assert!(matches!(error, StreamError::Transient(_)));
    assert!(error.is_transient());
}

/// T034: a request-shaped rejection (4xx-equivalent) must stay terminal —
/// retrying the exact same request would just reproduce it.
#[tokio::test]
async fn stream_chat_maps_invalid_request_error_to_terminal_model_error() {
    let error = stream_chat_sse_error("invalid_request_error", "bad request").await;
    match &error {
        StreamError::Model(msg) => assert!(msg.contains("bad request"), "{msg}"),
        other => panic!("expected Model error, got {other:?}"),
    }
    assert!(!error.is_transient());
}

/// T034: `AdapterError`/`StreamError` classification used by the Phase 6
/// retry wrapper (spec.md FR-012) — a pure unit test over the enum, no
/// network involved.
#[test]
fn adapter_error_classifies_transient_vs_terminal() {
    assert!(AdapterError::Http {
        reason: "connect timed out".into()
    }
    .is_transient());
    assert!(AdapterError::Status {
        status: 429,
        body: String::new()
    }
    .is_transient());
    assert!(AdapterError::Status {
        status: 500,
        body: String::new()
    }
    .is_transient());
    assert!(AdapterError::Status {
        status: 503,
        body: String::new()
    }
    .is_transient());
    assert!(!AdapterError::Status {
        status: 400,
        body: String::new()
    }
    .is_transient());
    assert!(!AdapterError::Status {
        status: 404,
        body: String::new()
    }
    .is_transient());
    assert!(!AdapterError::InvalidCredentials.is_transient());
    assert!(!AdapterError::Parse {
        reason: "bad json".into()
    }
    .is_transient());
}

#[test]
fn stream_error_classifies_transient_vs_terminal() {
    assert!(StreamError::Transient("x".into()).is_transient());
    assert!(StreamError::Internal("x".into()).is_transient());
    assert!(StreamError::UnexpectedEnd.is_transient());
    assert!(!StreamError::Model("x".into()).is_transient());
    assert!(!StreamError::Validation("x".into()).is_transient());
    assert!(!StreamError::StartFailed("x".into()).is_transient());
}

#[tokio::test]
async fn tool_arguments_that_are_not_json_end_the_stream_with_a_model_error() {
    let server = MockServer::start().await;
    let body = sse_body(&[
        (
            "content_block_start",
            serde_json::json!({
                "type": "content_block_start", "index": 0,
                "content_block": {"type": "tool_use", "id": "toolu_1", "name": "weather", "input": {}}
            }),
        ),
        (
            "content_block_delta",
            serde_json::json!({
                "type": "content_block_delta", "index": 0,
                "delta": {"type": "input_json_delta", "partial_json": "{\"city\": \"Bo"}
            }),
        ),
        (
            "content_block_stop",
            serde_json::json!({"type": "content_block_stop", "index": 0}),
        ),
        ("message_stop", serde_json::json!({"type": "message_stop"})),
    ]);
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(body, "text/event-stream"))
        .mount(&server)
        .await;
    let adapter = AnthropicAdapter::new(server.uri(), "key".to_string()).expect("adapter");
    let mut stream = adapter
        .stream_chat(sample_request("claude-x"))
        .await
        .expect("stream starts");

    let mut chunks = Vec::new();
    while let Some(chunk) = stream.next().await {
        chunks.push(chunk);
    }

    assert!(
        matches!(chunks.last(), Some(Err(StreamError::Model(m))) if m.contains("invalid tool arguments for weather")),
        "{chunks:?}"
    );
    assert!(!chunks
        .iter()
        .any(|c| matches!(c, Ok(StreamChunk::ToolCalls(_)))));
}
