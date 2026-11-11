//! What the settings page is told about a choice (spec 086 FR-006 to FR-009).

use super::{report, Reason, Verdict};
use crate::adapters::PlacementInfo;
use crate::llm::llamacpp::choice::{CacheType, ContextChoice};
use crate::llm::llamacpp::memory::tests::{gemma4_12b, qwen3_4b, GIB};
use crate::llm::llamacpp::memory::{FreeMemory, ModelShape, TARGET_CONTEXT};

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

fn ask(shape: &ModelShape, free: FreeMemory, choice: ContextChoice) -> super::ContextReport {
    report(
        Some(shape),
        free,
        TARGET_CONTEXT,
        choice,
        CacheType::Standard,
        None,
    )
}

#[test]
fn auto_on_a_roomy_card_fits_and_shows_what_the_file_says() {
    let found = ask(&gemma4_12b(), card(12), ContextChoice::Auto);
    assert_eq!(found.verdict, Verdict::Fits);
    assert_eq!(found.limits.trained, Some(262_144));
    assert_eq!(found.yarn, None);
    assert!(found.sized);
    assert_eq!(found.effective, None);
}

#[test]
fn a_context_beyond_the_training_says_by_how_much_it_stretches() {
    let found = ask(&qwen3_32k(), card(24), ContextChoice::Fixed(65_536));
    assert_eq!(found.verdict, Verdict::Fits);
    assert_eq!(found.yarn, Some(2.0));
}

#[test]
fn a_context_that_needs_the_host_is_slower_and_one_that_fits_nowhere_says_what_is_missing() {
    let found = ask(&qwen3_32k(), card(4), ContextChoice::Fixed(32_768));
    assert_eq!(found.verdict, Verdict::FitsOnHost);

    let small = FreeMemory {
        device: Some(2 * GIB),
        host: 3 * GIB,
    };
    let found = ask(&qwen3_32k(), small, ContextChoice::Fixed(32_768));
    assert_eq!(found.verdict, Verdict::TooLarge);
    assert!(found.need_mib.unwrap() > found.free_mib.unwrap());
}

#[test]
fn the_rules_of_a_choice_are_named_as_verdicts() {
    let verdict =
        |shape: &ModelShape, asked| ask(shape, card(24), ContextChoice::Fixed(asked)).verdict;
    assert_eq!(verdict(&qwen3_32k(), 1024), Verdict::BelowFloor);
    assert_eq!(verdict(&qwen3_32k(), 200_000), Verdict::BeyondTraining);
    let mut fixed_rope = qwen3_32k();
    fixed_rope.stretchable = false;
    assert_eq!(verdict(&fixed_rope, 65_536), Verdict::NotStretchable);
}

#[test]
fn a_header_that_cannot_be_sized_says_so_and_still_shows_what_is_loaded() {
    let info = PlacementInfo {
        context: 8192,
        gpu_layers: 999,
        layers: 30,
        cache: "standard",
        yarn_from: None,
    };
    let found = report(
        None,
        card(8),
        TARGET_CONTEXT,
        ContextChoice::Auto,
        CacheType::Standard,
        Some(info.clone()),
    );
    assert_eq!(found.verdict, Verdict::Unsized);
    assert!(!found.sized);
    assert_eq!(found.effective.unwrap().info, info);
}

#[test]
fn the_context_in_effect_says_why_it_is_what_it_is() {
    let info = |context| PlacementInfo {
        context,
        gpu_layers: 37,
        layers: 36,
        cache: "standard",
        yarn_from: None,
    };
    let reason = |context, choice| {
        report(
            Some(&qwen3_32k()),
            card(24),
            TARGET_CONTEXT,
            choice,
            CacheType::Standard,
            Some(info(context)),
        )
        .effective
        .unwrap()
        .reason
    };
    assert_eq!(reason(131_072, ContextChoice::Auto), Reason::Target);
    assert_eq!(reason(32_768, ContextChoice::Auto), Reason::Trained);
    assert_eq!(reason(16_384, ContextChoice::Auto), Reason::Memory);
    assert_eq!(reason(65_536, ContextChoice::Fixed(65_536)), Reason::Choice);
}

#[test]
fn the_report_is_json_the_page_can_read() {
    let found = ask(&qwen3_32k(), card(24), ContextChoice::Fixed(65_536));
    let json = serde_json::to_value(&found).unwrap();
    assert_eq!(json["verdict"], "fits");
    assert_eq!(json["trained"], 32_768);
    assert_eq!(json["maxStretched"], 131_072);
    assert_eq!(json["yarn"], 2.0);
    assert!(json["maxOnGpu"].is_number());
}

/// With a real file (`HOLZI_TEST_GGUF=/path/to/model.gguf cargo test -- --ignored a_real_file --nocapture`): what
/// the settings page would show for it on a card of 12 GiB, with each cache type.
#[test]
#[ignore = "needs a GGUF file"]
fn a_real_file_reports_what_the_page_would_show() {
    use crate::llm::llamacpp::choice::CacheType;
    use crate::llm::llamacpp::model::read_shape;

    let path = std::env::var("HOLZI_TEST_GGUF").expect("HOLZI_TEST_GGUF names a .gguf file");
    let shape = read_shape(std::path::Path::new(&path), None)
        .expect("a readable header")
        .expect("a header the planner can size");
    println!(
        "{path}: {} layers, trained {:?}, stretchable {}",
        shape.layers, shape.trained_context, shape.stretchable
    );
    for cache in [CacheType::Standard, CacheType::Q8, CacheType::Q4] {
        let found = report(
            Some(&shape),
            card(12),
            TARGET_CONTEXT,
            ContextChoice::Auto,
            cache,
            None,
        );
        println!(
            "{}: {}",
            cache.name(),
            serde_json::to_string(&found).unwrap()
        );
    }
}
