use super::CacheType;
use super::{
    plan, DrafterShape, FreeMemory, Kv, MemoryRefusal, ModelShape, Placement, DEFAULT_CONTEXT,
    TARGET_CONTEXT,
};
use crate::llm::gguf_header::GgufSummary;

pub(in crate::llm::llamacpp) const GIB: u64 = 1 << 30;

/// A 4B model as the Q4_K_M file of Qwen3: 36 blocks, 8 KV heads of 128, 2.4 GiB.
pub(in crate::llm::llamacpp) fn qwen3_4b() -> ModelShape {
    ModelShape::from_header(
        &GgufSummary {
            architecture: "qwen3".to_owned(),
            block_count: Some(36),
            embedding_length: Some(2560),
            head_count: Some(32),
            head_count_kv: Some(8),
            key_length: Some(128),
            context_length: Some(40960),
            ..Default::default()
        },
        2_400 << 20,
    )
    .unwrap()
}

#[test]
fn kv_estimate_counts_k_and_v_in_f16_for_every_layer() {
    // 36 layers × 8 heads × 128 × 4 bytes = 147456 bytes per token.
    assert_eq!(qwen3_4b().kv, Kv::dense(147_456));
}

#[test]
fn head_size_falls_back_to_embedding_over_heads() {
    let summary = GgufSummary {
        architecture: "llama".to_owned(),
        block_count: Some(32),
        embedding_length: Some(4096),
        head_count: Some(32),
        head_count_kv: None,
        key_length: None,
        context_length: None,
        ..Default::default()
    };
    let shape = ModelShape::from_header(&summary, 1).unwrap();
    // KV heads unknown: all 32 heads of 128 count.
    assert_eq!(shape.kv, Kv::dense(32 * 32 * 128 * 4));
    assert_eq!(shape.trained_context, None);
}

#[test]
fn a_header_without_layers_gives_no_shape() {
    let summary = GgufSummary {
        architecture: "x".to_owned(),
        block_count: None,
        embedding_length: None,
        head_count: None,
        head_count_kv: None,
        key_length: None,
        context_length: None,
        ..Default::default()
    };
    assert!(ModelShape::from_header(&summary, 1).is_none());
}

#[test]
fn everything_goes_on_a_roomy_gpu_with_the_wanted_context() {
    let free = FreeMemory {
        device: Some(8 * GIB),
        host: 16 * GIB,
    };
    assert_eq!(
        plan(&qwen3_4b(), free, Some(8192)),
        Ok(Placement {
            gpu_layers: 37,
            context: 8192,
            drafter: false,
            cache: CacheType::Standard,
            yarn_from: None,
        })
    );
}

#[test]
fn context_is_capped_by_what_the_model_was_trained_for() {
    let free = FreeMemory {
        device: Some(24 * GIB),
        host: 16 * GIB,
    };
    let mut shape = qwen3_4b();
    shape.trained_context = Some(4096);
    assert_eq!(plan(&shape, free, Some(32768)).unwrap().context, 4096);
}

#[test]
fn trained_context_below_the_normal_floor_is_preserved() {
    let free = FreeMemory {
        device: Some(24 * GIB),
        host: 16 * GIB,
    };
    let mut shape = qwen3_4b();
    shape.trained_context = Some(1024);
    assert_eq!(plan(&shape, free, Some(8192)).unwrap().context, 1024);
}

#[test]
fn requested_context_below_the_normal_floor_is_preserved() {
    let free = FreeMemory {
        device: Some(24 * GIB),
        host: 16 * GIB,
    };
    let mut shape = qwen3_4b();
    shape.trained_context = Some(4096);
    assert_eq!(plan(&shape, free, Some(512)).unwrap().context, 512);
}

#[test]
fn the_default_context_applies_without_a_catalog_value() {
    let free = FreeMemory {
        device: Some(24 * GIB),
        host: 16 * GIB,
    };
    assert_eq!(
        plan(&qwen3_4b(), free, None).unwrap().context,
        DEFAULT_CONTEXT
    );
}

#[test]
fn a_shorter_context_is_taken_to_keep_everything_on_the_gpu() {
    // 2.4 GiB weights + 768 MiB margin leaves room for about 1.3 GiB of cache: 8192 tokens need 1.15 GiB at
    // 147456 bytes, 16384 would need 2.3 GiB.
    let free = FreeMemory {
        device: Some(4_500 << 20),
        host: 16 * GIB,
    };
    let placed = plan(&qwen3_4b(), free, Some(16384)).unwrap();
    assert_eq!(placed.gpu_layers, 37);
    assert_eq!(placed.context, 8192);
}

