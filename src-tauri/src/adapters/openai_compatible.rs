//! Adapter for servers that speak the OpenAI chat-completions format (spec 055, research R7).
//!
//! One adapter covers a GPU machine running Ollama or vLLM, a school server, and Google's
//! OpenAI-compatible endpoint for Gemini. `base_url` is the address including the version path
//! (`http://localhost:11434/v1`, `https://generativelanguage.googleapis.com/v1beta/openai`); the
//! adapter appends `/models` and `/chat/completions`. The key is optional: a server in the own
//! network often wants none.

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use eventsource_stream::Eventsource;
use futures::StreamExt;
use reqwest::{Client, RequestBuilder};
use serde::Deserialize;
use serde_json::Value;
use tokio::sync::mpsc;

use super::anthropic::truncate_body;
use super::openai_request::build_chat_body;
use super::types::{AdapterStream, ChatRequest, StreamChunk, StreamError, ToolCall};
use super::{AdapterError, ProviderAdapter, ProviderModel};
use crate::model_capabilities::ModelCapabilities;

/// Budget for the finite model-list request. Streaming requests are long-lived and are governed by
/// the caller's abort handle.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// Adapter for `api_key`-kind providers with the adapter id `openai_compatible`.
pub struct OpenAiCompatibleAdapter {
    client: Client,
    base_url: String,
    /// `None` when the server needs no key.
    api_key: Option<String>,
}

impl OpenAiCompatibleAdapter {
    /// Builds an adapter. An empty `api_key` means no `Authorization` header is sent. Trailing
    /// slashes on `base_url` are trimmed when a request is made.
    pub fn new(base_url: String, api_key: String) -> Result<Self, AdapterError> {
        let client = Client::builder()
            .connect_timeout(Duration::from_secs(15))
            .user_agent(concat!("holzi/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|e| AdapterError::Http {
                reason: format!("build reqwest client: {e}"),
            })?;
        let api_key = Some(api_key).filter(|key| !key.trim().is_empty());
        Ok(Self {
            client,
            base_url,
            api_key,
        })
    }

    fn endpoint(&self, tail: &str) -> String {
        format!("{}/{tail}", self.base_url.trim_end_matches('/'))
    }

    fn authorize(&self, request: RequestBuilder) -> RequestBuilder {
        match &self.api_key {
            Some(key) => request.bearer_auth(key),
            None => request,
        }
    }
}

#[derive(Deserialize)]
struct ModelList {
    data: Vec<ModelInfo>,
}

#[derive(Deserialize)]
struct ModelInfo {
    id: String,
    /// The servers that report a context size use one of these names (OpenRouter, vLLM, LM Studio).
    #[serde(default)]
    context_length: Option<i64>,
    #[serde(default)]
    max_model_len: Option<i64>,
    #[serde(default)]
    context_window: Option<i64>,
}

impl ModelInfo {
    fn into_model(self) -> ProviderModel {
        let context_window = [self.context_length, self.max_model_len, self.context_window]
            .into_iter()
            .flatten()
            .find(|n| *n > 0);
        // Gemini lists `models/<name>`; the id stays as listed, the picker shows the short name.
        let display_name = self
            .id
            .strip_prefix("models/")
            .unwrap_or(&self.id)
            .to_string();
        ProviderModel {
            remote_id: self.id,
            display_name,
            context_window,
            // `/models` hardly ever says what a model can do: nothing is determined. The user can
            // state it per model (spec 055 FR-019).
            capabilities: ModelCapabilities::default(),
        }
    }
}

/// Maps a non-success status of a request before the first byte.
fn status_error(status: u16, body: String) -> AdapterError {
    match status {
        401 | 403 => AdapterError::InvalidCredentials,
        other => AdapterError::Status {
            status: other,
            body: truncate_body(body),
        },
    }
}

