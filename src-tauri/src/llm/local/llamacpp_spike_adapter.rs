//! Spec 070 spike, second part: a prototype [`ProviderAdapter`] over llama.cpp, only to run the
//! 30-sentence tool evaluation of spec 032 against it (SC-002).
//!
//! It is what the binding does not give us: the model's own Jinja chat template is rendered with
//! `minijinja` (tools included), the answer is split into thinking and text, and tool calls are read
//! back out of the text, in both formats the Qwen family uses (JSON, and the XML of Qwen3.5).
//! Not production code: it generates the whole answer before it emits anything, ignores cancelling
//! and always samples greedily.

use std::num::NonZeroU32;
use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use llama_cpp_2::context::params::LlamaContextParams;
use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::llama_batch::LlamaBatch;
use llama_cpp_2::model::params::LlamaModelParams;
use llama_cpp_2::model::LlamaModel;
use llama_cpp_2::sampling::LlamaSampler;
use minijinja::value::Kwargs;
use minijinja::{context, Environment, Error, ErrorKind};
use serde_json::{json, Value};
use tokio::sync::mpsc;

use crate::adapters::types::{
    AdapterStream, ChatRequest, ChatRole, StreamChunk, StreamError, ToolCall, ToolSpec,
};
use crate::adapters::{AdapterError, ProviderAdapter, ProviderModel};

const N_CTX: u32 = 8192;
const N_BATCH: usize = 512;
const DEFAULT_MAX_NEW_TOKENS: usize = 1024;

pub struct LlamaCppSpikeAdapter {
    backend: Arc<LlamaBackend>,
    model: Arc<LlamaModel>,
    template: Arc<String>,
    bos: Arc<String>,
    eos: Arc<String>,
}

impl LlamaCppSpikeAdapter {
    /// Loads a GGUF; `gpu_layers` above the model's layer count puts everything on the GPU.
    pub fn load(path: &Path, gpu_layers: u32) -> Result<Self, String> {
        let backend = LlamaBackend::init().map_err(|e| e.to_string())?;
        let params = LlamaModelParams::default().with_n_gpu_layers(gpu_layers);
        let model =
            LlamaModel::load_from_file(&backend, path, &params).map_err(|e| e.to_string())?;
        let template = model
            .chat_template(None)
            .map_err(|e| format!("no chat template in the file: {e:?}"))?
            .to_string()
            .map_err(|e| e.to_string())?;
        let vocab = model.vocab();
        let piece =
            |token| String::from_utf8_lossy(&vocab.token_to_piece(token, true, None)).into_owned();
        let (bos, eos) = (piece(vocab.bos()), piece(vocab.eos()));
        Ok(Self {
            backend: Arc::new(backend),
            model: Arc::new(model),
            template: Arc::new(template),
            bos: Arc::new(bos),
            eos: Arc::new(eos),
        })
    }
}

fn tojson(value: minijinja::Value, kwargs: Kwargs) -> Result<String, Error> {
    let _ = kwargs.get::<Option<usize>>("indent");
    let _ = kwargs.get::<Option<bool>>("ensure_ascii");
    serde_json::to_string(&value).map_err(|e| {
        Error::new(ErrorKind::InvalidOperation, "cannot serialize to JSON").with_source(e)
    })
}

fn raise_exception(message: String) -> Result<String, Error> {
    Err(Error::new(ErrorKind::InvalidOperation, message))
}

/// The messages of a request in the shape the chat templates of the Qwen family read.
fn template_messages(req: &ChatRequest) -> Vec<Value> {
    let mut out = Vec::new();
    if let Some(system) = &req.system_prompt {
        out.push(json!({"role": "system", "content": system}));
    }
    let mut i = 0;
    while i < req.messages.len() {
        let message = &req.messages[i];
        match &message.role {
            ChatRole::User => out.push(json!({"role": "user", "content": message.content})),
            ChatRole::Assistant => {
                out.push(json!({"role": "assistant", "content": message.content}))
            }
            ChatRole::ToolCall { .. } => {
                let mut calls = Vec::new();
                while let Some(ChatRole::ToolCall { name, input, .. }) =
                    req.messages.get(i).map(|m| &m.role)
                {
                    calls.push(
                        json!({"type": "function", "function": {"name": name, "arguments": input}}),
                    );
                    i += 1;
                }
                out.push(json!({"role": "assistant", "content": "", "tool_calls": calls}));
                continue;
            }
            ChatRole::ToolResult {
                call_id, content, ..
            } => out.push(json!({"role": "tool", "content": content, "tool_call_id": call_id})),
        }
        i += 1;
    }
    out
}