#[test]
fn layers_move_to_the_host_when_the_gpu_is_too_small() {
    let free = FreeMemory {
        device: Some(2 * GIB),
        host: 16 * GIB,
    };
    let placed = plan(&qwen3_4b(), free, Some(8192)).unwrap();
    assert!(
        placed.gpu_layers > 0 && placed.gpu_layers < 36,
        "{placed:?}"
    );
    assert_eq!(placed.context, 8192);
}

#[test]
fn a_machine_without_a_gpu_runs_on_the_host() {
    let free = FreeMemory {
        device: None,
        host: 8 * GIB,
    };
    assert_eq!(
        plan(&qwen3_4b(), free, Some(8192)),
        Ok(Placement {
            gpu_layers: 0,
            context: 8192,
            drafter: false,
            cache: CacheType::Standard,
            yarn_from: None,
        })
    );
}

#[test]
fn the_context_shrinks_when_the_host_is_short() {
    // Weights 2.4 GiB + 512 MiB margin; 90 % of the rest must take the layers and the cache.
    let free = FreeMemory {
        device: None,
        host: 4 * GIB,
    };
    let placed = plan(&qwen3_4b(), free, Some(32768)).unwrap();
    assert!(placed.context < 32768, "{placed:?}");
}

#[test]
fn a_model_that_fits_nowhere_is_refused_with_the_numbers() {
    let free = FreeMemory {
        device: Some(GIB),
        host: 2 * GIB,
    };
    let refusal = plan(&qwen3_4b(), free, Some(8192)).unwrap_err();
    assert_eq!(
        refusal,
        MemoryRefusal {
            need_mib: 2400 + 2048 * 147_456 / (1 << 20),
            free_mib: 3 * 1024,
            context: 2048,
        }
    );
    assert!(refusal.to_string().contains("MiB"));
}

/// The drafter of Gemma 4 E4B: 94 MiB, a cache of four small layers.
pub(in crate::llm::llamacpp) fn with_drafter(mut shape: ModelShape) -> ModelShape {
    shape.drafter = Some(DrafterShape {
        file_bytes: 94 << 20,
        kv: Kv::dense(16_384),
    });
    shape
}

#[test]
fn the_drafter_is_used_when_it_fits_next_to_the_whole_model() {
    let free = FreeMemory {
        device: Some(8 * GIB),
        host: 16 * GIB,
    };
    let without = plan(&qwen3_4b(), free, Some(8192)).unwrap();
    let with = plan(&with_drafter(qwen3_4b()), free, Some(8192)).unwrap();
    assert!(with.drafter);
    assert_eq!(
        (with.gpu_layers, with.context),
        (without.gpu_layers, without.context)
    );
}

#[test]
fn a_drafter_that_does_not_fit_next_to_the_whole_model_is_left_out() {
    // 2.4 GiB weights + 1.15 GiB cache at 8192 + 768 MiB margin fit 4.5 GiB; the drafter's 94 MiB, its
    // cache and its 256 MiB margin do not fit as well.
    let free = FreeMemory {
        device: Some(4_500 << 20),
        host: 16 * GIB,
    };
    let without = plan(&qwen3_4b(), free, Some(8192)).unwrap();
    let with = plan(&with_drafter(qwen3_4b()), free, Some(8192)).unwrap();
    assert_eq!(without.gpu_layers, 37);
    assert_eq!(
        with,
        Placement {
            drafter: false,
            ..without
        }
    );
}

#[test]
fn a_model_partly_on_the_gpu_loads_without_its_drafter() {
    let free = FreeMemory {
        device: Some(2 * GIB),
        host: 16 * GIB,
    };
    let without = plan(&qwen3_4b(), free, Some(8192)).unwrap();
    let with = plan(&with_drafter(qwen3_4b()), free, Some(8192)).unwrap();
    assert!(with.gpu_layers < 36);
    assert_eq!(with, without);
}

#[test]
fn a_machine_without_a_gpu_loads_without_the_drafter() {
    let free = FreeMemory {
        device: None,
        host: 16 * GIB,
    };
    assert!(
        !plan(&with_drafter(qwen3_4b()), free, Some(8192))
            .unwrap()
            .drafter
    );
}

#[test]
fn a_model_without_a_drafter_never_gets_one() {
    let free = FreeMemory {
        device: Some(24 * GIB),
        host: 64 * GIB,
    };
    assert!(!plan(&qwen3_4b(), free, Some(8192)).unwrap().drafter);
}

