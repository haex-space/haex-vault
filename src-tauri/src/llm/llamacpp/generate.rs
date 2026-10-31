//! The token loop (spec 070, plan D5): one context per generation, prompt in batches, then one token at a
//! time until the model stops, the limit is reached, the caller cancels or the receiver is gone.

use std::num::NonZeroU32;

use llama_cpp_2::context::params::LlamaContextParams;
use llama_cpp_2::llama_batch::LlamaBatch;
use llama_cpp_2::sampling::LlamaSampler;
use tokio_util::sync::CancellationToken;

use super::model::{backend, LoadError, LoadedModel};

/// Prompt tokens decoded per call. The cancel flag is looked at between batches, so this bounds how long
/// a cancel during a long prompt can wait.
const BATCH: usize = 512;
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

fn ggml(error: impl std::fmt::Display) -> GenerateError {
    GenerateError::Ggml(error.to_string())
}

fn sampler(deterministic: bool) -> LlamaSampler {
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

/// Runs one generation. `on_text` gets every piece of text as soon as it is complete (never half a UTF-8
/// character) and returns false when nobody wants more. Blocking; call it on a blocking thread.
pub fn generate(
    loaded: &LoadedModel,
    job: &Job<'_>,
    cancel: &CancellationToken,
    mut on_text: impl FnMut(&str) -> bool,
) -> Result<Outcome, GenerateError> {
    let backend = backend()?;
    let model = &loaded.model;
    let vocab = model.vocab();
    // The template writes the beginning-of-sequence token itself.
    let tokens = vocab.tokenize(job.prompt.as_bytes(), false, true);
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
    let params = LlamaContextParams::default()
        .with_n_ctx(NonZeroU32::new(n_ctx as u32))
        .with_n_batch(BATCH as u32);
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
    let mut pending: Vec<u8> = Vec::new();
    let mut new_tokens = 0;
    let ended = loop {
        if cancel.is_cancelled() {
            break Ended::Cancelled;
        }
        if new_tokens >= max_new {
            break Ended::Length;
        }
        let token = sampler.sample(&ctx, batch.n_tokens() - 1);
        if vocab.is_eog(token) {
            break Ended::Stop;
        }
        new_tokens += 1;
        pending.extend(vocab.token_to_piece(token, true, None));
        if let Some(text) = take_complete_utf8(&mut pending) {
            if !on_text(&text) {
                break Ended::Cancelled;
            }
        }
        batch.clear();
        batch.add(token, position, &[0], true).map_err(ggml)?;
        ctx.decode(&mut batch).map_err(ggml)?;
        position += 1;
    };
    if ended != Ended::Cancelled && !pending.is_empty() {
        // Bytes that never completed a character: show them as the replacement character.
        on_text(&String::from_utf8_lossy(&pending));
    }
    Ok(Outcome {
        prompt_tokens: tokens.len(),
        new_tokens,
        ended,
    })
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
