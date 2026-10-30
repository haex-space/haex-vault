//! Spec 070 spike, second part: a prototype [`ProviderAdapter`] over llama.cpp, only to run the
//! tool evaluation of spec 032 against it (SC-002) and to try models the production adapter does not yet
//! cover (Gemma 4, whose call syntax has no format yet).
//!
//! The chat template and the tool-call formats are the production modules in `llm/llamacpp`; what is still
//! prototype is the rest: it generates the whole answer before it emits anything, ignores cancelling and
//! always samples greedily. PR C of spec 070 replaces it with `LlamaCppAdapter`.

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
use tokio::sync::mpsc;

use crate::adapters::types::{AdapterStream, ChatRequest, StreamChunk, StreamError};
use crate::adapters::{AdapterError, ProviderAdapter, ProviderModel};
use crate::llm::llamacpp::formats::{
    default_messages, detect, split_thinking, template_tools, ChatFormat, Parsed,
};
use crate::llm::llamacpp::template::{render, RenderInput};

const N_CTX: u32 = 8192;
const N_BATCH: usize = 512;
const DEFAULT_MAX_NEW_TOKENS: usize = 1024;

pub struct LlamaCppSpikeAdapter {
    backend: Arc<LlamaBackend>,
    model: Arc<LlamaModel>,
    template: Arc<String>,
    format: Option<&'static dyn ChatFormat>,
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
        let format = detect(&template);
        Ok(Self {
            backend: Arc::new(backend),
            model: Arc::new(model),
            template: Arc::new(template),
            format,
            bos: Arc::new(bos),
            eos: Arc::new(eos),
        })
    }
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
        let format = self.format;
        let task = tokio::spawn(async move {
            let started = Instant::now();
            let result = tokio::task::spawn_blocking(move || {
                if !req.tools.is_empty() && format.is_none() {
                    return Err(StreamError::StartFailed(
                        "this model's chat template has no tool-call format the engine knows"
                            .to_owned(),
                    ));
                }
                let messages = match format {
                    Some(f) => f.shape_messages(req.system_prompt.as_deref(), &req.messages),
                    None => default_messages(req.system_prompt.as_deref(), &req.messages),
                };
                let prompt = render(
                    &template,
                    &RenderInput {
                        messages,
                        tools: template_tools(&req.tools),
                        enable_thinking: req.reasoning_requested,
                        bos_token: &bos,
                        eos_token: &eos,
                    },
                )
                .map_err(|e| StreamError::StartFailed(e.to_string()))?;
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
            let Parsed {
                reasoning,
                content,
                calls,
            } = match format {
                Some(f) => f.parse(&generated.text, &req.tools),
                None => {
                    let (reasoning, content) = split_thinking(&generated.text);
                    Parsed {
                        reasoning,
                        content,
                        calls: Vec::new(),
                    }
                }
            };
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
