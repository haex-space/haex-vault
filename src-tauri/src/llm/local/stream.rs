//! Cancellable, token-by-token generation on top of
//! [`mistralrs::Model::stream_chat_request`].
//!
//! Etappe 0 finding #5: TTFT is only observable via the streaming API;
//! the non-streaming `send_chat_request` blocks until completion. This
//! module always uses streaming.
//!
//! The public request / chunk / error types live in
//! [`crate::adapters::types`] so both the local and remote paths speak
//! the same shape; `LocalModel::stream_chat` returns an
//! [`AdapterStream`] directly.

use std::time::Instant;

use mistralrs::{
    CalledFunction, Function, RequestBuilder, Response, TextMessageRole, Tool, ToolCallResponse,
    ToolCallType, ToolType,
};
use tokio::sync::mpsc;

use super::LocalModel;
use crate::adapters::types::{
    AdapterStream, ChatRequest, ChatRole, Sampling, StreamChunk, StreamError, ToolCall, ToolSpec,
};
use crate::llm::holdback::longest_prefix_overlap;

/// JSON-Schema `input_schema` → mistralrs' `Function.parameters` shape
/// (research.md §2). `None` when the schema is not a JSON object (should
/// not happen for a well-formed `ToolSpec`, but mistralrs' type is an
/// `Option` so there is no lossy fallback needed).
pub(super) fn to_mistralrs_tool(spec: &ToolSpec) -> Tool {
    let parameters = spec
        .input_schema
        .as_object()
        .map(|obj| obj.clone().into_iter().collect());
    Tool {
        tp: ToolType::Function,
        function: Function {
            description: Some(spec.description.clone()),
            name: spec.name.clone(),
            parameters,
        },
    }
}

/// `ToolCallResponse.function.arguments` is a JSON string (research.md
/// §2); a malformed one (should not happen) degrades to an empty object
/// rather than dropping the call.
fn from_mistralrs_tool_call(t: &ToolCallResponse) -> ToolCall {
    let input = serde_json::from_str(&t.function.arguments).unwrap_or(serde_json::json!({}));
    ToolCall {
        id: t.id.clone(),
        name: t.function.name.clone(),
        input,
    }
}

/// mistralrs 0.8.1's reasoning-mode content path never runs generated text
/// through its own tool-call-tag stripping (unlike its non-reasoning path),
/// so on a reasoning-capable Qwen-family model the literal `<tool_call>…
/// </tool_call>` block the model emits to signal a tool call leaks straight
/// into `delta.content`, right alongside the correctly-parsed
/// `delta.tool_calls`. Upstream bug:
/// <https://github.com/EricLBuehler/mistral.rs/issues/2427>. See
/// `chat::events::strip_leaked_tool_call_markup` for the persisted-
/// message-side half of this workaround; this half stops the leak from
/// ever reaching a live streamed delta in the first place.
///
/// Only called while `req.reasoning_requested` — the non-reasoning path
/// never exhibits the leak, so there is nothing to buffer there.
const REASONING_LEAK_OPEN: &str = "<tool_call>";
const REASONING_LEAK_CLOSE: &str = "</tool_call>";

/// Feeds newly-arrived `content` into `buffer` and returns whatever part of
/// it is now safe to stream to the caller — i.e. text that cannot still
/// turn into (or extend) a leaked tool-call tag once more chunks arrive.
/// Anything held back stays in `buffer` for the next call.
fn drain_reasoning_leak_buffer(buffer: &mut String, content: &str) -> String {
    buffer.push_str(content);

    let mut emit = String::new();
    loop {
        let Some(open_pos) = buffer.find(REASONING_LEAK_OPEN) else {
            // No open tag anywhere in the buffer — but its tail might be
            // the start of one that completes in a later chunk (e.g. this
            // chunk ended with "<tool_c"). Hold that overlap back.
            let hold = longest_prefix_overlap(buffer, REASONING_LEAK_OPEN);
            let safe_len = buffer.len() - hold;
            emit.push_str(&buffer[..safe_len]);
            *buffer = buffer[safe_len..].to_string();
            return emit;
        };
        // Whatever precedes the tag is definitely real content.
        emit.push_str(&buffer[..open_pos]);
        match buffer[open_pos..].find(REASONING_LEAK_CLOSE) {
            Some(close_rel) => {
                // Complete tagged block: drop it entirely and keep
                // looking — the remainder may hold more real text, or
                // another tag.
                let close_end = open_pos + close_rel + REASONING_LEAK_CLOSE.len();
                *buffer = buffer[close_end..].to_string();
            }
            None => {
                // Tag still generating — hold everything from the open
                // tag onward and wait for more chunks.
                *buffer = buffer[open_pos..].to_string();
                return emit;
            }
        }
    }
}

/// Discards a confirmed but incomplete leaked tool-call block at stream end,
/// while returning an incomplete marker-like prefix as ordinary prose.
fn take_terminal_reasoning_leak_buffer(buffer: &mut String) -> Option<String> {
    if buffer.is_empty() || buffer.starts_with(REASONING_LEAK_OPEN) {
        buffer.clear();
        None
    } else {
        Some(std::mem::take(buffer))
    }
}

