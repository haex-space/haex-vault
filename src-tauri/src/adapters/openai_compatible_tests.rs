//! Tests for the OpenAI-compatible adapter's transport and stream decoding (spec 055 R7), against
//! `wiremock` doubles. The request body itself is covered in `openai_request_tests.rs`.

use serde_json::json;
use wiremock::matchers::{body_partial_json, header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::openai_compatible::{token_limit_field_for_endpoint, OpenAiCompatibleAdapter};
use super::openai_request::TokenLimitField;
use super::types::{ChatMessage, ChatRequest, ChatRole, StreamChunk, StreamError};
use super::{AdapterError, ProviderAdapter};

#[test]
fn token_limit_field_is_selected_by_endpoint() {
    assert_eq!(
        token_limit_field_for_endpoint("https://api.openai.com/v1"),
        TokenLimitField::MaxCompletionTokens
    );
    assert_eq!(
        token_limit_field_for_endpoint("http://localhost:11434/v1"),
        TokenLimitField::MaxTokens
    );
    assert_eq!(
        token_limit_field_for_endpoint("http://school-server/v1"),
        TokenLimitField::MaxTokens
    );
}

fn request(reasoning: bool) -> ChatRequest {
    ChatRequest {
        sampling: Default::default(),
        model_id: "qwen3-vl".to_string(),
        thread_id: None,
        system_prompt: None,
        messages: vec![ChatMessage {
            role: ChatRole::User,
            content: "hi".to_string(),
            attachments: Vec::new(),
        }],
        reasoning_requested: reasoning,
        max_new_tokens: None,
        tools: Vec::new(),
        autonomy_mode: Default::default(),
        reasoning_option: None,
        capabilities: None,
    }
}

/// An SSE body: one `data:` event per entry, then the end marker.
fn sse(events: &[serde_json::Value], done: bool) -> ResponseTemplate {
    let mut body: String = events.iter().map(|e| format!("data: {e}\n\n")).collect();
    if done {
        body.push_str("data: [DONE]\n\n");
    }
    ResponseTemplate::new(200).set_body_raw(body, "text/event-stream")
}

async fn collect(
    adapter: &OpenAiCompatibleAdapter,
    req: ChatRequest,
) -> Vec<Result<StreamChunk, StreamError>> {
    let mut stream = adapter.stream_chat(req).await.expect("stream starts");
    let mut out = Vec::new();
    while let Some(item) = stream.next().await {
        out.push(item);
    }
    out
}

#[tokio::test]
async fn list_models_reads_the_ids_and_a_reported_context_size() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/models"))
        .and(header("authorization", "Bearer sk-test"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "object": "list",
            "data": [
                {"id": "qwen3-vl:8b", "object": "model", "max_model_len": 32768},
                {"id": "models/gemini-2.5-flash", "object": "model"},
                {"id": "zero-context", "context_length": 0},
            ],
        })))
        .expect(1)
        .mount(&server)
        .await;

    let adapter = OpenAiCompatibleAdapter::new(format!("{}/v1/", server.uri()), "sk-test".into())
        .expect("adapter");
    let models = adapter.list_models().await.expect("list succeeds");

    assert_eq!(models.len(), 3);
    assert_eq!(models[0].remote_id, "qwen3-vl:8b");
    assert_eq!(models[0].context_window, Some(32768));
    // The id stays as listed, the picker shows the short name.
    assert_eq!(models[1].remote_id, "models/gemini-2.5-flash");
    assert_eq!(models[1].display_name, "gemini-2.5-flash");
    // A zero is "not reported", and nothing is determined about what a model can do.
    assert_eq!(models[2].context_window, None);
    assert!(models[0].capabilities.accepted_attachment_kinds.is_none());
}

#[tokio::test]
async fn a_server_without_a_key_gets_no_authorization_header() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/models"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data": []})))
        .mount(&server)
        .await;

    let adapter = OpenAiCompatibleAdapter::new(server.uri(), String::new()).expect("adapter");
    adapter.list_models().await.expect("list succeeds");

    let requests = server.received_requests().await.expect("recorded");
    assert_eq!(requests.len(), 1);
    assert!(requests[0].headers.get("authorization").is_none());
}

