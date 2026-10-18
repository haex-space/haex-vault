//! Anthropic HTTP adapter.
//!
//! Reference: <https://docs.claude.com/en/api/models-list> and
//! <https://docs.claude.com/en/api/versioning>. Uses the stable
//! `2023-06-01` version header (see [`ANTHROPIC_VERSION`]). Additive
//! output fields under that version are safe — the deserializer
//! ignores unknown keys.

use std::time::{Duration, Instant};

use async_trait::async_trait;
use eventsource_stream::Eventsource;
use futures::StreamExt;
use reqwest::Client;
use serde::Deserialize;
use serde_json::Value;
use tokio::sync::mpsc;

use super::anthropic_capabilities::{map_capabilities, WireCapabilities};
use super::request::build_messages_body;
use super::types::{AdapterStream, ChatRequest, StreamChunk, StreamError, ToolCall};
use super::{AdapterError, ProviderAdapter, ProviderModel};

/// The pinned Anthropic API version. Per docs, additive optional
/// inputs and outputs are allowed inside a version without breaking
/// callers.
const ANTHROPIC_VERSION: &str = "2023-06-01";

/// Anthropic caps `limit` at 1000. The catalog is small (~a dozen
/// entries) so one page suffices in practice; the loop paginates on
/// `has_more` for correctness.
const PAGE_LIMIT: u16 = 1000;

/// Budget for the finite model-list request. Streaming requests are
/// intentionally long-lived and are governed by the caller's abort handle.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// Adapter for `api_key`-kind providers pointed at the Anthropic
/// Messages API. Constructed per refresh from the provider row plus
/// its stored credentials blob.
pub struct AnthropicAdapter {
    client: Client,
    base_url: String,
    api_key: String,
}

impl AnthropicAdapter {
    /// Builds an adapter. `base_url` should be
    /// `https://api.anthropic.com` for production; tests pass a
    /// wiremock URL. Trailing slashes on `base_url` are trimmed so
    /// callers do not need to normalise.
    pub fn new(base_url: String, api_key: String) -> Result<Self, AdapterError> {
        let client = Client::builder()
            .connect_timeout(Duration::from_secs(15))
            .user_agent(concat!("holzi/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|e| AdapterError::Http {
                reason: format!("build reqwest client: {e}"),
            })?;
        Ok(Self {
            client,
            base_url,
            api_key,
        })
    }
}

#[derive(Deserialize)]
struct ListModelsPage {
    data: Vec<ModelInfo>,
    has_more: bool,
    last_id: Option<String>,
}

#[derive(Deserialize)]
struct ModelInfo {
    id: String,
    display_name: String,
    /// Nullable integer in the response. A returned `0` is treated as
    /// "not reported" so the picker does not misrender a zero-width
    /// context.
    #[serde(default)]
    max_input_tokens: Option<i64>,
    /// Per-model capability tree (effort levels, thinking, image/PDF input).
    /// Absent on responses that omit it, which maps to "not determined".
    #[serde(default)]
    capabilities: Option<WireCapabilities>,
}

/// How a caller of [`fetch_models`] authenticates to `/v1/models`. Both
/// variants hit the exact same public endpoint (docs.claude.com/en/api/
/// models-list) — only the credential shape differs: a metered API key
/// (`api_key`-kind providers) vs. the OAuth access token `claude
/// setup-token` issues for a subscription (`cli_delegate` providers,
/// which have no API key of their own — see `cli_delegate::fetch_claude_models`).
pub(crate) enum ModelsAuth {
    ApiKey(String),
    /// `anthropic-beta: oauth-2025-04-20` is required alongside the
    /// bearer token — without it the API treats the request as
    /// unauthenticated `x-api-key` traffic and rejects it, since OAuth
    /// bearer auth is otherwise opt-in per request.
    OAuthBearer(String),
}

impl ModelsAuth {
    fn apply(&self, req: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        match self {
            ModelsAuth::ApiKey(key) => req.header("x-api-key", key),
            ModelsAuth::OAuthBearer(token) => req
                .header("authorization", format!("Bearer {token}"))
                .header("anthropic-beta", "oauth-2025-04-20"),
        }
    }
}

