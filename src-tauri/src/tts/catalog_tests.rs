//! Spec 066 T015: the built-in speech catalog.

use super::catalog::{
    entries, entries_with_fit, for_language, get, pocket_for, TtsEngine, TtsVariant,
};
use super::LANGUAGES;
use crate::hardware::{Backend, Fit, HardwareInfo};

fn hw(ram: u64) -> HardwareInfo {
    HardwareInfo {
        backend: Backend::Cpu,
        total_ram_bytes: ram,
        available_ram_bytes: ram,
        vram_bytes: None,
    }
}

#[test]
fn every_entry_is_pinned_and_speaks_known_languages() {
    let all = entries().expect("catalog parses");
    assert!(!all.is_empty());
    for entry in all {
        assert_eq!(entry.hf_revision.len(), 40, "{}", entry.id);
        assert!(
            entry.hf_revision.chars().all(|c| c.is_ascii_hexdigit()),
            "{}",
            entry.id
        );
        assert!(!entry.languages.is_empty(), "{}", entry.id);
        for language in &entry.languages {
            assert!(LANGUAGES.contains(&language.as_str()), "{}", entry.id);
        }
        assert!(entry.approx_size_bytes > 0, "{}", entry.id);
        assert!(entry.quality_rank > 0, "{}", entry.id);
        assert_eq!(entry.sample_rate, 24_000);
        if let Some(files) = &entry.files {
            assert_eq!(
                files.iter().map(|f| f.bytes).sum::<u64>(),
                entry.approx_size_bytes,
                "{}",
                entry.id
            );
            for file in files {
                assert_eq!(file.sha256.len(), 64, "{}", file.name);
                assert!(file.sha256.chars().all(|c| c.is_ascii_hexdigit()));
            }
        }
    }
}

/// Pocket TTS models: Kyutai's CC BY weights, one folder per language, sampled with a temperature.
#[test]
fn pocket_entries_are_credited_and_have_a_temperature() {
    for entry in entries()
        .expect("catalog parses")
        .iter()
        .filter(|e| e.engine == TtsEngine::Pocket)
    {
        assert_eq!(entry.license, "CC-BY-4.0", "{}", entry.id);
        assert!(entry.attribution.contains("Kyutai"), "{}", entry.id);
        assert!(!entry.hf_folder.is_empty(), "{}", entry.id);
        assert_eq!(entry.languages.len(), 1, "{}", entry.id);
        assert!(entry.temperature.is_some(), "{}", entry.id);
        assert!(entry.files.is_none(), "{}", entry.id);
    }
}

#[test]
fn ids_are_unique() {
    let all = entries().expect("catalog parses");
    for (i, entry) in all.iter().enumerate() {
        assert!(
            all[i + 1..].iter().all(|other| other.id != entry.id),
            "{}",
            entry.id
        );
    }
}

#[test]
fn variants_use_the_wire_names() {
    for entry in entries().expect("catalog parses") {
        let value = serde_json::to_value(entry).expect("serializes");
        let variant = value["variant"].as_str().expect("variant is a string");
        assert!(["standard", "24l"].contains(&variant), "{variant}");
        let engine = value["engine"].as_str().expect("engine is a string");
        assert!(["pocket", "qwen3-tts"].contains(&engine), "{engine}");
    }
}

/// One standard model per language; French exists only as the 24-layer variant, which is then
/// its default.
#[test]
fn every_language_has_exactly_one_pocket_model() {
    for language in LANGUAGES {
        let of_language: Vec<_> = entries()
            .expect("catalog parses")
            .iter()
            .filter(|e| e.engine == TtsEngine::Pocket && e.speaks(language))
            .collect();
        let standard = of_language
            .iter()
            .filter(|e| e.variant == TtsVariant::Standard)
            .count();
        assert!(standard <= 1, "{language} has {standard} standard models");
        let default = pocket_for(language)
            .expect("catalog parses")
            .unwrap_or_else(|| panic!("{language} has a default"));
        if standard == 0 {
            assert_eq!(of_language.len(), 1, "{language}");
        } else {
            assert_eq!(default.variant, TtsVariant::Standard, "{language}");
        }
    }
    assert_eq!(
        pocket_for("fr").expect("parses").map(|e| e.variant),
        Some(TtsVariant::Layers24)
    );
    assert!(pocket_for("nl").expect("parses").is_none());
}

#[test]
fn get_reports_unknown_ids() {
    assert!(get("pocket-de").expect("known").speaks("de"));
    assert!(get("pocket-xx").is_err());
}

#[test]
fn fit_is_annotated() {
    let roomy = entries_with_fit(&hw(32 * 1024 * 1024 * 1024)).expect("annotates");
    assert_eq!(roomy.len(), entries().expect("parses").len());
    assert!(roomy.iter().all(|e| e.fit == Fit::Fits));

    let tiny = entries_with_fit(&hw(64 * 1024 * 1024)).expect("annotates");
    assert!(tiny.iter().all(|e| e.fit == Fit::TooBig));

    let wire = serde_json::to_value(&roomy[0]).expect("serializes");
    assert!(wire.get("languages").is_some() && wire.get("hfRevision").is_some());
    assert!(wire.get("qualityRank").is_some());
}

#[test]
fn for_language_lists_the_best_sounding_first() {
    for language in LANGUAGES {
        let found = for_language(language).expect("parses");
        assert!(!found.is_empty(), "{language}");
        assert!(
            found
                .windows(2)
                .all(|pair| pair[0].quality_rank >= pair[1].quality_rank),
            "{language}"
        );
    }
    assert!(for_language("nl").expect("parses").is_empty());
}
