//! The token loop with an MTP drafter (spec 079 research R6), in the order of llama.cpp's own reference
//! loop (`examples/speculative-simple`): the drafter proposes a few tokens, the model decodes them in one
//! batch, and its sampler decides every position. A proposal only saves a decode step where the sampler
//! would have picked it anyway (spec FR-002).

use std::num::NonZeroU32;

use llama_cpp_2::context::params::{LlamaContextParams, LlamaContextType};
use llama_cpp_2::llama_batch::LlamaBatch;
use llama_cpp_2::model::LlamaModel;
use llama_cpp_2::speculative::{MtpSpeculative, MtpSpeculativeParams};
use llama_cpp_2::token::LlamaToken;
use llama_cpp_sys_2::LLAMA_FLASH_ATTN_TYPE_DISABLED;
use tokio_util::sync::CancellationToken;

use super::generate::{ggml, sampler, Ended, GenerateError, Job, Outcome, Writer, BATCH, SWA_FULL};
use super::model::{backend, LoadedModel};

/// Proposals per round, as in the measurement of research R1.
const DRAFT_MAX: i32 = 4;
/// The drafter's batch. Gemma 4's drafter shares the model's cache and decodes one token per draft step,
/// never the prompt, so a small batch is enough; the batch of 512 would cost about 1 GiB of compute
/// buffers on the GPU.
const DRAFT_BATCH: u32 = 8;

/// The tokens one round keeps: the proposals the model agrees with, then the model's own next token.
/// `sample(i)` asks the model for position `i` of the decoded batch; nothing after the first disagreement
/// is sampled, so a random sampler sees the same calls as in the plain loop. Stops after an end token.
pub(super) fn kept_tokens(
    draft: &[LlamaToken],
    mut sample: impl FnMut(usize) -> LlamaToken,
    is_end: impl Fn(LlamaToken) -> bool,
) -> Vec<LlamaToken> {
    let mut kept = Vec::with_capacity(draft.len() + 1);
    for i in 0..=draft.len() {
        let token = sample(i);
        kept.push(token);
        if is_end(token) || draft.get(i) != Some(&token) {
            break;
        }
    }
    kept
}

/// Context settings both contexts share. Flash attention is off on both: the drafter aborts in the CUDA
/// flash-attention kernel (research R8), and with it on for the model alone, the drafter reads the model's
/// cache so slowly that writing takes twice as long (measured 2026-11-04).
fn context_params(loaded: &LoadedModel, n_ctx: usize) -> LlamaContextParams {
    LlamaContextParams::default()
        .with_n_ctx(NonZeroU32::new(n_ctx as u32))
        .with_n_batch(BATCH as u32)
        .with_n_threads(loaded.threads as i32)
        .with_n_threads_batch(loaded.threads as i32)
        .with_flash_attention_policy(LLAMA_FLASH_ATTN_TYPE_DISABLED)
}

/// Runs one generation with the drafter. Blocking, like [`super::generate::generate`], which hands over
/// to it.
pub(super) fn generate_speculative<F: FnMut(&str) -> bool>(
    loaded: &LoadedModel,
    drafter: &LlamaModel,
    tokens: &[LlamaToken],
    n_ctx: usize,
    job: &Job<'_>,
    cancel: &CancellationToken,
    mut writer: Writer<'_, F>,
) -> Result<Outcome, GenerateError> {
    let backend = backend()?;
    let model = &loaded.model;
    let target = model
        .new_context(
            backend,
            context_params(loaded, n_ctx).with_swa_full(SWA_FULL),
        )
        .map_err(ggml)?;
    let draft_params = context_params(loaded, n_ctx)
        .with_n_batch(DRAFT_BATCH)
        .with_n_ubatch(DRAFT_BATCH)
        .with_context_type(LlamaContextType::Mtp);
    let draft = drafter
        .new_context_with_ctx_other(backend, draft_params, &target)
        .map_err(ggml)?;
    let params = MtpSpeculativeParams {
        n_max: DRAFT_MAX,
        ..MtpSpeculativeParams::default()
    };
    let mut spec = MtpSpeculative::new(target, draft, params).map_err(ggml)?;

    // The whole prompt, as in the plain loop, so the first token comes as early as there: the drafter only
    // starts after it.
    let mut batch = LlamaBatch::new(BATCH, 1);
    for (start, chunk) in tokens.chunks(BATCH).enumerate() {
        if cancel.is_cancelled() {
            return Ok(writer.finish(tokens.len(), Ended::Cancelled));
        }
        batch.clear();
        for (i, token) in chunk.iter().enumerate() {
            let position = start * BATCH + i;
            batch
                .add(*token, position as i32, &[0], position + 1 == tokens.len())
                .map_err(ggml)?;
        }
        spec.target_context_mut().decode(&mut batch).map_err(ggml)?;
        spec.process(&batch).map_err(ggml)?;
    }
    let mut history = tokens.to_vec();
    spec.begin(&history).map_err(ggml)?;

    let vocab = model.vocab();
    let mut sampler = sampler(job.deterministic);
    let mut id_last = sampler.sample(spec.target_context(), batch.n_tokens() - 1);
    if let Some(ended) = writer.take(id_last) {
        return Ok(writer.finish(tokens.len(), ended));
    }
    let mut n_past = tokens.len();
    let ended = 'generation: loop {
        if cancel.is_cancelled() {
            break Ended::Cancelled;
        }
        let mut proposal = spec.draft(n_past as i32, id_last, &history).map_err(ggml)?;
        // The decoded batch must fit the context: the token of this round plus the proposals.
        proposal.truncate(n_ctx.saturating_sub(n_past + 1));
        trim(&mut spec, n_past)?;

        batch.clear();
        batch
            .add(id_last, n_past as i32, &[0], true)
            .map_err(ggml)?;
        for (i, token) in proposal.iter().enumerate() {
            batch
                .add(*token, (n_past + 1 + i) as i32, &[0], true)
                .map_err(ggml)?;
        }
        spec.target_context_mut().decode(&mut batch).map_err(ggml)?;
        spec.process(&batch).map_err(ggml)?;

        let kept = kept_tokens(
            &proposal,
            |i| sampler.sample(spec.target_context(), i as i32),
            |token| vocab.is_eog(token),
        );
        if !proposal.is_empty() {
            spec.accept((kept.len() - 1) as u16).map_err(ggml)?;
        }
        n_past += kept.len();
        trim(&mut spec, n_past)?;

        for token in kept {
            history.push(id_last);
            id_last = token;
            if let Some(ended) = writer.take(token) {
                break 'generation ended;
            }
        }
        if n_past + 1 >= n_ctx {
            break Ended::Length;
        }
    };
    Ok(writer.finish(tokens.len(), ended))
}

/// Removes everything from position `n_past` on from both caches: the rejected proposals.
fn trim(spec: &mut MtpSpeculative<'_>, n_past: usize) -> Result<(), GenerateError> {
    let from = Some(n_past as u32);
    let target = spec
        .target_context_mut()
        .clear_kv_cache_seq(Some(0), from, None)
        .map_err(ggml)?;
    let draft = spec
        .draft_context_mut()
        .clear_kv_cache_seq(Some(0), from, None)
        .map_err(ggml)?;
    if target && draft {
        Ok(())
    } else {
        Err(GenerateError::Ggml(format!(
            "the cache refused to drop the positions from {n_past} on"
        )))
    }
}

#[cfg(test)]
#[path = "speculative_tests.rs"]
mod tests;