/// Fetches the live model catalog from `/v1/models`, paginating on
/// `has_more`. Shared by [`AnthropicAdapter::list_models`] and the
/// `cli_delegate` Claude adapter (`cli_delegate::fetch_claude_models`) so
/// neither provider kind hardcodes a model list — both always reflect
/// whatever Anthropic's API currently reports.
pub(crate) async fn fetch_models(
    client: &Client,
    base_url: &str,
    auth: &ModelsAuth,
) -> Result<Vec<ProviderModel>, AdapterError> {
    let endpoint = format!("{}/v1/models", base_url.trim_end_matches('/'));
    let limit_str = PAGE_LIMIT.to_string();
    let mut out: Vec<ProviderModel> = Vec::new();
    let mut after_id: Option<String> = None;

    loop {
        let mut query: Vec<(&str, &str)> = vec![("limit", limit_str.as_str())];
        if let Some(after) = after_id.as_deref() {
            query.push(("after_id", after));
        }
        let req = auth.apply(
            client
                .get(&endpoint)
                .timeout(REQUEST_TIMEOUT)
                .query(&query)
                .header("anthropic-version", ANTHROPIC_VERSION),
        );
        let resp = req
            .send()
            .await
            .map_err(|e| super::transport_error(format!("GET {endpoint}"), e))?;

        let status = resp.status();
        if !status.is_success() {
            let body = truncate_body(resp.text().await.unwrap_or_default());
            return Err(match status.as_u16() {
                401 | 403 => AdapterError::InvalidCredentials,
                other => AdapterError::Status {
                    status: other,
                    body,
                },
            });
        }

        let page: ListModelsPage = resp.json().await.map_err(|e| AdapterError::Parse {
            reason: format!("decode /v1/models: {e}"),
        })?;
        for m in page.data {
            out.push(ProviderModel {
                remote_id: m.id,
                display_name: m.display_name,
                context_window: m.max_input_tokens.filter(|n| *n > 0),
                capabilities: map_capabilities(&m.capabilities.unwrap_or_default()),
            });
        }
        if !page.has_more {
            break;
        }
        // Only follow the cursor when it actually advances. A
        // provider that keeps answering `has_more: true` with a
        // missing or unchanged `last_id` would otherwise spin
        // forever, re-fetching the same page and growing `out`
        // without bound.
        match page.last_id {
            Some(id) if after_id.as_deref() != Some(id.as_str()) => after_id = Some(id),
            Some(_) => {
                return Err(AdapterError::Parse {
                    reason: "pagination response has_more=true without an advancing last_id".into(),
                });
            }
            None => {
                return Err(AdapterError::Parse {
                    reason: "pagination response has_more=true without last_id".into(),
                });
            }
        }
    }
    Ok(out)
}

#[async_trait]
impl ProviderAdapter for AnthropicAdapter {
    async fn list_models(&self) -> Result<Vec<ProviderModel>, AdapterError> {
        fetch_models(
            &self.client,
            &self.base_url,
            &ModelsAuth::ApiKey(self.api_key.clone()),
        )
        .await
    }

