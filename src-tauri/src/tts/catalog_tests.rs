//! Spec 066 T015: the built-in speech catalog.

use super::catalog::{default_for, entries, entries_with_fit, get, TtsEngine, TtsVariant};
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
fn every_entry_is_pinned_licensed_and_credited() {
    let all = entries().expect("catalog parses");
    assert!(!all.is_empty());
    for entry in all {
        assert_eq!(entry.hf_revision.len(), 40, "{}", entry.id);
        assert!(
            entry.hf_revision.chars().all(|c| c.is_ascii_hexdigit()),
            "{}",
            entry.id
        );
        assert!(LANGUAGES.contains(&entry.language.as_str()), "{}", entry.id);
        assert_eq!(entry.license, "CC-BY-4.0", "{}", entry.id);
        assert!(entry.attribution.contains("Kyutai"), "{}", entry.id);
        assert!(!entry.hf_folder.is_empty(), "{}", entry.id);
        assert!(entry.approx_size_bytes > 0, "{}", entry.id);
        assert_eq!(entry.engine, TtsEngine::Pocket);
        assert_eq!(entry.sample_rate, 24_000);
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
        assert_eq!(value["engine"], "pocket");
    }
}

/// One standard model per language; French exists only as the 24-layer variant, which is then
/// its default.
#[test]
fn every_language_has_exactly_one_default() {
    for language in LANGUAGES {
        let of_language: Vec<_> = entries()
            .expect("catalog parses")
            .iter()
            .filter(|e| e.language == language)
            .collect();
        let standard = of_language
            .iter()
            .filter(|e| e.variant == TtsVariant::Standard)
            .count();
        assert!(standard <= 1, "{language} has {standard} standard models");
        let default = default_for(language)
            .expect("catalog parses")
            .unwrap_or_else(|| panic!("{language} has a default"));
        if standard == 0 {
            assert_eq!(of_language.len(), 1, "{language}");
        } else {
            assert_eq!(default.variant, TtsVariant::Standard, "{language}");
        }
    }
    assert_eq!(
        default_for("fr").expect("parses").map(|e| e.variant),
        Some(TtsVariant::Layers24)
    );
    assert!(default_for("nl").expect("parses").is_none());
}

#[test]
fn get_reports_unknown_ids() {
    assert_eq!(get("pocket-de").expect("known").language, "de");
    assert!(get("pocket-xx").is_err());
}

#[test]
fn fit_and_default_are_annotated() {
    let roomy = entries_with_fit(&hw(32 * 1024 * 1024 * 1024)).expect("annotates");
    assert_eq!(roomy.len(), entries().expect("parses").len());
    assert!(roomy.iter().all(|e| e.fit == Fit::Fits));
    assert_eq!(
        roomy.iter().filter(|e| e.is_default).count(),
        LANGUAGES.len()
    );

    let tiny = entries_with_fit(&hw(64 * 1024 * 1024)).expect("annotates");
    assert!(tiny.iter().all(|e| e.fit == Fit::TooBig));

    let wire = serde_json::to_value(&roomy[0]).expect("serializes");
    assert!(wire.get("isDefault").is_some() && wire.get("hfRevision").is_some());
}
