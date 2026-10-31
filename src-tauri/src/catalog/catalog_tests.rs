//! Unit tests for the tier-selection algorithm. Uses the compiled-in
//! catalog against synthetic `HardwareInfo` scenarios.

use super::{entries, recommend_tiers, CatalogEntry, Tier};
use crate::hardware::{Backend, Fit, HardwareInfo};
use crate::platform::ModelPresets;

fn hw(backend: Backend, ram: u64, vram: Option<u64>) -> HardwareInfo {
    HardwareInfo {
        backend,
        total_ram_bytes: ram,
        available_ram_bytes: ram,
        vram_bytes: vram,
    }
}

#[test]
/// Roomy CUDA host: every catalog entry fits, so Easy is the smallest,
/// Sweet is the largest, and Max mirrors Sweet.
fn recommend_tiers_on_large_vram_host() {
    let info = hw(
        Backend::Cuda,
        64 * 1024 * 1024 * 1024,
        Some(24 * 1024 * 1024 * 1024),
    );
    let tiers = recommend_tiers(&info, ModelPresets::Desktop).expect("non-empty catalog");
    assert_eq!(tiers[0].tier, Tier::Easy);
    assert_eq!(tiers[1].tier, Tier::Sweet);
    assert_eq!(tiers[2].tier, Tier::Max);
    assert!(tiers[0].entry.approx_size_bytes <= tiers[1].entry.approx_size_bytes);
    assert!(tiers[1].entry.approx_size_bytes <= tiers[2].entry.approx_size_bytes);
    assert_eq!(tiers[0].fit, Fit::Fits);
    // On a big host, Sweet must also be Fits.
    assert_eq!(tiers[1].fit, Fit::Fits);
}

#[test]
/// CPU-only host with modest RAM: the tier algorithm never returns
/// fewer than three chips even when several entries are TooBig.
fn recommend_tiers_returns_three_even_when_most_are_too_big() {
    let info = hw(Backend::Cpu, 3 * 1024 * 1024 * 1024, None);
    let tiers = recommend_tiers(&info, ModelPresets::Desktop).expect("non-empty catalog");
    assert_eq!(tiers.len(), 3);
    assert_eq!(tiers[0].tier, Tier::Easy);
}

#[test]
/// CUDA host with no VRAM probe: every entry is Fit::Unknown, so Easy
/// falls back to the smallest Unknown; Sweet falls back to the median.
fn recommend_tiers_falls_back_when_hardware_unknown() {
    let info = hw(Backend::Cuda, 32 * 1024 * 1024 * 1024, None);
    let tiers = recommend_tiers(&info, ModelPresets::Desktop).expect("non-empty catalog");
    assert_eq!(tiers[0].fit, Fit::Unknown);
    // Easy is the smallest entry when nothing fits; the deterministic
    // sort by (size, id) puts it first.
    let mut sorted_ids: Vec<String> = crate::catalog::entries()
        .iter()
        .map(|e| e.id.clone())
        .collect();
    sorted_ids.sort_by(|a, b| {
        let ea = crate::catalog::get(a).unwrap();
        let eb = crate::catalog::get(b).unwrap();
        ea.approx_size_bytes
            .cmp(&eb.approx_size_bytes)
            .then_with(|| a.cmp(b))
    });
    assert_eq!(tiers[0].entry.id, sorted_ids[0]);
}

#[test]
fn catalog_contains_the_selected_qwen3_profiles() {
    let desktop = crate::catalog::get("qwen3-4b-instruct-q4_k_m").expect("desktop profile");
    assert_eq!(desktop.family, "Qwen 3");
    assert_eq!(desktop.parameters, "4B");
    assert_eq!(desktop.hf_filename, "Qwen_Qwen3-4B-Q4_K_M.gguf");

    let mobile = crate::catalog::get("qwen3-1.7b-instruct-q4_k_m").expect("mobile profile");
    assert_eq!(mobile.family, "Qwen 3");
    assert_eq!(mobile.parameters, "1.7B");

    let low_memory =
        crate::catalog::get("qwen3-0.6b-instruct-q4_k_m").expect("mobile low-memory profile");
    assert_eq!(low_memory.family, "Qwen 3");
    assert_eq!(low_memory.parameters, "0.6B");
}