#[tokio::test]
async fn a_rejected_key_is_invalid_credentials_and_never_echoed() {
    for status in [401, 403] {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/models"))
            .respond_with(ResponseTemplate::new(status).set_body_string("bad key sk-secret"))
            .mount(&server)
            .await;
        let adapter =
            OpenAiCompatibleAdapter::new(server.uri(), "sk-secret".into()).expect("adapter");

        let error = adapter.list_models().await.expect_err("rejected");

        assert!(matches!(error, AdapterError::InvalidCredentials), "{error}");
        assert!(!error.to_string().contains("sk-secret"));
    }
}

#[tokio::test]
async fn a_rate_limit_and_a_server_error_are_transient_but_a_bad_request_is_not() {
    for (status, transient) in [(429, true), (503, true), (400, false)] {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(status).set_body_string("nope"))
            .mount(&server)
            .await;
        let adapter = OpenAiCompatibleAdapter::new(server.uri(), String::new()).expect("adapter");

        let error = match adapter.stream_chat(request(false)).await {
            Ok(_) => panic!("status {status} must fail"),
            Err(error) => error,
        };

        assert!(matches!(error, AdapterError::Status { status: s, .. } if s == status));
        assert_eq!(error.is_transient(), transient, "status {status}");
    }
}

#[tokio::test]
async fn the_stream_gives_deltas_then_done_with_the_token_counts() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .and(body_partial_json(
            json!({"model": "qwen3-vl", "stream": true}),
        ))
        .respond_with(sse(
            &[
                json!({"choices": [{"delta": {"role": "assistant", "content": "Hel"}}]}),
                json!({"choices": [{"delta": {"content": "lo"}, "finish_reason": "stop"}]}),
                json!({"choices": [], "usage": {"prompt_tokens": 12, "completion_tokens": 3}}),
            ],
            true,
        ))
        .expect(1)
        .mount(&server)
        .await;
    let adapter = OpenAiCompatibleAdapter::new(server.uri(), String::new()).expect("adapter");

    let chunks = collect(&adapter, request(false)).await;

    let text: String = chunks
        .iter()
        .filter_map(|c| match c {
            Ok(StreamChunk::Delta { content, .. }) => Some(content.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(text, "Hello");
    let Some(Ok(StreamChunk::Done {
        finish_reason,
        prompt_tokens,
        completion_tokens,
        ttft_ms,
        ..
    })) = chunks.last()
    else {
        panic!("the stream must end with Done: {chunks:?}");
    };
    assert_eq!(finish_reason.as_deref(), Some("stop"));
    assert_eq!(*prompt_tokens, Some(12));
    assert_eq!(*completion_tokens, Some(3));
    assert!(ttft_ms.is_some());
}

#[tokio::test]
async fn reasoning_is_shown_only_when_it_was_asked_for() {
    let events = [
        json!({"choices": [{"delta": {"reasoning_content": "thinking"}}]}),
        json!({"choices": [{"delta": {"content": "answer"}, "finish_reason": "stop"}]}),
    ];
    for (asked, expect_reasoning) in [(true, true), (false, false)] {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(sse(&events, true))
            .mount(&server)
            .await;
        let adapter = OpenAiCompatibleAdapter::new(server.uri(), String::new()).expect("adapter");

        let chunks = collect(&adapter, request(asked)).await;

        let reasoning: Vec<&str> = chunks
            .iter()
            .filter_map(|c| match c {
                Ok(StreamChunk::Delta { reasoning, .. }) => reasoning.as_deref(),
                _ => None,
            })
            .collect();
        assert_eq!(reasoning == ["thinking"], expect_reasoning, "asked {asked}");
    }
}

#[tokio::test]
async fn tool_call_fragments_become_one_call_before_done() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(sse(
            &[
                json!({"choices": [{"delta": {"tool_calls": [
                    {"index": 0, "id": "call_1", "type": "function",
                     "function": {"name": "weather", "arguments": ""}}]}}]}),
                json!({"choices": [{"delta": {"tool_calls": [
                    {"index": 0, "function": {"arguments": "{\"city\":"}}]}}]}),
                json!({"choices": [{"delta": {"tool_calls": [
                    {"index": 0, "function": {"arguments": "\"Bonn\"}"}}]},
                    "finish_reason": "tool_calls"}]}),
            ],
            true,
        ))
        .mount(&server)
        .await;
    let adapter = OpenAiCompatibleAdapter::new(server.uri(), String::new()).expect("adapter");

    let chunks = collect(&adapter, request(false)).await;

    let calls: Vec<_> = chunks
        .iter()
        .filter_map(|c| match c {
            Ok(StreamChunk::ToolCalls(calls)) => Some(calls.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0][0].id, "call_1");
    assert_eq!(calls[0][0].name, "weather");
    assert_eq!(calls[0][0].input, json!({"city": "Bonn"}));
    // The calls come right before the closing frame.
    let tool_position = chunks
        .iter()
        .position(|c| matches!(c, Ok(StreamChunk::ToolCalls(_))))
        .expect("tool chunk");
    assert!(matches!(
        chunks[tool_position + 1],
        Ok(StreamChunk::Done { .. })
    ));
}

#[tokio::test]
async fn malformed_tool_arguments_end_the_stream_with_a_model_error() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(sse(
            &[
                json!({"choices": [{"delta": {"tool_calls": [
                    {"index": 0, "id": "call_1", "type": "function",
                     "function": {"name": "weather", "arguments": "{"}}
                ]}}]}),
                json!({"choices": [{"delta": {}, "finish_reason": "tool_calls"}]}),
            ],
            true,
        ))
        .mount(&server)
        .await;
    let adapter = OpenAiCompatibleAdapter::new(server.uri(), String::new()).expect("adapter");

    let chunks = collect(&adapter, request(false)).await;

    assert!(matches!(
        chunks.last(),
        Some(Err(StreamError::Model(message))) if message.contains("invalid tool arguments")
    ));
    assert!(!chunks
        .iter()
        .any(|chunk| matches!(chunk, Ok(StreamChunk::ToolCalls(_)))));
}

#[tokio::test]
async fn a_stream_that_closes_after_the_finish_reason_still_ends_with_done() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(sse(
            &[json!({"choices": [{"delta": {"content": "ok"}, "finish_reason": "stop"}]})],
            false,
        ))
        .mount(&server)
        .await;
    let adapter = OpenAiCompatibleAdapter::new(server.uri(), String::new()).expect("adapter");

    let chunks = collect(&adapter, request(false)).await;

    assert!(
        matches!(chunks.last(), Some(Ok(StreamChunk::Done { .. }))),
        "{chunks:?}"
    );
}

