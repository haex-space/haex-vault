use super::{plan, FreeMemory, MemoryRefusal, ModelShape, Placement, DEFAULT_CONTEXT};
use crate::llm::local::gguf_header::GgufSummary;

const GIB: u64 = 1 << 30;

/// A 4B model as the Q4_K_M file of Qwen3: 36 blocks, 8 KV heads of 128, 2.4 GiB.
fn qwen3_4b() -> ModelShape {
    ModelShape::from_header(
        &GgufSummary {
            architecture: "qwen3".to_owned(),
            block_count: Some(36),
            embedding_length: Some(2560),
            head_count: Some(32),
            head_count_kv: Some(8),
            key_length: Some(128),
            context_length: Some(40960),
        },
        2_400 << 20,
    )
    .unwrap()
}

#[test]
fn kv_estimate_counts_k_and_v_in_f16_for_every_layer() {
    // 36 layers × 8 heads × 128 × 4 bytes = 147456 bytes per token.
    assert_eq!(qwen3_4b().kv_bytes_per_token, 147_456);
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
    };
    let shape = ModelShape::from_header(&summary, 1).unwrap();
    // KV heads unknown: all 32 heads of 128 count.
    assert_eq!(shape.kv_bytes_per_token, 32 * 32 * 128 * 4);
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
            context: 8192
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
            context: 8192
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