#[test]
fn tool_use_is_optional_and_only_measured_entries_carry_it() {
    // Qwen3-4B was measured (spec 032 T046: 16 of 30 sentences, it calls the core tools) and so was
    // Qwen3.5-4B (spec 070: 60 of 83 sentences); the smaller models were not, so the self-test finds
    // out at their first load.
    const MEASURED: [&str; 2] = ["qwen3-4b-instruct-q4_k_m", "qwen3.5-4b-instruct-q4_k_m"];
    for entry in entries() {
        let expected = MEASURED
            .contains(&entry.id.as_str())
            .then_some(crate::model_capabilities::ToolSupport::Supported);
        assert_eq!(entry.tool_use, expected, "{}", entry.id);
    }
    let entry: CatalogEntry = serde_json::from_value(serde_json::json!({
        "id": "x", "name": "x", "family": "x", "parameters": "1B", "quantization": "Q4",
        "hf_repo": "a/b", "hf_filename": "c.gguf", "tokenizer_repo": "a/b",
        "approx_size_bytes": 1, "context_window": 1, "license": "MIT",
        "tool_use": "unsupported",
    }))
    .unwrap();
    assert_eq!(
        entry.tool_use,
        Some(crate::model_capabilities::ToolSupport::Unsupported)
    );
}

#[test]
fn a_local_record_takes_the_catalog_answer_as_curated_and_otherwise_starts_unknown() {
    use crate::model_capabilities::{ModelCapabilities, ToolSupport, ToolUse, ToolUseBasis};
    assert_eq!(
        ModelCapabilities::local("qwen3-4b-instruct-q4_k_m").tool_use,
        Some(ToolUse::new(ToolSupport::Supported, ToolUseBasis::Curated))
    );
    assert_eq!(
        ModelCapabilities::local("qwen3-1.7b-instruct-q4_k_m").tool_use,
        None
    );
    assert_eq!(ModelCapabilities::local("my-own-finetune").tool_use, None);
}

#[cfg(feature = "llm-cpu")]
mod engines {
    use super::super::{all_entries, entries};
    use crate::llm::engine::{engine_for, is_loadable, Engine};

    #[test]
    fn every_built_in_entry_names_its_architecture() {
        for entry in all_entries() {
            let architecture = entry.architecture.as_deref();
            assert!(
                architecture.is_some_and(|a| !a.is_empty()),
                "{} has no architecture",
                entry.id
            );
        }
    }

    /// Spec 070 FR-007: a model is offered only when an engine compiled into this build loads it.
    #[test]
    fn catalog_entries_name_a_loadable_architecture() {
        for entry in entries() {
            let architecture = entry.architecture.as_deref().expect("checked above");
            assert!(
                is_loadable(architecture),
                "{} ({architecture}) is loadable by no compiled engine",
                entry.id
            );
        }
    }

    #[test]
    fn a_build_hides_exactly_the_entries_it_cannot_load() {
        let hidden: Vec<String> = all_entries()
            .into_iter()
            .filter(|e| !is_loadable(e.architecture.as_deref().unwrap_or_default()))
            .map(|e| e.id)
            .collect();
        assert_eq!(all_entries().len() - hidden.len(), entries().len());
        for id in &hidden {
            assert!(entries().iter().all(|e| &e.id != id), "{id} is offered");
        }
        // Without llama.cpp the Qwen3.5 download is not offered; with it, it is.
        let qwen35_offered = entries()
            .iter()
            .any(|e| e.id == "qwen3.5-4b-instruct-q4_k_m");
        assert_eq!(qwen35_offered, cfg!(feature = "llm-llamacpp"));
    }

    /// The three profiles are the defaults of a fresh install: they must load without the optional
    /// engine, so a build without llama.cpp still has a working default (spec 070, until US5 is decided).
    #[test]
    fn the_profile_models_load_with_mistralrs() {
        let profiles = super::super::profiles::profiles();
        for id in [
            &profiles.desktop,
            &profiles.mobile,
            &profiles.mobile_low_memory,
        ] {
            let entry = crate::catalog::get(id).expect("profile model is in the catalog");
            let architecture = entry.architecture.as_deref().expect("has an architecture");
            assert_eq!(engine_for(architecture).unwrap(), Engine::Mistralrs, "{id}");
        }
    }
}