fn template_tools(tools: &[ToolSpec]) -> Vec<Value> {
    tools
        .iter()
        .map(|t| {
            json!({"type": "function", "function": {
                "name": t.name, "description": t.description, "parameters": t.input_schema,
            }})
        })
        .collect()
}

/// Renders the template like mistralrs does for the same inputs (same context variables).
pub(super) fn render_prompt(
    template: &str,
    req: &ChatRequest,
    bos: &str,
    eos: &str,
) -> Result<String, Error> {
    let mut env = Environment::new();
    env.set_unknown_method_callback(minijinja_contrib::pycompat::unknown_method_callback);
    env.set_lstrip_blocks(true);
    env.set_trim_blocks(true);
    env.add_function("raise_exception", raise_exception);
    env.add_filter("tojson", tojson);
    env.add_function("strftime_now", |_format: String| "2026-10-30".to_owned());
    let template = template.replace("[::-1]", "|reverse");
    env.add_template("chat", &template)?;
    let tools = template_tools(&req.tools);
    let ctx = if tools.is_empty() {
        context! {
            messages => template_messages(req),
            add_generation_prompt => true,
            bos_token => bos,
            eos_token => eos,
            enable_thinking => req.reasoning_requested,
        }
    } else {
        context! {
            messages => template_messages(req),
            add_generation_prompt => true,
            bos_token => bos,
            eos_token => eos,
            tools => tools,
            enable_thinking => req.reasoning_requested,
        }
    };
    env.get_template("chat")?.render(ctx)
}

/// Splits `<think>…</think>` off the front: the template may have opened the block in the prompt.
pub(super) fn split_thinking(text: &str) -> (Option<String>, String) {
    match text.find("</think>") {
        Some(end) => {
            let reasoning = text[..end].trim_start_matches("<think>").trim().to_owned();
            let rest = text[end + "</think>".len()..].trim_start().to_owned();
            ((!reasoning.is_empty()).then_some(reasoning), rest)
        }
        None => (None, text.to_owned()),
    }
}

/// The JSON type a tool's schema gives one parameter.
fn param_type<'a>(tools: &'a [ToolSpec], tool: &str, param: &str) -> Option<&'a str> {
    tools
        .iter()
        .find(|t| t.name == tool)?
        .input_schema
        .get("properties")?
        .get(param)?
        .get("type")?
        .as_str()
}

fn parse_xml_call(inner: &str, tools: &[ToolSpec]) -> Option<(String, Value)> {
    let start = inner.find("<function=")?;
    let rest = &inner[start + "<function=".len()..];
    let name_end = rest.find('>')?;
    let name = rest[..name_end].trim().to_owned();
    let mut args = serde_json::Map::new();
    let mut body = &rest[name_end + 1..];
    while let Some(p) = body.find("<parameter=") {
        let after = &body[p + "<parameter=".len()..];
        let key_end = after.find('>')?;
        let key = after[..key_end].trim().to_owned();
        let value_start = &after[key_end + 1..];
        let value_end = value_start
            .find("</parameter>")
            .unwrap_or(value_start.len());
        let raw = value_start[..value_end]
            .strip_prefix('\n')
            .unwrap_or(&value_start[..value_end]);
        let raw = raw.strip_suffix('\n').unwrap_or(raw);
        let value = match param_type(tools, &name, &key) {
            Some("string") => Value::String(raw.to_owned()),
            _ => serde_json::from_str(raw).unwrap_or_else(|_| Value::String(raw.to_owned())),
        };
        args.insert(key, value);
        body = &value_start[(value_end + "</parameter>".len()).min(value_start.len())..];
    }
    Some((name, Value::Object(args)))
}

fn parse_json_call(inner: &str) -> Option<(String, Value)> {
    let value: Value = serde_json::from_str(inner).ok()?;
    let name = value.get("name")?.as_str()?.to_owned();
    let arguments = match value.get("arguments") {
        Some(Value::String(s)) => serde_json::from_str(s).ok()?,
        Some(other) => other.clone(),
        None => json!({}),
    };
    if !arguments.is_object() {
        return None;
    }
    Some((name, arguments))
}