#[tokio::test]
async fn a_stream_that_stops_without_any_finish_is_an_unexpected_end() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(sse(
            &[json!({"choices": [{"delta": {"content": "half"}}]})],
            false,
        ))
        .mount(&server)
        .await;
    let adapter = OpenAiCompatibleAdapter::new(server.uri(), String::new()).expect("adapter");

    let chunks = collect(&adapter, request(false)).await;

    assert!(
        matches!(chunks.last(), Some(Err(StreamError::UnexpectedEnd))),
        "{chunks:?}"
    );
}

#[tokio::test]
async fn an_error_inside_the_stream_is_classified() {
    for (error, transient) in [
        (
            json!({"message": "slow down", "type": "rate_limit_error"}),
            true,
        ),
        (
            json!({"message": "overloaded", "type": "server_error"}),
            true,
        ),
        (
            json!({"message": "bad model", "type": "invalid_request_error"}),
            false,
        ),
    ] {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(sse(&[json!({"error": error})], false))
            .mount(&server)
            .await;
        let adapter = OpenAiCompatibleAdapter::new(server.uri(), String::new()).expect("adapter");

        let chunks = collect(&adapter, request(false)).await;

        let Some(Err(error)) = chunks.last() else {
            panic!("an error must end the stream: {chunks:?}");
        };
        assert_eq!(error.is_transient(), transient, "{error}");
    }
}
