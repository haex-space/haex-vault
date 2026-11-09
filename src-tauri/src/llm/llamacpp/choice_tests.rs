//! The choice of a context length and a cache type against the planner (spec 086): the numbers are those of
//! the real models, the memory is set by the test.

use super::{check_fixed, limits, CacheType, ContextChoice, PlanError, MAX_STRETCH};
use crate::llm::llamacpp::memory::tests::{gemma4_12b, qwen3_4b, with_drafter, GIB};
use crate::llm::llamacpp::memory::{plan_with, FreeMemory, ModelShape, Placement, TARGET_CONTEXT};

/// Qwen3 4B as its file says: trained for 32768, no rope scaling of its own.
fn qwen3_32k() -> ModelShape {
    ModelShape {
        trained_context: Some(32_768),
        ..qwen3_4b()
    }
}

fn card(device_gib: u64) -> FreeMemory {
    FreeMemory {
        device: Some(device_gib * GIB),
        host: 64 * GIB,
    }
}

#[test]
fn a_smaller_cache_is_counted_as_the_smaller_cache() {
    assert_eq!(CacheType::Standard.scale(6400), 6400);
    assert_eq!(CacheType::Q8.scale(6400), 3400);
    assert_eq!(CacheType::Q4.scale(6400), 1800);
    let gib = |bytes: u64| bytes as f64 / GIB as f64;
    let kv = qwen3_4b().kv;
    assert!((gib(kv.bytes(131_072)) - 18.0).abs() < 0.01);
    assert!((gib(kv.bytes_with(131_072, CacheType::Q8)) - 9.56).abs() < 0.01);
    assert!((gib(kv.bytes_with(131_072, CacheType::Q4)) - 5.06).abs() < 0.01);
}

#[test]
fn the_state_of_a_hybrid_model_is_not_made_smaller_by_a_smaller_cache() {
    let hybrid = crate::llm::llamacpp::memory::tests::qwen35_4b().kv;
    assert!(hybrid.fixed > 0);
    let standard = hybrid.bytes(2048);
    let q4 = hybrid.bytes_with(2048, CacheType::Q4);
    assert!(q4 < standard);
    assert!(q4 >= hybrid.fixed);
}

#[test]
fn a_fixed_context_that_fits_is_placed_as_asked() {
    let placed = plan_with(
        &qwen3_32k(),
        card(24),
        Some(TARGET_CONTEXT),
        ContextChoice::Fixed(32_768),
        CacheType::Standard,
    );
    assert_eq!(
        placed,
        Ok(Placement {
            gpu_layers: 37,
            context: 32_768,
            drafter: false,
            cache: CacheType::Standard,
            yarn_from: None,
        })
    );
}

#[test]
fn a_context_beyond_the_training_is_stretched_and_says_from_where() {
    let placed = plan_with(
        &qwen3_32k(),
        card(24),
        Some(TARGET_CONTEXT),
        ContextChoice::Fixed(65_536),
        CacheType::Q8,
    )
    .unwrap();
    assert_eq!(placed.context, 65_536);
    assert_eq!(placed.cache, CacheType::Q8);
    assert_eq!(placed.yarn_from, Some(32_768));
    assert_eq!(placed.gpu_layers, 37);
}

#[test]
fn a_fixed_context_is_refused_beyond_four_times_the_training() {
    let asked = 32_768 * MAX_STRETCH + 1;
    assert_eq!(
        plan_with(
            &qwen3_32k(),
            card(24),
            Some(TARGET_CONTEXT),
            ContextChoice::Fixed(asked),
            CacheType::Q8,
        ),
        Err(PlanError::BeyondTraining {
            asked,
            trained: 32_768,
            max: 131_072
        })
    );
}

#[test]
fn a_model_with_a_rope_scaling_of_its_own_is_not_stretched() {
    let mut shape = qwen3_32k();
    shape.stretchable = false;
    assert_eq!(
        check_fixed(&shape, 65_536),
        Err(PlanError::NotStretchable {
            asked: 65_536,
            trained: 32_768
        })
    );
    // Up to its own length it is fine.
    assert_eq!(check_fixed(&shape, 32_768), Ok(()));
}

#[test]
fn a_fixed_context_below_the_floor_is_refused() {
    assert_eq!(
        check_fixed(&qwen3_32k(), 1024),
        Err(PlanError::BelowFloor {
            asked: 1024,
            floor: 2048
        })
    );
}

#[test]
fn a_model_of_unknown_length_may_be_asked_for_any_context_the_memory_holds() {
    let mut shape = qwen3_4b();
    shape.trained_context = None;
    assert_eq!(check_fixed(&shape, 500_000), Ok(()));
}

#[test]
fn a_fixed_context_that_needs_the_host_keeps_its_length_and_puts_layers_on_the_host() {
    // 2.4 GiB of weights and 4.5 GiB of cache at 32768 do not fit 4 GiB of device.
    let placed = plan_with(
        &qwen3_32k(),
        card(4),
        Some(TARGET_CONTEXT),
        ContextChoice::Fixed(32_768),
        CacheType::Standard,
    )
    .unwrap();
    assert_eq!(placed.context, 32_768);
    assert!(
        placed.gpu_layers > 0 && placed.gpu_layers < 36,
        "{placed:?}"
    );
}