/// Reads the `<tool_call>` blocks out of the text; returns the text without them and the calls.
pub(super) fn parse_tool_calls(text: &str, tools: &[ToolSpec]) -> (String, Vec<ToolCall>) {
    const OPEN: &str = "<tool_call>";
    const CLOSE: &str = "</tool_call>";
    let mut content = String::new();
    let mut calls = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find(OPEN) {
        content.push_str(&rest[..start]);
        let after = &rest[start + OPEN.len()..];
        let (inner, next) = match after.find(CLOSE) {
            Some(end) => (&after[..end], &after[end + CLOSE.len()..]),
            None => (after, ""),
        };
        let inner = inner.trim();
        let parsed = if inner.starts_with('{') {
            parse_json_call(inner)
        } else {
            parse_xml_call(inner, tools)
        };
        match parsed {
            Some((name, input)) => calls.push(ToolCall {
                id: format!("call_{}", calls.len()),
                name,
                input,
            }),
            None => content.push_str(&format!("{OPEN}{inner}{CLOSE}")),
        }
        rest = next;
    }
    content.push_str(rest);
    (content.trim().to_owned(), calls)
}

struct Generated {
    text: String,
    prompt_tokens: usize,
    new_tokens: usize,
    hit_limit: bool,
}

fn generate(
    backend: &LlamaBackend,
    model: &LlamaModel,
    prompt: &str,
    max_new_tokens: usize,
) -> Result<Generated, String> {
    let vocab = model.vocab();
    let tokens = vocab.tokenize(prompt.as_bytes(), false, true);
    if tokens.len() + max_new_tokens > N_CTX as usize {
        return Err(format!(
            "prompt of {} tokens does not fit the context",
            tokens.len()
        ));
    }
    let params = LlamaContextParams::default()
        .with_n_ctx(NonZeroU32::new(N_CTX))
        .with_n_batch(N_BATCH as u32);
    let mut ctx = model
        .new_context(backend, params)
        .map_err(|e| e.to_string())?;
    let mut batch = LlamaBatch::new(N_BATCH, 1);
    let mut pos = 0i32;
    for chunk in tokens.chunks(N_BATCH) {
        batch.clear();
        for (i, token) in chunk.iter().enumerate() {
            let last = pos as usize + 1 == tokens.len();
            batch
                .add(*token, pos, &[0], last && i == chunk.len() - 1)
                .map_err(|e| e.to_string())?;
            pos += 1;
        }
        ctx.decode(&mut batch).map_err(|e| e.to_string())?;
    }

    let mut sampler = LlamaSampler::greedy();
    let mut bytes = Vec::new();
    let mut new_tokens = 0;
    let mut hit_limit = false;
    loop {
        if new_tokens >= max_new_tokens {
            hit_limit = true;
            break;
        }
        let token = sampler.sample(&ctx, batch.n_tokens() - 1);
        if vocab.is_eog(token) {
            break;
        }
        bytes.extend(vocab.token_to_piece(token, true, None));
        new_tokens += 1;
        batch.clear();
        batch
            .add(token, pos, &[0], true)
            .map_err(|e| e.to_string())?;
        ctx.decode(&mut batch).map_err(|e| e.to_string())?;
        pos += 1;
    }
    Ok(Generated {
        text: String::from_utf8_lossy(&bytes).into_owned(),
        prompt_tokens: tokens.len(),
        new_tokens,
        hit_limit,
    })
}

#[async_trait]
impl ProviderAdapter for LlamaCppSpikeAdapter {
    async fn list_models(&self) -> Result<Vec<ProviderModel>, AdapterError> {
        Ok(Vec::new())
    }

