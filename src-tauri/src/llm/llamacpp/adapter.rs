//! `LlamaCppAdapter`: the llama.cpp engine behind the chat's `ProviderAdapter` (spec 070, plan D4 to D7).

use std::cell::Cell;
use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use super::formats::{default_messages, template_tools, ChatFormat};
use super::generate::{generate, Ended, GenerateError, Job};
use super::model::{LoadError, LoadOptions, LoadedModel};
use super::streaming::Streamer;
use super::template::{render, RenderInput};
use crate::adapters::types::{
    AdapterStream, ChatMessage, ChatRequest, ChatRole, Sampling, StreamChunk, StreamError,
    ToolSpec, ToolTemplateProbe,
};
use crate::adapters::{AdapterError, ProviderAdapter, ProviderModel};
use crate::llm::local::probe::{decide, dummy_tool, PROBE_MESSAGE};

pub struct LlamaCppAdapter {
    model: Arc<LoadedModel>,
}

impl LlamaCppAdapter {
    /// Loads a GGUF. Takes seconds to a minute; the work runs on a blocking thread.
    pub async fn load(path: &Path, options: LoadOptions) -> Result<Self, LoadError> {
        let path = path.to_path_buf();
        let model = tokio::task::spawn_blocking(move || LoadedModel::load(&path, options))
            .await
            .map_err(|e| LoadError::Load(e.to_string()))??;
        Ok(Self {
            model: Arc::new(model),
        })
    }

    pub fn model(&self) -> &LoadedModel {
        &self.model
    }
}

/// What a prompt is rendered from: the parts of a loaded model, apart from the model so a test can feed
/// any template.
pub(super) struct PromptParts<'a> {
    pub template: &'a str,
    pub bos: &'a str,
    pub eos: &'a str,
    pub format: Option<&'static dyn ChatFormat>,
}

impl LoadedModel {
    fn prompt_parts(&self) -> PromptParts<'_> {
        PromptParts {
            template: &self.template,
            bos: &self.bos,
            eos: &self.eos,
            format: self.format,
        }
    }
}

/// Renders the request through the model's own chat template. A model whose template has no known
/// tool-call format is refused tools instead of being shown a syntax nothing reads.
pub(super) fn prompt_for(
    parts: &PromptParts<'_>,
    req: &ChatRequest,
) -> Result<String, StreamError> {
    if !req.tools.is_empty() && parts.format.is_none() {
        return Err(StreamError::StartFailed(
            "this model's chat template has no tool-call format the engine knows".to_owned(),
        ));
    }
    let messages = match parts.format {
        Some(format) => format.shape_messages(req.system_prompt.as_deref(), &req.messages),
        None => default_messages(req.system_prompt.as_deref(), &req.messages),
    };
    render(
        parts.template,
        &RenderInput {
            messages,
            tools: template_tools(&req.tools),
            enable_thinking: req.reasoning_requested,
            bos_token: parts.bos,
            eos_token: parts.eos,
        },
    )
    .map_err(|e| StreamError::StartFailed(e.to_string()))
}

/// How a failed generation reaches the chat. None of them is transient: a retry would run the same
/// prompt into the same wall.
pub(super) fn stream_error(error: GenerateError) -> StreamError {
    match error {
        GenerateError::PromptTooLong { .. } => StreamError::Validation(error.to_string()),
        GenerateError::Ggml(_) => StreamError::Model(error.to_string()),
        GenerateError::Backend(_) => StreamError::StartFailed(error.to_string()),
    }
}

/// The finish reason of the `Done` frame, as the mistralrs path reports it.
pub(super) fn finish_reason(ended: Ended, tool_calls: bool) -> &'static str {
    match (tool_calls, ended) {
        (true, _) => "tool_calls",
        (false, Ended::Length) => "length",
        (false, _) => "stop",
    }
}