#[test]
fn a_fixed_context_that_fits_nowhere_is_refused_with_the_numbers_and_not_made_smaller() {
    let free = FreeMemory {
        device: None,
        host: 2 * GIB,
    };
    let Err(PlanError::Memory(refusal)) = plan_with(
        &qwen3_32k(),
        free,
        Some(TARGET_CONTEXT),
        ContextChoice::Fixed(32_768),
        CacheType::Standard,
    ) else {
        panic!("expected a memory refusal");
    };
    assert_eq!(refusal.context, 32_768);
    assert!(refusal.need_mib > refusal.free_mib);
}

#[test]
fn a_smaller_cache_keeps_the_whole_model_on_the_gpu_where_the_standard_one_cannot() {
    let mut shape = qwen3_4b();
    shape.trained_context = Some(131_072);
    let on = |cache| {
        plan_with(
            &shape,
            card(6),
            Some(TARGET_CONTEXT),
            ContextChoice::Auto,
            cache,
        )
        .unwrap()
    };
    let standard = on(CacheType::Standard);
    let q8 = on(CacheType::Q8);
    assert!(standard.gpu_layers <= 36, "{standard:?}");
    assert_eq!(q8.gpu_layers, 37);
    assert_eq!(q8.context, 32_768);
    assert_eq!(q8.cache, CacheType::Q8);
    assert_eq!(q8.yarn_from, None);
}

#[test]
fn auto_never_stretches_a_model() {
    let placed = plan_with(
        &qwen3_32k(),
        card(48),
        Some(TARGET_CONTEXT),
        ContextChoice::Auto,
        CacheType::Standard,
    )
    .unwrap();
    assert_eq!(placed.context, 32_768);
    assert_eq!(placed.yarn_from, None);
}

#[test]
fn the_drafter_is_left_out_with_a_smaller_cache_and_with_a_stretched_model() {
    let shape = with_drafter(qwen3_32k());
    let with = |choice, cache| {
        plan_with(&shape, card(48), Some(TARGET_CONTEXT), choice, cache)
            .unwrap()
            .drafter
    };
    assert!(with(ContextChoice::Auto, CacheType::Standard));
    assert!(!with(ContextChoice::Auto, CacheType::Q8));
    assert!(!with(ContextChoice::Fixed(65_536), CacheType::Standard));
}

#[test]
fn the_limits_of_gemma_on_a_card_of_12_gib_reach_beyond_the_target_and_grow_with_a_smaller_cache() {
    let standard = limits(&gemma4_12b(), card(12), CacheType::Standard);
    let q8 = limits(&gemma4_12b(), card(12), CacheType::Q8);
    assert_eq!(standard.trained, Some(262_144));
    assert_eq!(standard.max_stretched, Some(262_144 * MAX_STRETCH));
    let on_gpu = standard.max_on_gpu.unwrap();
    assert!(on_gpu > TARGET_CONTEXT, "{standard:?}");
    assert!(on_gpu < 262_144 * MAX_STRETCH, "{standard:?}");
    assert!(q8.max_on_gpu.unwrap() > on_gpu);
    assert!(standard.max_any.unwrap() >= on_gpu);
}

#[test]
fn the_limits_of_qwen3_stop_at_what_it_can_be_stretched_to() {
    let standard = limits(&qwen3_32k(), card(12), CacheType::Standard);
    let q8 = limits(&qwen3_32k(), card(12), CacheType::Q8);
    assert_eq!(standard.max_stretched, Some(131_072));
    let on_gpu = standard.max_on_gpu.unwrap();
    assert!(on_gpu > 32_768 && on_gpu < 131_072, "{standard:?}");
    assert!(q8.max_on_gpu.unwrap() > on_gpu);
    assert!(q8.max_on_gpu.unwrap() <= 131_072);
}

#[test]
fn a_model_that_cannot_be_stretched_has_no_stretched_limit_and_stops_at_its_own_length() {
    let mut shape = qwen3_32k();
    shape.stretchable = false;
    let found = limits(&shape, card(48), CacheType::Standard);
    assert_eq!(found.max_stretched, None);
    assert_eq!(found.max_on_gpu, Some(32_768));
}

#[test]
fn without_a_gpu_nothing_fits_on_it_and_a_machine_too_small_for_the_floor_has_no_limit() {
    let host_only = FreeMemory {
        device: None,
        host: 32 * GIB,
    };
    let found = limits(&qwen3_32k(), host_only, CacheType::Standard);
    assert_eq!(found.max_on_gpu, None);
    assert!(found.max_any.is_some());
    let tiny = FreeMemory {
        device: Some(100 << 20),
        host: 100 << 20,
    };
    let found = limits(&qwen3_32k(), tiny, CacheType::Standard);
    assert_eq!((found.max_on_gpu, found.max_any), (None, None));
}
