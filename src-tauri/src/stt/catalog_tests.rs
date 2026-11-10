//! Unit tests for the Parakeet STT catalog and recommendation.

#[cfg(feature = "voice")]
use super::catalog::recommend_tiers;
use super::catalog::{default_entry, entries, get, SttEngine, DEFAULT_MODEL_ID};
#[cfg(feature = "voice")]
use crate::hardware::tiers::Tier;
#[cfg(feature = "voice")]
use crate::hardware::{Backend, HardwareInfo};
use crate::platform::ModelPresets;

#[cfg(feature = "voice")]
fn hw(ram: u64) -> HardwareInfo {
    HardwareInfo {
        backend: Backend::Cpu,
        total_ram_bytes: ram,
        available_ram_bytes: ram,
        vram_bytes: None,
    }
}

#[test]
fn catalog_contains_only_the_pinned_parakeet_entry() {
    let entries = entries().expect("catalog should parse");
    assert_eq!(entries.len(), 1);
    let entry = get(DEFAULT_MODEL_ID)
        .expect("catalog should parse")
        .expect("Parakeet entry");
    assert_eq!(entry.engine, SttEngine::SherpaParakeet);
    assert_eq!(entry.hf_revision.len(), 40);
    assert!(entry.hf_revision.chars().all(|c| c.is_ascii_hexdigit()));
    assert_eq!(entry.license, "CC-BY-4.0");
    assert_eq!(entry.languages.len(), 25);
    assert!(entry.languages.iter().any(|language| language == "de"));
}

#[test]
fn catalog_serializes_with_frontend_field_names() {
    let entry = get(DEFAULT_MODEL_ID)
        .expect("catalog parses")
        .expect("Parakeet entry");
    let value = serde_json::to_value(entry).expect("serializes");
    assert_eq!(value["engine"], "sherpa-parakeet");
    assert_eq!(value["hfRepo"], entry.hf_repo);
    assert!(value.get("hf_repo").is_none());
}

#[test]
#[cfg(feature = "voice")]
fn parakeet_is_default_for_desktop_and_phone_presets() {
    for presets in [ModelPresets::Desktop, ModelPresets::Phone] {
        assert_eq!(
            default_entry(presets).expect("Parakeet enabled").id,
            DEFAULT_MODEL_ID
        );
    }
}

#[test]
#[cfg(feature = "voice")]
fn recommendations_use_parakeet_for_every_tier_when_it_is_the_only_engine() {
    let tiers = recommend_tiers(
        &hw(16 * 1024 * 1024 * 1024),
        ModelPresets::Desktop,
        &[SttEngine::SherpaParakeet],
    )
    .expect("catalog parses")
    .expect("recommendations exist");
    assert!(tiers.iter().all(|tier| tier.entry.id == DEFAULT_MODEL_ID));
    assert_eq!(
        tiers.map(|tier| tier.tier),
        [Tier::Easy, Tier::Sweet, Tier::Max]
    );
}

#[test]
#[cfg(not(feature = "voice"))]
fn default_is_unavailable_when_voice_is_not_compiled() {
    assert!(default_entry(ModelPresets::Desktop).is_err());
}

#[test]
fn get_returns_none_for_removed_whisper_ids() {
    for id in ["whisper-tiny", "whisper-base", "whisper-small"] {
        assert!(get(id).expect("catalog parses").is_none());
    }
}