/// Gemma 4 12B as its file says: 48 blocks, five of six layers look at a window of 1024 tokens with
/// 8 KV heads of 256, the sixth looks at everything with one KV head of 512; trained for 262144.
pub(in crate::llm::llamacpp) fn gemma4_12b() -> ModelShape {
    let heads: Vec<u64> = (0..48)
        .map(|layer| if layer % 6 == 5 { 1 } else { 8 })
        .collect();
    let pattern: Vec<bool> = (0..48).map(|layer| layer % 6 != 5).collect();
    ModelShape::from_header(
        &GgufSummary {
            architecture: "gemma4".to_owned(),
            block_count: Some(48),
            embedding_length: Some(3840),
            head_count: Some(16),
            head_count_kv: None,
            key_length: Some(512),
            context_length: Some(262_144),
            head_count_kv_layers: Some(heads),
            value_length: Some(512),
            key_length_swa: Some(256),
            value_length_swa: Some(256),
            sliding_window: Some(1024),
            sliding_window_pattern: Some(pattern),
            ..Default::default()
        },
        7_120_000_000,
    )
    .unwrap()
}

#[test]
fn a_model_with_window_layers_is_not_sized_as_if_every_layer_kept_everything() {
    let shape = gemma4_12b();
    // Eight global layers: 1 head × (512 + 512) values × 2 bytes.
    assert_eq!(shape.kv.growing_per_token, 8 * 1024 * 2);
    // Forty window layers: 8 heads × (256 + 256) values × 2 bytes, up to the window and a margin.
    assert_eq!(shape.kv.windowed_per_token, 40 * 8 * 512 * 2);
    assert_eq!(shape.kv.window_tokens, 1024 + 1024);
    // At 128k that is about 2.8 GiB; counting every layer as global with all 16 heads gave 192 GiB.
    let at_128k = shape.kv.bytes(TARGET_CONTEXT);
    assert!(at_128k < 3 * GIB, "{} MiB", at_128k >> 20);
    let every_layer_global = 48 * 16 * 512 * 4 * u64::from(TARGET_CONTEXT);
    assert!(every_layer_global / at_128k > 50);
}

#[test]
fn a_context_shorter_than_the_window_is_counted_whole() {
    let kv = gemma4_12b().kv;
    assert_eq!(
        kv.bytes(512),
        512 * (kv.growing_per_token + kv.windowed_per_token)
    );
}

#[test]
fn a_header_without_a_window_pattern_counts_every_layer_as_growing() {
    let mut shape = gemma4_12b();
    let summary = GgufSummary {
        architecture: "gemma4".to_owned(),
        block_count: Some(48),
        head_count: Some(16),
        head_count_kv: None,
        key_length: Some(512),
        head_count_kv_layers: Some(vec![8; 48]),
        ..Default::default()
    };
    shape.kv = ModelShape::from_header(&summary, 1).unwrap().kv;
    assert_eq!(shape.kv.windowed_per_token, 0);
    assert_eq!(shape.kv.growing_per_token, 48 * 8 * (512 + 512) * 2);
}

#[test]
fn gemma_4_gets_the_target_context_on_a_card_of_12_gib_and_keeps_every_layer_on_it() {
    let free = FreeMemory {
        device: Some(12 * GIB),
        host: 32 * GIB,
    };
    let placed = plan(&gemma4_12b(), free, Some(TARGET_CONTEXT)).unwrap();
    assert_eq!(placed.context, TARGET_CONTEXT);
    assert_eq!(placed.gpu_layers, 49, "all 48 blocks and the output layer");
}

#[test]
fn a_dense_model_still_gives_up_context_before_layers_while_it_stays_above_the_floor() {
    // Qwen3 4B: 147456 bytes per token. 24 GiB hold 2.4 GiB of weights and 128k of cache (18 GiB).
    let roomy = FreeMemory {
        device: Some(24 * GIB),
        host: 32 * GIB,
    };
    let mut shape = qwen3_4b();
    shape.trained_context = Some(262_144);
    assert_eq!(
        plan(&shape, roomy, Some(TARGET_CONTEXT)).unwrap().context,
        TARGET_CONTEXT
    );
    // 8 GiB: 128k and 64k do not fit, 32k (4.5 GiB) does, with every layer on the card.
    let tight = FreeMemory {
        device: Some(8 * GIB),
        host: 32 * GIB,
    };
    let placed = plan(&shape, tight, Some(TARGET_CONTEXT)).unwrap();
    assert_eq!(placed.context, 32_768);
    assert_eq!(placed.gpu_layers, 37);
}

