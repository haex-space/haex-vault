//! Unit tests for the built-in STT catalog and its tier recommendation.

#[cfg(any(feature = "llm-cpu", feature = "voice"))]
use super::catalog::recommend_tiers;
use super::catalog::{entries, get, SttEngine};
#[cfg(any(feature = "llm-cpu", feature = "voice"))]
use crate::hardware::tiers::Tier;
#[cfg(any(feature = "llm-cpu", feature = "voice"))]
use crate::hardware::{Backend, HardwareInfo};
#[cfg(any(feature = "llm-cpu", feature = "voice"))]
use crate::platform::ModelPresets;

#[cfg(any(feature = "llm-cpu", feature = "voice"))]
fn hw(backend: Backend, ram: u64, vram: Option<u64>) -> HardwareInfo {
    HardwareInfo {
        backend,
        total_ram_bytes: ram,
        available_ram_bytes: ram,
        vram_bytes: vram,
    }
}

#[test]
fn catalog_contains_the_three_whisper_tiers() {
    let tiny = get("whisper-tiny")
        .expect("catalog should parse")
        .expect("whisper-tiny entry");
    assert_eq!(tiny.hf_repo, "openai/whisper-tiny");
    assert!(!tiny.hf_revision.is_empty());
    assert_ne!(tiny.hf_revision, "main");

    let base = get("whisper-base")
        .expect("catalog should parse")
        .expect("whisper-base entry");
    assert_eq!(base.hf_repo, "openai/whisper-base");

    let small = get("whisper-small")
        .expect("catalog should parse")
        .expect("whisper-small entry");
    assert_eq!(small.hf_repo, "openai/whisper-small");

    for whisper in [tiny, base, small] {
        assert_eq!(whisper.engine, SttEngine::CandleWhisper);
        assert!(whisper.languages.is_empty());
    }
    assert_eq!(entries().expect("catalog should parse").len(), 4);
}

/// Spec 066 T131: the Parakeet entry is pinned, credited and says which
/// languages it knows.
#[test]
fn catalog_contains_a_pinned_parakeet_entry() {
    let parakeet = get("parakeet-primeline")
        .expect("catalog should parse")
        .expect("parakeet-primeline entry");
    assert_eq!(parakeet.engine, SttEngine::SherpaParakeet);
    assert_eq!(parakeet.hf_revision.len(), 40);
    assert!(parakeet.hf_revision.chars().all(|c| c.is_ascii_hexdigit()));
    assert_eq!(parakeet.license, "CC-BY-4.0");
    assert!(parakeet
        .attribution
        .as_deref()
        .is_some_and(|credit| credit.contains("primeline") && credit.contains("NVIDIA")));
    assert_eq!(parakeet.languages.len(), 25);
    for language in ["de", "en", "fr", "es", "it", "pt"] {
        assert!(
            parakeet.languages.iter().any(|l| l == language),
            "{language}"
        );
    }
}

#[test]
fn every_entry_names_its_engine_on_the_wire() {
    for entry in entries().expect("catalog should parse") {
        let value = serde_json::to_value(entry).expect("serializes");
        let engine = value["engine"].as_str().expect("engine is a string");
        assert!(
            ["candle-whisper", "sherpa-parakeet"].contains(&engine),
            "{}: {engine}",
            entry.id
        );
    }
}

#[test]
fn get_returns_none_for_unknown_id() {
    assert!(get("whisper-does-not-exist")
        .expect("catalog should parse")
        .is_none());
}

#[test]
fn catalog_entry_serializes_with_frontend_field_names() {
    let value = serde_json::to_value(
        get("whisper-tiny")
            .expect("catalog should parse")
            .expect("whisper-tiny entry"),
    )
    .expect("catalog entry should serialize");

    assert_eq!(value["hfRepo"], "openai/whisper-tiny");
    assert_eq!(
        value["hfRevision"],
        "169d4a4341b33bc18d8881c4b69c2e104e1cc0af"
    );
    assert_eq!(value["approxSizeBytes"], 153_544_121);
    assert!(value.get("hf_repo").is_none());
    assert!(value.get("hf_revision").is_none());
    assert!(value.get("approx_size_bytes").is_none());
}