/// Runs one request on the calling (blocking) thread and pushes its chunks. A cancel ends it quietly:
/// the listener has gone, there is nobody to tell.
fn run(
    model: &LoadedModel,
    req: ChatRequest,
    tx: &mpsc::UnboundedSender<Result<StreamChunk, StreamError>>,
    cancel: &CancellationToken,
    started: Instant,
) -> Result<(), StreamError> {
    let prompt = prompt_for(&model.prompt_parts(), &req)?;
    let mut streamer = Streamer::new(
        model.format,
        req.tools.clone(),
        Streamer::prompt_opens_thinking(&prompt, model.format),
    );
    let ttft_ms = Cell::new(None);
    let send = |chunk: StreamChunk| {
        if ttft_ms.get().is_none() {
            if let StreamChunk::Delta { content, .. } = &chunk {
                if !content.is_empty() {
                    ttft_ms.set(Some(started.elapsed().as_millis() as u64));
                }
            }
        }
        tx.send(Ok(chunk)).is_ok()
    };

    let job = Job {
        prompt: &prompt,
        max_new_tokens: req.max_new_tokens.unwrap_or(usize::MAX),
        deterministic: req.sampling == Sampling::Deterministic,
    };
    let outcome = generate(model, &job, cancel, |text| {
        streamer.push(text).into_iter().all(&send)
    })
    .map_err(stream_error)?;
    if outcome.ended == Ended::Cancelled {
        return Ok(());
    }

    let finished = streamer.finish();
    for chunk in finished.chunks {
        if !send(chunk) {
            return Ok(());
        }
    }
    let _ = send(StreamChunk::Done {
        finish_reason: Some(finish_reason(outcome.ended, finished.tool_calls).to_owned()),
        prompt_tokens: Some(outcome.prompt_tokens),
        completion_tokens: Some(outcome.new_tokens),
        ttft_ms: ttft_ms.get(),
        total_ms: started.elapsed().as_millis() as u64,
    });
    Ok(())
}

#[async_trait]
impl ProviderAdapter for LlamaCppAdapter {
    async fn list_models(&self) -> Result<Vec<ProviderModel>, AdapterError> {
        Ok(Vec::new())
    }

    async fn stream_chat(&self, req: ChatRequest) -> Result<AdapterStream, AdapterError> {
        let (tx, rx) = mpsc::unbounded_channel();
        let cancel = CancellationToken::new();
        let (model, token) = (Arc::clone(&self.model), cancel.clone());
        let task = tokio::spawn(async move {
            let started = Instant::now();
            // One generation at a time; a second request waits here, and gives up when it is cancelled.
            let guard = tokio::select! {
                biased;
                () = token.cancelled() => return,
                guard = Arc::clone(&model.gate).lock_owned() => guard,
            };
            let result = {
                let (tx, token) = (tx.clone(), token.clone());
                tokio::task::spawn_blocking(move || {
                    let _guard = guard;
                    run(&model, req, &tx, &token, started)
                })
                .await
            };
            match result {
                Ok(Ok(())) => {}
                Ok(Err(error)) => {
                    let _ = tx.send(Err(error));
                }
                Err(join) => {
                    let _ = tx.send(Err(StreamError::Internal(join.to_string())));
                }
            }
        });
        Ok(AdapterStream::new_with_cancellation(
            rx,
            task.abort_handle(),
            cancel,
        ))
    }

    async fn probe_tool_template(&self) -> ToolTemplateProbe {
        let model = Arc::clone(&self.model);
        tokio::task::spawn_blocking(move || probe(&model.prompt_parts()))
            .await
            .unwrap_or(ToolTemplateProbe::Inconclusive)
    }
}

/// Renders the fixed message with and without the dummy tool through the real template and compares.
pub(super) fn probe(parts: &PromptParts<'_>) -> ToolTemplateProbe {
    let history = [ChatMessage {
        role: ChatRole::User,
        content: PROBE_MESSAGE.to_owned(),
        attachments: Vec::new(),
    }];
    // `prompt_for` refuses tools for a format it does not know; the probe asks the template only.
    let render_with = |tools: Vec<ToolSpec>| {
        render(
            parts.template,
            &RenderInput {
                messages: default_messages(None, &history),
                tools: template_tools(&tools),
                enable_thinking: false,
                bos_token: parts.bos,
                eos_token: parts.eos,
            },
        )
        .map_err(|e| e.to_string())
    };
    let without = render_with(Vec::new());
    let with = render_with(vec![dummy_tool()]);
    decide(
        without.as_deref().map_err(String::as_str),
        with.as_deref().map_err(String::as_str),
    )
}

#[cfg(test)]
#[path = "adapter_tests.rs"]
mod tests;
