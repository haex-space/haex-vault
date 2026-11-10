use llama_cpp_2::context::params::{KvCacheType, LlamaContextParams, RopeScalingType};
use llama_cpp_sys_2::{LLAMA_FLASH_ATTN_TYPE_DISABLED, LLAMA_FLASH_ATTN_TYPE_ENABLED};

use super::with_choice;
use crate::llm::llamacpp::choice::CacheType;
use crate::llm::llamacpp::memory::Placement;

fn placement(cache: CacheType, context: u32, yarn_from: Option<u32>) -> Placement {
    Placement {
        gpu_layers: 37,
        context,
        drafter: false,
        cache,
        yarn_from,
    }
}

fn base() -> LlamaContextParams {
    LlamaContextParams::default().with_flash_attention_policy(LLAMA_FLASH_ATTN_TYPE_DISABLED)
}

#[test]
fn the_standard_cache_leaves_the_engine_as_it_was() {
    let params = with_choice(
        base(),
        &placement(CacheType::Standard, 131_072, None),
        4_000,
    );
    assert!(matches!(params.type_k(), KvCacheType::F16));
    assert!(matches!(params.type_v(), KvCacheType::F16));
    assert_eq!(
        params.flash_attention_policy(),
        LLAMA_FLASH_ATTN_TYPE_DISABLED
    );
    assert!(matches!(
        params.rope_scaling_type(),
        RopeScalingType::Unspecified
    ));
}

#[test]
fn a_smaller_cache_sets_both_types_and_needs_flash_attention() {
    let q8 = with_choice(base(), &placement(CacheType::Q8, 131_072, None), 4_000);
    assert!(matches!(q8.type_k(), KvCacheType::Q8_0));
    assert!(matches!(q8.type_v(), KvCacheType::Q8_0));
    assert_eq!(q8.flash_attention_policy(), LLAMA_FLASH_ATTN_TYPE_ENABLED);
    let q4 = with_choice(base(), &placement(CacheType::Q4, 131_072, None), 4_000);
    assert!(matches!(q4.type_k(), KvCacheType::Q4_0));
    assert!(matches!(q4.type_v(), KvCacheType::Q4_0));
}

#[test]
fn a_long_generation_of_a_stretched_model_uses_yarn_with_the_training_length() {
    let params = with_choice(
        base(),
        &placement(CacheType::Standard, 131_072, Some(32_768)),
        100_000,
    );
    assert!(matches!(params.rope_scaling_type(), RopeScalingType::Yarn));
    assert_eq!(params.yarn_orig_ctx(), 32_768);
    assert!((params.rope_freq_scale() - 0.25).abs() < 1e-6);
}

#[test]
fn a_short_generation_of_a_stretched_model_runs_as_trained() {
    let params = with_choice(
        base(),
        &placement(CacheType::Standard, 131_072, Some(32_768)),
        20_000,
    );
    assert!(matches!(
        params.rope_scaling_type(),
        RopeScalingType::Unspecified
    ));
}