/// Builds the request, translating the flat per-row history (data-model.md)
/// into mistralrs' message shape: consecutive `ToolCall` rows become one
/// `add_message_with_tool_call`, and each `ToolResult` row becomes its own
/// `add_tool_message` — mirroring the Anthropic adapter's grouping
/// (research.md §1/§2 both being JSON-Schema/tool-call-shaped, just with
/// different wire encodings).
fn build_request(req: &ChatRequest) -> RequestBuilder {
    let mut builder = RequestBuilder::new();
    if req.sampling == Sampling::Deterministic {
        builder = builder.set_deterministic_sampler();
    }
    if let Some(sys) = req.system_prompt.as_ref() {
        builder = builder.add_message(TextMessageRole::System, sys);
    }

    let mut i = 0;
    while i < req.messages.len() {
        match &req.messages[i].role {
            ChatRole::User => {
                builder = builder.add_message(TextMessageRole::User, &req.messages[i].content);
                i += 1;
            }
            ChatRole::Assistant => {
                builder = builder.add_message(TextMessageRole::Assistant, &req.messages[i].content);
                i += 1;
            }
            ChatRole::ToolCall { .. } => {
                let mut calls = Vec::new();
                while let Some(ChatRole::ToolCall { id, name, input }) =
                    req.messages.get(i).map(|m| &m.role)
                {
                    calls.push(ToolCallResponse {
                        index: calls.len(),
                        id: id.clone(),
                        tp: ToolCallType::Function,
                        function: CalledFunction {
                            name: name.clone(),
                            arguments: serde_json::to_string(input)
                                .unwrap_or_else(|_| "{}".to_string()),
                        },
                    });
                    i += 1;
                }
                builder = builder.add_message_with_tool_call(TextMessageRole::Assistant, "", calls);
            }
            ChatRole::ToolResult { .. } => {
                let ChatRole::ToolResult {
                    call_id, content, ..
                } = &req.messages[i].role
                else {
                    unreachable!()
                };
                builder = builder.add_tool_message(content, call_id);
                i += 1;
            }
        }
    }

    if let Some(cap) = req.max_new_tokens {
        builder = builder.set_sampler_max_len(cap);
    }
    // mistralrs defaults an unspecified thinking flag to `true`. Set it
    // explicitly so Qwen3 tool requests really use the structured
    // tool-calling path selected by the chat command.
    builder = builder.enable_thinking(req.reasoning_requested);
    if !req.tools.is_empty() {
        builder = builder.set_tools(req.tools.iter().map(to_mistralrs_tool).collect());
    }
    builder
}

