//! The token loop (spec 070, plan D5): one context per generation, prompt in batches, then one token at a
//! time until the model stops, the limit is reached, the caller cancels or the receiver is gone.

use std::num::NonZeroU32;

use llama_cpp_2::context::params::LlamaContextParams;
use llama_cpp_2::llama_batch::LlamaBatch;
use llama_cpp_2::sampling::LlamaSampler;
use llama_cpp_2::token::LlamaToken;
use llama_cpp_2::vocab::LlamaVocab;
use tokio_util::sync::CancellationToken;

use super::model::{backend, LoadError, LoadedModel};

/// Prompt tokens decoded per call. The cancel flag is looked at between batches, so this bounds how long
/// a cancel during a long prompt can wait.
pub(super) const BATCH: usize = 512;
/// Whether a layer that looks at a window keeps the whole context anyway. It does not: the window is what
/// the memory plan counts. (The drafter's rejected tail can be removed from such a cache as it is:
/// research R6, the `mtp_spike` test of 2026-11-04.) Both generation paths use it, so it lives in a module
/// the mobile targets build too.
pub(super) const SWA_FULL: bool = false;
/// The sampler of a normal chat (the evaluation uses greedy decoding).
const TOP_K: i32 = 20;
const TOP_P: f32 = 0.95;
const TEMPERATURE: f32 = 0.7;

/// What to generate.
pub struct Job<'a> {
    pub prompt: &'a str,
    pub max_new_tokens: usize,
    /// Greedy decoding, so two runs give the same answer.
    pub deterministic: bool,
}

/// Why a generation ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ended {
    /// The model produced an end-of-generation token.
    Stop,
    /// `max_new_tokens`, or the end of the context, was reached.
    Length,
    /// The caller cancelled, or nobody is listening any more.
    Cancelled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Outcome {
    pub prompt_tokens: usize,
    pub new_tokens: usize,
    pub ended: Ended,
}

#[derive(Debug, thiserror::Error)]
pub enum GenerateError {
    #[error("the prompt has {tokens} tokens and does not fit the context of {context} tokens")]
    PromptTooLong { tokens: usize, context: u32 },
    #[error("llama.cpp failed: {0}")]
    Ggml(String),
    #[error(transparent)]
    Backend(#[from] LoadError),
}

pub(super) fn ggml(error: impl std::fmt::Display) -> GenerateError {
    GenerateError::Ggml(error.to_string())
}

pub(super) fn sampler(deterministic: bool) -> LlamaSampler {
    if deterministic {
        return LlamaSampler::greedy();
    }
    let seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.subsec_nanos());
    LlamaSampler::chain_simple([
        LlamaSampler::top_k(TOP_K),
        LlamaSampler::top_p(TOP_P, 1),
        LlamaSampler::temp(TEMPERATURE),
        LlamaSampler::dist(seed),
    ])
}

/// The tokens of a rendered prompt. The template writes the beginning-of-sequence token itself.
fn prompt_tokens(vocab: &LlamaVocab<'_>, prompt: &str) -> Vec<LlamaToken> {
    vocab.tokenize(prompt.as_bytes(), false, true)
}

impl LoadedModel {
    /// How many tokens a rendered prompt takes, as a generation reads it; the chat measures its requests
    /// with it (spec 086).
    pub fn count_tokens(&self, prompt: &str) -> usize {
        prompt_tokens(&self.model.vocab(), prompt).len()
    }
}