#[async_trait]
impl ProviderAdapter for OpenAiCompatibleAdapter {
    async fn list_models(&self) -> Result<Vec<ProviderModel>, AdapterError> {
        let endpoint = self.endpoint("models");
        let resp = self
            .authorize(self.client.get(&endpoint).timeout(REQUEST_TIMEOUT))
            .send()
            .await
            .map_err(|e| super::transport_error(format!("GET {endpoint}"), e))?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(status_error(status.as_u16(), body));
        }
        let list: ModelList = resp.json().await.map_err(|e| AdapterError::Parse {
            reason: format!("decode /models: {e}"),
        })?;
        Ok(list.data.into_iter().map(ModelInfo::into_model).collect())
    }

    async fn stream_chat(&self, req: ChatRequest) -> Result<AdapterStream, AdapterError> {
        let endpoint = self.endpoint("chat/completions");
        let body = build_chat_body(&req);
        let resp = self
            .authorize(
                self.client
                    .post(&endpoint)
                    .header("accept", "text/event-stream")
                    .json(&body),
            )
            .send()
            .await
            .map_err(|e| super::transport_error(format!("POST {endpoint}"), e))?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(status_error(status.as_u16(), body));
        }

        let (tx, rx) = mpsc::unbounded_channel();
        let byte_stream = resp.bytes_stream();
        let reasoning_requested = req.reasoning_requested;
        let task = tokio::spawn(async move {
            let mut events = byte_stream.eventsource();
            let mut run = Run::new(reasoning_requested);
            while let Some(event) = events.next().await {
                let event = match event {
                    Ok(event) => event,
                    Err(e) => {
                        let _ = tx.send(Err(StreamError::Internal(format!("sse decode: {e}"))));
                        return;
                    }
                };
                let data = event.data.trim();
                if data == "[DONE]" {
                    let chunks = match run.finish() {
                        Ok(chunks) => chunks,
                        Err(error) => {
                            let _ = tx.send(Err(error));
                            return;
                        }
                    };
                    for chunk in chunks {
                        let _ = tx.send(Ok(chunk));
                    }
                    return;
                }
                let Ok(payload) = serde_json::from_str::<Value>(data) else {
                    continue;
                };
                if let Some(error) = error_of(&payload) {
                    let _ = tx.send(Err(error));
                    return;
                }
                for chunk in run.absorb(&payload) {
                    if tx.send(Ok(chunk)).is_err() {
                        return;
                    }
                }
            }
            // Some servers close the stream after the last chunk without `[DONE]`.
            if run.finish_reason.is_some() {
                let chunks = match run.finish() {
                    Ok(chunks) => chunks,
                    Err(error) => {
                        let _ = tx.send(Err(error));
                        return;
                    }
                };
                for chunk in chunks {
                    let _ = tx.send(Ok(chunk));
                }
            } else {
                let _ = tx.send(Err(StreamError::UnexpectedEnd));
            }
        });
        Ok(AdapterStream::new(rx, task.abort_handle()))
    }
}

/// An error object inside the stream (`data: {"error": {...}}`). A rate limit or an overload is
/// transient, everything else reproduces for the same request.
fn error_of(payload: &Value) -> Option<StreamError> {
    let error = payload.get("error")?;
    let message = error
        .get("message")
        .and_then(Value::as_str)
        .unwrap_or("provider error")
        .to_string();
    let kind = error
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let code = error.get("code");
    let transient = ["rate_limit", "overloaded", "server_error"]
        .iter()
        .any(|word| kind.contains(word))
        || code.and_then(Value::as_u64) == Some(429)
        || code.and_then(Value::as_str) == Some("rate_limit_exceeded");
    Some(if transient {
        StreamError::Transient(message)
    } else {
        StreamError::Model(message)
    })
}

/// What one streamed answer has collected so far.
struct Run {
    start: Instant,
    reasoning_requested: bool,
    ttft_ms: Option<u64>,
    finish_reason: Option<String>,
    prompt_tokens: Option<usize>,
    completion_tokens: Option<usize>,
    /// One call per index: the id, the name and the concatenated argument fragments.
    tool_calls: BTreeMap<u64, (String, String, String)>,
}