impl LocalModel {
    /// Starts a streaming chat generation. Returns immediately; the
    /// generation runs on a background task and pushes into the
    /// returned [`AdapterStream`].
    pub fn stream_chat(&self, req: ChatRequest) -> AdapterStream {
        let (tx, rx) = mpsc::unbounded_channel();
        let model = self.inner();
        let start = Instant::now();

        let task = tokio::spawn(async move {
            // `ChatRequest::model_id` is ignored here — for local runs
            // the model is bound at `LocalAdapter::new` time.
            let builder = build_request(&req);

            let mut stream = match model.stream_chat_request(builder).await {
                Ok(s) => s,
                Err(e) => {
                    let _ = tx.send(Err(StreamError::StartFailed(e.to_string())));
                    return;
                }
            };

            let mut ttft_ms: Option<u64> = None;
            let mut done_emitted = false;
            let mut delta_emitted = false;
            let mut last_finish_reason: Option<String> = None;
            let mut tool_call_fragments: Vec<ToolCallResponse> = Vec::new();
            // Only ever holds text back when `req.reasoning_requested` —
            // see `drain_reasoning_leak_buffer`.
            let mut leak_buffer = String::new();

            while let Some(response) = stream.next().await {
                match response {
                    Response::Chunk(chunk) => {
                        let mut content = String::new();
                        let mut reasoning: Option<String> = None;
                        for choice in &chunk.choices {
                            if let Some(c) = &choice.delta.content {
                                content.push_str(c);
                            }
                            if let Some(r) = &choice.delta.reasoning_content {
                                reasoning.get_or_insert_with(String::new).push_str(r);
                            }
                            if let Some(fr) = &choice.finish_reason {
                                last_finish_reason = Some(fr.clone());
                            }
                            if let Some(calls) = &choice.delta.tool_calls {
                                for call in calls {
                                    let existing =
                                        tool_call_fragments.iter_mut().find(|existing| {
                                            existing.index == call.index
                                                || (!call.id.is_empty() && existing.id == call.id)
                                        });
                                    if let Some(existing) = existing {
                                        if existing.id.is_empty() {
                                            existing.id = call.id.clone();
                                        }
                                        if existing.function.name.is_empty() {
                                            existing.function.name = call.function.name.clone();
                                        }
                                        existing
                                            .function
                                            .arguments
                                            .push_str(&call.function.arguments);
                                    } else {
                                        tool_call_fragments.push(call.clone());
                                    }
                                }
                            }
                        }
                        // A reasoning-enabled local model can leak a raw
                        // `<tool_call>` tag straight into `content` (see
                        // `drain_reasoning_leak_buffer`); everything else
                        // takes `content` as-is, same as before.
                        let content = if req.reasoning_requested {
                            drain_reasoning_leak_buffer(&mut leak_buffer, &content)
                        } else {
                            content
                        };
                        if ttft_ms.is_none() && !content.is_empty() {
                            ttft_ms = Some(start.elapsed().as_millis() as u64);
                        }
                        if !content.is_empty() {
                            delta_emitted = true;
                        }
                        if tx
                            .send(Ok(StreamChunk::Delta { content, reasoning }))
                            .is_err()
                        {
                            // Receiver dropped: caller cancelled.
                            return;
                        }
                    }
                    Response::Done(final_resp) => {
                        let finish_reason =
                            final_resp.choices.first().map(|c| c.finish_reason.clone());
                        // Deltas normally carry every tool call as they
                        // stream in; fall back to the final aggregated
                        // message in case a pipeline only populates it
                        // there.
                        if tool_call_fragments.is_empty() {
                            if let Some(calls) = final_resp
                                .choices
                                .first()
                                .and_then(|c| c.message.tool_calls.as_ref())
                            {
                                tool_call_fragments.extend(calls.iter().cloned());
                            }
                        }
                        // Whatever the leak buffer is still holding back
                        // never turned into (the rest of) a tag — flush it
                        // as ordinary content before anything else.
                        if let Some(content) = take_terminal_reasoning_leak_buffer(&mut leak_buffer)
                        {
                            let _ = tx.send(Ok(StreamChunk::Delta {
                                content,
                                reasoning: None,
                            }));
                        }
                        if !tool_call_fragments.is_empty() {
                            let calls = tool_call_fragments
                                .iter()
                                .map(from_mistralrs_tool_call)
                                .collect();
                            let _ = tx.send(Ok(StreamChunk::ToolCalls(calls)));
                        }
                        let done = StreamChunk::Done {
                            finish_reason,
                            prompt_tokens: Some(final_resp.usage.prompt_tokens),
                            completion_tokens: Some(final_resp.usage.completion_tokens),
                            ttft_ms,
                            total_ms: start.elapsed().as_millis() as u64,
                        };
                        let _ = tx.send(Ok(done));
                        done_emitted = true;
                        break;
                    }
                    Response::ModelError(msg, _partial) => {
                        let _ = tx.send(Err(StreamError::Model(msg)));
                        done_emitted = true;
                        break;
                    }
                    Response::ValidationError(e) => {
                        let _ = tx.send(Err(StreamError::Validation(e.to_string())));
                        done_emitted = true;
                        break;
                    }
                    Response::InternalError(e) => {
                        let _ = tx.send(Err(StreamError::Internal(e.to_string())));
                        done_emitted = true;
                        break;
                    }
                    // Non-chat variants (completion/image/speech/raw) cannot
                    // arrive here because we only submit chat requests, but
                    // enum exhaustiveness demands a branch. `Response` does
                    // not implement `Debug`, so we cannot format it.
                    _ => {
                        let _ = tx.send(Err(StreamError::Internal(
                            "unexpected non-chat response variant".to_string(),
                        )));
                        done_emitted = true;
                        break;
                    }
                }
            }

            if !done_emitted {
                // Mirrors the `Response::Done` handling above: a stream
                // that closes without a final frame (see below) must not
                // silently drop tool calls — or held-back leak-buffer
                // content — it already accumulated.
                if let Some(content) = take_terminal_reasoning_leak_buffer(&mut leak_buffer) {
                    delta_emitted = true;
                    let _ = tx.send(Ok(StreamChunk::Delta {
                        content,
                        reasoning: None,
                    }));
                }
                if !tool_call_fragments.is_empty() {
                    let calls = tool_call_fragments
                        .iter()
                        .map(from_mistralrs_tool_call)
                        .collect();
                    let _ = tx.send(Ok(StreamChunk::ToolCalls(calls)));
                }
                if delta_emitted || !tool_call_fragments.is_empty() {
                    // mistralrs 0.8.1 has been observed to close the stream
                    // on CUDA without emitting a final `Response::Done`
                    // when the model hits its max_new_tokens cap. Treat a
                    // clean stream close after we saw content or tool
                    // calls as a synthetic completion — token counts are
                    // unknown because they only reach us in the Done frame.
                    let synth = StreamChunk::Done {
                        finish_reason: last_finish_reason,
                        prompt_tokens: None,
                        completion_tokens: None,
                        ttft_ms,
                        total_ms: start.elapsed().as_millis() as u64,
                    };
                    let _ = tx.send(Ok(synth));
                } else {
                    let _ = tx.send(Err(StreamError::UnexpectedEnd));
                }
            }
        });

        let abort = task.abort_handle();
        // Only the abort handle is retained; `tokio::spawn` keeps the
        // task running regardless of the JoinHandle being dropped.
        AdapterStream::new(rx, abort)
    }
}

#[cfg(test)]
#[path = "stream_tests.rs"]
mod tests;