/// With a real file (`HOLZI_TEST_GGUF=/path/to/model.gguf cargo test -- --ignored a_real_header`): the
/// header is read as the file has it, and a card of 12 GiB gets the target context, or the file's own limit
/// where that is lower, with every layer on it.
#[test]
#[ignore = "needs a GGUF file"]
fn a_real_header_plans_the_target_context_on_12_gib() {
    let path = std::env::var("HOLZI_TEST_GGUF").expect("HOLZI_TEST_GGUF");
    let summary = crate::llm::gguf_header::read_summary(&mut std::io::BufReader::new(
        std::fs::File::open(&path).unwrap(),
    ))
    .unwrap();
    let bytes = std::fs::metadata(&path).unwrap().len();
    let shape = ModelShape::from_header(&summary, bytes).unwrap();
    eprintln!(
        "{}: {} layers, trained {:?}, kv {:?}, at {TARGET_CONTEXT}: {} MiB",
        summary.architecture,
        shape.layers,
        shape.trained_context,
        shape.kv,
        shape.kv.bytes(TARGET_CONTEXT) >> 20
    );
    let free = FreeMemory {
        device: Some(12 * GIB),
        host: 32 * GIB,
    };
    let placed = plan(&shape, free, Some(TARGET_CONTEXT)).unwrap();
    eprintln!("12 GiB card: {placed:?}");
    assert_eq!(
        placed.context,
        TARGET_CONTEXT.min(shape.trained_context.unwrap())
    );
    assert_eq!(placed.gpu_layers, shape.layers + 1);
}

/// Qwen3.5 4B as its file says: 33 blocks, every fourth one attention (4 KV heads of 256), the others
/// keep a state of 4096 × 128 values; trained for 262144.
pub(in crate::llm::llamacpp) fn qwen35_4b() -> ModelShape {
    ModelShape::from_header(
        &GgufSummary {
            architecture: "qwen35".to_owned(),
            block_count: Some(33),
            head_count: Some(16),
            head_count_kv: Some(4),
            key_length: Some(256),
            context_length: Some(262_144),
            full_attention_interval: Some(4),
            ssm_inner_size: Some(4096),
            ssm_state_size: Some(128),
            ..Default::default()
        },
        3_010_000_000,
    )
    .unwrap()
}

#[test]
fn a_hybrid_model_has_a_cache_in_every_fourth_layer_only() {
    let kv = qwen35_4b().kv;
    // Layers 4, 8, ..., 32 are attention layers: 8 × 4 heads × (256 + 256) values × 2 bytes.
    assert_eq!(kv.growing_per_token, 8 * 4 * 512 * 2);
    // The other 25 keep a state of 4096 × 128 × 4 bytes.
    assert_eq!(kv.fixed, 25 * 4096 * 128 * 4);
    assert!(
        kv.bytes(TARGET_CONTEXT) < 5 * GIB,
        "{} MiB",
        kv.bytes(TARGET_CONTEXT) >> 20
    );
}

#[test]
fn a_hybrid_model_gets_the_target_context_on_an_8_gib_card() {
    let free = FreeMemory {
        device: Some(8 * GIB),
        host: 32 * GIB,
    };
    let placed = plan(&qwen35_4b(), free, Some(TARGET_CONTEXT)).unwrap();
    assert_eq!(placed.context, TARGET_CONTEXT);
    assert_eq!(placed.gpu_layers, 34);
}

#[test]
fn a_header_with_more_layers_than_any_model_has_gives_no_shape() {
    let summary = GgufSummary {
        architecture: "qwen3".to_owned(),
        block_count: Some(u64::from(u32::MAX)),
        head_count: Some(8),
        key_length: Some(128),
        ..Default::default()
    };
    assert!(ModelShape::from_header(&summary, 1).is_none());
}

#[test]
fn absurd_header_numbers_never_panic_and_never_fit() {
    let mut shape = gemma4_12b();
    shape.kv = Kv {
        growing_per_token: u64::MAX,
        windowed_per_token: u64::MAX,
        window_tokens: u64::MAX,
        fixed: u64::MAX,
    };
    assert_eq!(shape.kv.bytes(TARGET_CONTEXT), u64::MAX);
    let free = FreeMemory {
        device: Some(12 * GIB),
        host: 32 * GIB,
    };
    assert!(plan(&shape, free, Some(TARGET_CONTEXT)).is_err());

    // A window of u64::MAX in the header: the margin on top of it saturates instead of overflowing.
    let summary = GgufSummary {
        architecture: "gemma4".to_owned(),
        block_count: Some(2),
        head_count: Some(8),
        key_length: Some(64),
        sliding_window: Some(u64::MAX),
        sliding_window_pattern: Some(vec![true, false]),
        ..Default::default()
    };
    let shape = ModelShape::from_header(&summary, 1).unwrap();
    assert_eq!(shape.kv.window_tokens, u64::MAX);
}