    async fn stream_chat(&self, req: ChatRequest) -> Result<AdapterStream, AdapterError> {
        let endpoint = format!("{}/v1/messages", self.base_url.trim_end_matches('/'));
        let body = build_messages_body(&req);
        // The response body is intentionally long-lived — abort via
        // `AdapterStream::abort_handle` if the caller wants to cancel.
        let resp = self
            .client
            .post(&endpoint)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", ANTHROPIC_VERSION)
            .header("accept", "text/event-stream")
            .json(&body)
            .send()
            .await
            .map_err(|e| super::transport_error(format!("POST {endpoint}"), e))?;

        let status = resp.status();
        if !status.is_success() {
            let body_text = truncate_body(resp.text().await.unwrap_or_default());
            return Err(match status.as_u16() {
                401 | 403 => AdapterError::InvalidCredentials,
                other => AdapterError::Status {
                    status: other,
                    body: body_text,
                },
            });
        }

        let (tx, rx) = mpsc::unbounded_channel();
        let start = Instant::now();
        let byte_stream = resp.bytes_stream();

        let reasoning_requested = req.reasoning_requested;
        let task = tokio::spawn(async move {
            let mut event_stream = byte_stream.eventsource();
            let mut prompt_tokens: Option<usize> = None;
            let mut completion_tokens: Option<usize> = None;
            let mut ttft_ms: Option<u64> = None;
            let mut finish_reason: Option<String> = None;
            let mut done_emitted = false;
            // Buffers one in-progress `tool_use` content block per index
            // (id, name, concatenated `partial_json`) until its
            // `content_block_stop` closes it (research.md §1).
            let mut tool_use_bufs: std::collections::BTreeMap<u64, (String, String, String)> =
                std::collections::BTreeMap::new();
            let mut tool_calls: Vec<ToolCall> = Vec::new();

            while let Some(event) = event_stream.next().await {
                let event = match event {
                    Ok(e) => e,
                    Err(e) => {
                        let _ = tx.send(Err(StreamError::Internal(format!("sse decode: {e}"))));
                        return;
                    }
                };
                let payload: Value = match serde_json::from_str(&event.data) {
                    Ok(v) => v,
                    Err(_) => continue,
                };
                match event.event.as_str() {
                    "message_start" => {
                        if let Some(n) = payload
                            .pointer("/message/usage/input_tokens")
                            .and_then(Value::as_u64)
                        {
                            prompt_tokens = Some(n as usize);
                        }
                    }
                    "content_block_start" => {
                        if payload
                            .pointer("/content_block/type")
                            .and_then(Value::as_str)
                            == Some("tool_use")
                        {
                            let index = payload
                                .pointer("/index")
                                .and_then(Value::as_u64)
                                .unwrap_or(0);
                            let id = payload
                                .pointer("/content_block/id")
                                .and_then(Value::as_str)
                                .unwrap_or_default()
                                .to_string();
                            let name = payload
                                .pointer("/content_block/name")
                                .and_then(Value::as_str)
                                .unwrap_or_default()
                                .to_string();
                            tool_use_bufs.insert(index, (id, name, String::new()));
                        }
                    }
                    "content_block_delta" => {
                        let Some(delta_type) =
                            payload.pointer("/delta/type").and_then(Value::as_str)
                        else {
                            continue;
                        };
                        match delta_type {
                            "text_delta" => {
                                if let Some(text) =
                                    payload.pointer("/delta/text").and_then(Value::as_str)
                                {
                                    if !text.is_empty() && ttft_ms.is_none() {
                                        ttft_ms = Some(start.elapsed().as_millis() as u64);
                                    }
                                    if tx
                                        .send(Ok(StreamChunk::Delta {
                                            content: text.to_string(),
                                            reasoning: None,
                                        }))
                                        .is_err()
                                    {
                                        return;
                                    }
                                }
                            }
                            "thinking_delta" if reasoning_requested => {
                                if let Some(text) =
                                    payload.pointer("/delta/thinking").and_then(Value::as_str)
                                {
                                    if tx
                                        .send(Ok(StreamChunk::Delta {
                                            content: String::new(),
                                            reasoning: Some(text.to_string()),
                                        }))
                                        .is_err()
                                    {
                                        return;
                                    }
                                }
                            }
                            "input_json_delta" => {
                                let index = payload
                                    .pointer("/index")
                                    .and_then(Value::as_u64)
                                    .unwrap_or(0);
                                if let Some(partial) = payload
                                    .pointer("/delta/partial_json")
                                    .and_then(Value::as_str)
                                {
                                    if let Some(buf) = tool_use_bufs.get_mut(&index) {
                                        buf.2.push_str(partial);
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                    "content_block_stop" => {
                        let index = payload
                            .pointer("/index")
                            .and_then(Value::as_u64)
                            .unwrap_or(0);
                        if let Some((id, name, partial_json)) = tool_use_bufs.remove(&index) {
                            // Arguments that are not JSON (cut off, say) must not run the tool
                            // with empty ones.
                            let input: Value = if partial_json.trim().is_empty() {
                                serde_json::json!({})
                            } else {
                                match serde_json::from_str(&partial_json) {
                                    Ok(input) => input,
                                    Err(error) => {
                                        let _ = tx.send(Err(StreamError::Model(format!(
                                            "invalid tool arguments for {name}: {error}"
                                        ))));
                                        return;
                                    }
                                }
                            };
                            tool_calls.push(ToolCall { id, name, input });
                        }
                    }
                    "message_delta" => {
                        if let Some(n) = payload
                            .pointer("/usage/output_tokens")
                            .and_then(Value::as_u64)
                        {
                            completion_tokens = Some(n as usize);
                        }
                        if let Some(reason) = payload
                            .pointer("/delta/stop_reason")
                            .and_then(Value::as_str)
                        {
                            finish_reason = Some(reason.to_string());
                        }
                    }
                    "message_stop" => {
                        if !tool_calls.is_empty() {
                            let _ = tx
                                .send(Ok(StreamChunk::ToolCalls(std::mem::take(&mut tool_calls))));
                        }
                        let done = StreamChunk::Done {
                            finish_reason: finish_reason.clone(),
                            prompt_tokens,
                            completion_tokens,
                            ttft_ms,
                            total_ms: start.elapsed().as_millis() as u64,
                        };
                        let _ = tx.send(Ok(done));
                        done_emitted = true;
                        break;
                    }
                    "error" => {
                        let error_type = payload
                            .pointer("/error/type")
                            .and_then(Value::as_str)
                            .unwrap_or_default();
                        let message = payload
                            .pointer("/error/message")
                            .and_then(Value::as_str)
                            .unwrap_or("anthropic sse error")
                            .to_string();
                        // Anthropic's own transient categories (docs:
                        // errors) — everything else (invalid_request_error,
                        // authentication_error, permission_error,
                        // not_found_error, ...) reproduces deterministically
                        // for the same request (spec.md FR-012, T034).
                        let err = match error_type {
                            "overloaded_error" | "rate_limit_error" | "api_error" => {
                                StreamError::Transient(message)
                            }
                            _ => StreamError::Model(message),
                        };
                        let _ = tx.send(Err(err));
                        done_emitted = true;
                        break;
                    }
                    _ => {}
                }
            }

            if !done_emitted {
                let _ = tx.send(Err(StreamError::UnexpectedEnd));
            }
        });

        Ok(AdapterStream::new(rx, task.abort_handle()))
    }
}

/// Caps a provider-supplied error body before it travels into a
/// `HolziError` and across the Tauri boundary. A misconfigured
/// `base_url` can answer with a multi-megabyte HTML page, and that
/// belongs in neither an error string nor the UI.
pub(super) fn truncate_body(mut body: String) -> String {
    const MAX_BODY: usize = 512;
    if body.len() <= MAX_BODY {
        return body;
    }
    let mut end = MAX_BODY;
    while end > 0 && !body.is_char_boundary(end) {
        end -= 1;
    }
    body.truncate(end);
    body.push_str("… (truncated)");
    body
}