/// Runs one generation. `on_text` gets every piece of text as soon as it is complete (never half a UTF-8
/// character) and returns false when nobody wants more. Blocking; call it on a blocking thread.
pub fn generate(
    loaded: &LoadedModel,
    job: &Job<'_>,
    cancel: &CancellationToken,
    on_text: impl FnMut(&str) -> bool,
) -> Result<Outcome, GenerateError> {
    let backend = backend()?;
    let model = &loaded.model;
    let vocab = model.vocab();
    let tokens = prompt_tokens(&vocab, job.prompt);
    let limit = loaded.placement.context;
    if tokens.len() >= limit as usize {
        return Err(GenerateError::PromptTooLong {
            tokens: tokens.len(),
            context: limit,
        });
    }
    // The context holds the prompt and the answer, not the longest context the memory plan allows.
    let room = limit as usize - tokens.len();
    let max_new = job.max_new_tokens.min(room);
    let n_ctx = (tokens.len() + max_new).max(BATCH);
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    if let Some(drafter) = loaded
        .drafter
        .as_ref()
        .filter(|_| loaded.placement.drafter && max_new > 0)
    {
        let writer = Writer::new(vocab, max_new, on_text);
        return super::speculative::generate_speculative(
            loaded, drafter, &tokens, n_ctx, job, cancel, writer,
        );
    }
    // A layer that looks at a window keeps the window, not the whole context (the plan counts so).
    let params = LlamaContextParams::default()
        .with_swa_full(SWA_FULL)
        .with_n_ctx(NonZeroU32::new(n_ctx as u32))
        .with_n_batch(BATCH as u32)
        .with_n_threads(loaded.threads as i32)
        .with_n_threads_batch(loaded.threads as i32);
    let params = super::context_params::with_choice(params, &loaded.placement, n_ctx);
    let mut ctx = model.new_context(backend, params).map_err(ggml)?;

    let mut batch = LlamaBatch::new(BATCH, 1);
    let mut position = 0i32;
    for chunk in tokens.chunks(BATCH) {
        if cancel.is_cancelled() {
            return Ok(Outcome {
                prompt_tokens: tokens.len(),
                new_tokens: 0,
                ended: Ended::Cancelled,
            });
        }
        batch.clear();
        for (i, token) in chunk.iter().enumerate() {
            let last = position as usize + 1 == tokens.len();
            batch
                .add(*token, position, &[0], last && i == chunk.len() - 1)
                .map_err(ggml)?;
            position += 1;
        }
        ctx.decode(&mut batch).map_err(ggml)?;
    }

    let mut sampler = sampler(job.deterministic);
    let mut writer = Writer::new(vocab, max_new, on_text);
    let ended = loop {
        if cancel.is_cancelled() {
            break Ended::Cancelled;
        }
        if writer.new_tokens >= max_new {
            break Ended::Length;
        }
        let token = sampler.sample(&ctx, batch.n_tokens() - 1);
        if let Some(ended) = writer.take(token) {
            break ended;
        }
        batch.clear();
        batch.add(token, position, &[0], true).map_err(ggml)?;
        ctx.decode(&mut batch).map_err(ggml)?;
        position += 1;
    };
    Ok(writer.finish(tokens.len(), ended))
}

/// Turns sampled tokens into text for `on_text`, the step both token loops share.
pub(super) struct Writer<'a, F> {
    vocab: LlamaVocab<'a>,
    on_text: F,
    /// Bytes of a character the next token completes.
    pending: Vec<u8>,
    pub(super) new_tokens: usize,
    max_new: usize,
}

impl<'a, F: FnMut(&str) -> bool> Writer<'a, F> {
    pub(super) fn new(vocab: LlamaVocab<'a>, max_new: usize, on_text: F) -> Self {
        Self {
            vocab,
            on_text,
            pending: Vec::new(),
            new_tokens: 0,
            max_new,
        }
    }

    /// Takes one sampled token; `Some` when the generation ends with it.
    pub(super) fn take(&mut self, token: LlamaToken) -> Option<Ended> {
        if self.vocab.is_eog(token) {
            return Some(Ended::Stop);
        }
        self.new_tokens += 1;
        self.pending
            .extend(self.vocab.token_to_piece(token, true, None));
        if let Some(text) = take_complete_utf8(&mut self.pending) {
            if !(self.on_text)(&text) {
                return Some(Ended::Cancelled);
            }
        }
        (self.new_tokens >= self.max_new).then_some(Ended::Length)
    }

    /// Ends the generation: bytes that never completed a character are shown as the replacement
    /// character, unless nobody listens any more.
    pub(super) fn finish(mut self, prompt_tokens: usize, ended: Ended) -> Outcome {
        if ended != Ended::Cancelled && !self.pending.is_empty() {
            (self.on_text)(&String::from_utf8_lossy(&self.pending));
        }
        Outcome {
            prompt_tokens,
            new_tokens: self.new_tokens,
            ended,
        }
    }
}

/// Removes and returns the longest valid UTF-8 prefix of `bytes`; a character cut in two by the token
/// boundary stays in `bytes` for the next token. Bytes that can never be valid are replaced, not held.
pub(super) fn take_complete_utf8(bytes: &mut Vec<u8>) -> Option<String> {
    match std::str::from_utf8(bytes) {
        Ok(text) => {
            let text = text.to_owned();
            bytes.clear();
            (!text.is_empty()).then_some(text)
        }
        Err(error) => {
            let valid = error.valid_up_to();
            let mut text = String::from_utf8_lossy(&bytes[..valid]).into_owned();
            match error.error_len() {
                // A sequence that is invalid for good: replace it and go on.
                Some(bad) => {
                    text.push('\u{FFFD}');
                    bytes.drain(..valid + bad);
                    if let Some(more) = take_complete_utf8(bytes) {
                        text.push_str(&more);
                    }
                }
                // The tail is the start of a character the next token completes.
                None => {
                    bytes.drain(..valid);
                }
            }
            (!text.is_empty()).then_some(text)
        }
    }
}

#[cfg(test)]
#[path = "generate_tests.rs"]
mod tests;