    async fn stream_chat(&self, req: ChatRequest) -> Result<AdapterStream, AdapterError> {
        let (tx, rx) = mpsc::unbounded_channel();
        let (backend, model) = (Arc::clone(&self.backend), Arc::clone(&self.model));
        let (template, bos, eos) = (
            Arc::clone(&self.template),
            Arc::clone(&self.bos),
            Arc::clone(&self.eos),
        );
        let task = tokio::spawn(async move {
            let started = Instant::now();
            let result = tokio::task::spawn_blocking(move || {
                let prompt = render_prompt(&template, &req, &bos, &eos)
                    .map_err(|e| StreamError::StartFailed(format!("chat template: {e:#}")))?;
                let generated = generate(
                    &backend,
                    &model,
                    &prompt,
                    req.max_new_tokens.unwrap_or(DEFAULT_MAX_NEW_TOKENS),
                )
                .map_err(StreamError::Model)?;
                Ok::<_, StreamError>((req, generated))
            })
            .await;
            let (req, generated) = match result {
                Ok(Ok(done)) => done,
                Ok(Err(e)) => {
                    let _ = tx.send(Err(e));
                    return;
                }
                Err(e) => {
                    let _ = tx.send(Err(StreamError::Internal(e.to_string())));
                    return;
                }
            };
            let (reasoning, text) = split_thinking(&generated.text);
            let (content, calls) = parse_tool_calls(&text, &req.tools);
            let _ = tx.send(Ok(StreamChunk::Delta { content, reasoning }));
            let tool_calls = !calls.is_empty();
            if tool_calls {
                let _ = tx.send(Ok(StreamChunk::ToolCalls(calls)));
            }
            let finish = if tool_calls {
                "tool_calls"
            } else if generated.hit_limit {
                "length"
            } else {
                "stop"
            };
            let _ = tx.send(Ok(StreamChunk::Done {
                finish_reason: Some(finish.to_owned()),
                prompt_tokens: Some(generated.prompt_tokens),
                completion_tokens: Some(generated.new_tokens),
                ttft_ms: None,
                total_ms: started.elapsed().as_millis() as u64,
            }));
        });
        Ok(AdapterStream::new(rx, task.abort_handle()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tool(name: &str, schema: Value) -> ToolSpec {
        ToolSpec {
            name: name.to_owned(),
            description: String::new(),
            input_schema: schema,
        }
    }

    #[test]
    fn reads_a_json_call() {
        let text = "Sure.\n<tool_call>\n{\"name\": \"open\", \"arguments\": {\"page\": \"sync\"}}\n</tool_call>";
        let (content, calls) = parse_tool_calls(text, &[]);
        assert_eq!(content, "Sure.");
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "open");
        assert_eq!(calls[0].input, json!({"page": "sync"}));
    }

    #[test]
    fn reads_an_xml_call_and_types_its_parameters_by_schema() {
        let tools = [tool(
            "set",
            json!({"properties": {"label": {"type": "string"}, "count": {"type": "integer"}, "on": {"type": "boolean"}}}),
        )];
        let text = "<tool_call>\n<function=set>\n<parameter=label>\n42\n</parameter>\n<parameter=count>\n3\n</parameter>\n<parameter=on>\ntrue\n</parameter>\n</function>\n</tool_call>";
        let (content, calls) = parse_tool_calls(text, &tools);
        assert_eq!(content, "");
        assert_eq!(calls[0].name, "set");
        assert_eq!(
            calls[0].input,
            json!({"label": "42", "count": 3, "on": true})
        );
    }

    #[test]
    fn reads_two_calls_and_keeps_unparseable_text() {
        let text = "<tool_call>{\"name\":\"a\",\"arguments\":{}}</tool_call><tool_call>{broken</tool_call>";
        let (content, calls) = parse_tool_calls(text, &[]);
        assert_eq!(calls.len(), 1);
        assert!(content.contains("{broken"));
    }

    #[test]
    fn keeps_a_json_call_with_invalid_arguments_as_text() {
        let text = "<tool_call>{\"name\":\"open\",\"arguments\":\"{broken\"}</tool_call>";
        let (content, calls) = parse_tool_calls(text, &[]);
        assert!(calls.is_empty());
        assert!(content.contains(text));
    }

    #[test]
    fn splits_thinking_that_the_prompt_opened() {
        let (reasoning, text) = split_thinking("Let me see.\n</think>\n\nAnswer.");
        assert_eq!(reasoning.as_deref(), Some("Let me see."));
        assert_eq!(text, "Answer.");
        assert_eq!(split_thinking("<think>\n\n</think>\n\nHi").0, None);
        assert_eq!(split_thinking("plain").1, "plain");
    }
}
