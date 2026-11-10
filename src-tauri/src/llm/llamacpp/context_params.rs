//! What a person's choice (spec 086) changes in the context of one generation: the type of the KV cache
//! and the rope scaling beyond the training. The plain loop and the speculative one both make their
//! contexts through [`with_choice`], so they cannot disagree with the plan that counted the cache.

use llama_cpp_2::context::params::{KvCacheType, LlamaContextParams, RopeScalingType};
use llama_cpp_sys_2::LLAMA_FLASH_ATTN_TYPE_ENABLED;

use super::choice::CacheType;
use super::memory::Placement;

/// The binding's type of a cache type.
fn kv_type(cache: CacheType) -> KvCacheType {
    match cache {
        CacheType::Standard => KvCacheType::F16,
        CacheType::Q8 => KvCacheType::Q8_0,
        CacheType::Q4 => KvCacheType::Q4_0,
    }
}

/// `params` with the cache type of `placement` and, for a generation whose own context `n_ctx` is longer
/// than the model's training, YaRN rope scaling by the factor the load planned for (research R4). A
/// request inside the training runs as the model was trained, however far the load may stretch it.
pub(super) fn with_choice(
    params: LlamaContextParams,
    placement: &Placement,
    n_ctx: usize,
) -> LlamaContextParams {
    let mut params = params
        .with_type_k(kv_type(placement.cache))
        .with_type_v(kv_type(placement.cache));
    if placement.cache.needs_flash_attention() {
        params = params.with_flash_attention_policy(LLAMA_FLASH_ATTN_TYPE_ENABLED);
    }
    match placement.yarn_from {
        Some(trained) if n_ctx > trained as usize && placement.context > trained => params
            .with_rope_scaling_type(RopeScalingType::Yarn)
            .with_rope_freq_scale(trained as f32 / placement.context as f32)
            .with_yarn_orig_ctx(trained),
        _ => params,
    }
}

#[cfg(test)]
#[path = "context_params_tests.rs"]
mod tests;