impl Run {
    fn new(reasoning_requested: bool) -> Self {
        Self {
            start: Instant::now(),
            reasoning_requested,
            ttft_ms: None,
            finish_reason: None,
            prompt_tokens: None,
            completion_tokens: None,
            tool_calls: BTreeMap::new(),
        }
    }

    /// Takes one decoded chunk and returns what to hand on to the reader.
    fn absorb(&mut self, payload: &Value) -> Vec<StreamChunk> {
        let mut out = Vec::new();
        if let Some(usage) = payload.get("usage").filter(|u| u.is_object()) {
            self.prompt_tokens = usage
                .get("prompt_tokens")
                .and_then(Value::as_u64)
                .map(|n| n as usize);
            self.completion_tokens = usage
                .get("completion_tokens")
                .and_then(Value::as_u64)
                .map(|n| n as usize);
        }
        let Some(choice) = payload.pointer("/choices/0") else {
            return out;
        };
        if let Some(reason) = choice.get("finish_reason").and_then(Value::as_str) {
            self.finish_reason = Some(reason.to_string());
        }
        let Some(delta) = choice.get("delta") else {
            return out;
        };
        let content = delta
            .get("content")
            .and_then(Value::as_str)
            .unwrap_or_default();
        // `reasoning_content` (vLLM, DeepSeek) and `reasoning` (Ollama) name the same thing.
        let reasoning = ["reasoning_content", "reasoning"]
            .iter()
            .find_map(|key| delta.get(*key).and_then(Value::as_str))
            .filter(|text| !text.is_empty() && self.reasoning_requested);
        if !content.is_empty() && self.ttft_ms.is_none() {
            self.ttft_ms = Some(self.start.elapsed().as_millis() as u64);
        }
        if !content.is_empty() || reasoning.is_some() {
            out.push(StreamChunk::Delta {
                content: content.to_string(),
                reasoning: reasoning.map(str::to_string),
            });
        }
        if let Some(fragments) = delta.get("tool_calls").and_then(Value::as_array) {
            for fragment in fragments {
                let index = fragment.get("index").and_then(Value::as_u64).unwrap_or(0);
                let entry = self.tool_calls.entry(index).or_default();
                if let Some(id) = fragment.get("id").and_then(Value::as_str) {
                    entry.0 = id.to_string();
                }
                if let Some(name) = fragment.pointer("/function/name").and_then(Value::as_str) {
                    entry.1.push_str(name);
                }
                if let Some(args) = fragment
                    .pointer("/function/arguments")
                    .and_then(Value::as_str)
                {
                    entry.2.push_str(args);
                }
            }
        }
        out
    }

    /// The closing chunks. A tool call is complete only now, when all its fragments are in; the
    /// calls go out as one chunk right before `Done`, like with Anthropic.
    fn finish(&mut self) -> Result<Vec<StreamChunk>, StreamError> {
        let mut out = Vec::new();
        let mut calls = Vec::with_capacity(self.tool_calls.len());
        for (id, name, arguments) in std::mem::take(&mut self.tool_calls).into_values() {
            let input = if arguments.trim().is_empty() {
                serde_json::json!({})
            } else {
                serde_json::from_str(&arguments).map_err(|error| {
                    StreamError::Model(format!("invalid tool arguments for {name}: {error}"))
                })?
            };
            calls.push(ToolCall { id, name, input });
        }
        if !calls.is_empty() {
            out.push(StreamChunk::ToolCalls(calls));
        }
        out.push(StreamChunk::Done {
            finish_reason: self.finish_reason.clone(),
            prompt_tokens: self.prompt_tokens,
            completion_tokens: self.completion_tokens,
            ttft_ms: self.ttft_ms,
            total_ms: self.start.elapsed().as_millis() as u64,
        });
        Ok(out)
    }
}