#[test]
#[cfg(any(feature = "llm-cpu", feature = "voice"))]
/// Every tier is tiny relative to any realistic RAM budget, so on a roomy
/// host all three should classify as `Fits`, and the recommendation should
/// still be exactly three chips (Easy = smallest, Sweet/Max = largest).
fn recommend_tiers_on_roomy_host_prefers_smallest_for_easy() {
    let info = hw(Backend::Cpu, 16 * 1024 * 1024 * 1024, None);
    let tiers = recommend_tiers(
        &info,
        ModelPresets::Desktop,
        &[SttEngine::CandleWhisper, SttEngine::SherpaParakeet],
    )
    .expect("catalog should parse")
    .expect("non-empty catalog");
    assert_eq!(tiers[0].tier, Tier::Easy);
    assert_eq!(tiers[0].entry.id, "whisper-tiny");
    assert_eq!(tiers[1].tier, Tier::Sweet);
    assert_eq!(
        tiers[1].entry.id,
        if cfg!(feature = "voice") {
            "parakeet-primeline"
        } else {
            "whisper-tiny"
        }
    );
    assert_eq!(tiers[2].tier, Tier::Max);
    assert_eq!(tiers[2].entry.id, "whisper-small");
}

#[test]
#[cfg(any(feature = "llm-cpu", feature = "voice"))]
fn recommend_tiers_returns_three_even_on_a_tiny_host() {
    // Even a very constrained host still gets three tier chips — Whisper
    // sizes are small enough that this is mostly a defensive check.
    let info = hw(Backend::Cpu, 256 * 1024 * 1024, None);
    let tiers = recommend_tiers(
        &info,
        ModelPresets::Desktop,
        &[SttEngine::CandleWhisper, SttEngine::SherpaParakeet],
    )
    .expect("catalog should parse")
    .expect("non-empty catalog");
    assert_eq!(tiers.len(), 3);
}

#[test]
#[cfg(any(feature = "llm-cpu", feature = "voice"))]
fn a_phone_is_offered_the_smallest_model_and_keeps_the_larger_ones() {
    // Room for every model: on a desktop the suggestion would be the largest.
    let info = hw(Backend::Cpu, 16 * 1024 * 1024 * 1024, None);
    let tiers = recommend_tiers(
        &info,
        ModelPresets::Phone,
        &[SttEngine::CandleWhisper, SttEngine::SherpaParakeet],
    )
    .expect("catalog should parse")
    .expect("non-empty catalog");
    assert_eq!(tiers[1].tier, Tier::Sweet);
    assert_eq!(
        tiers[1].entry.id,
        if cfg!(feature = "llm-cpu") {
            "whisper-tiny"
        } else {
            "parakeet-primeline"
        }
    );
    assert_eq!(tiers[2].entry.id, "whisper-small");

    let desktop = recommend_tiers(
        &info,
        ModelPresets::Desktop,
        &[SttEngine::CandleWhisper, SttEngine::SherpaParakeet],
    )
    .expect("catalog should parse")
    .expect("non-empty catalog");
    assert_eq!(
        desktop[1].entry.id,
        if cfg!(feature = "voice") {
            "parakeet-primeline"
        } else {
            "whisper-tiny"
        }
    );
}

#[test]
#[cfg(any(feature = "llm-cpu", feature = "voice"))]
fn exactly_one_entry_is_the_default_and_it_follows_the_device_kind() {
    use super::catalog::entries_with_fit;
    let info = hw(Backend::Cpu, 16 * 1024 * 1024 * 1024, None);
    for (presets, id) in [
        (
            ModelPresets::Desktop,
            if cfg!(feature = "voice") {
                "parakeet-primeline"
            } else {
                "whisper-tiny"
            },
        ),
        (
            ModelPresets::Phone,
            if cfg!(feature = "llm-cpu") {
                "whisper-tiny"
            } else {
                "parakeet-primeline"
            },
        ),
    ] {
        let listed = entries_with_fit(
            &info,
            presets,
            &[SttEngine::CandleWhisper, SttEngine::SherpaParakeet],
        )
        .expect("catalog should parse");
        let defaults: Vec<_> = listed.iter().filter(|e| e.is_default).collect();
        assert_eq!(defaults.len(), 1);
        assert_eq!(defaults[0].entry.id, id);
        let wire = serde_json::to_value(defaults[0]).expect("serializes");
        assert_eq!(wire["isDefault"], true);
    }
}
