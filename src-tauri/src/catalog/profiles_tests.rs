use super::profiles;
use crate::catalog::{get, recommend_tiers, Tier};
use crate::hardware::{Backend, HardwareInfo};
use crate::platform::ModelPresets;

const GIB: u64 = 1024 * 1024 * 1024;

fn cpu(available: u64) -> HardwareInfo {
    HardwareInfo {
        backend: Backend::Cpu,
        total_ram_bytes: available,
        available_ram_bytes: available,
        vram_bytes: None,
    }
}

fn suggestion(available: u64, presets: ModelPresets) -> String {
    let tiers = recommend_tiers(&cpu(available), presets).expect("non-empty catalog");
    let sweet = tiers
        .iter()
        .find(|rec| rec.tier == Tier::Sweet)
        .expect("a sweet tier");
    sweet.entry.id.clone()
}

#[test]
fn every_profile_names_a_catalog_model() {
    let profiles = profiles();
    for id in [&profiles.mobile, &profiles.mobile_low_memory] {
        assert!(get(id).is_some(), "{id} is not in the catalog");
    }
    // Qwen3.5 4B is offered whenever llama.cpp is enabled and the processor supports this build.
    #[cfg(feature = "llm-llamacpp")]
    let llama_cpp_available = crate::llm::llamacpp::cpu::missing_features().is_empty();
    #[cfg(not(feature = "llm-llamacpp"))]
    let llama_cpp_available = true;
    assert_eq!(
        get(&profiles.desktop).is_some(),
        llama_cpp_available,
        "{}",
        profiles.desktop
    );
    assert!(
        crate::catalog::all_entries()
            .iter()
            .any(|e| e.id == profiles.desktop),
        "{} is not in the catalog",
        profiles.desktop
    );
}

#[test]
fn a_phone_chooses_only_between_the_two_phone_profiles() {
    let profiles = profiles();
    // Enough memory for every model of the catalog, the desktop one too.
    let tiers = recommend_tiers(&cpu(16 * GIB), ModelPresets::Phone).expect("non-empty catalog");
    for rec in &tiers {
        assert!(
            rec.entry.id == profiles.mobile || rec.entry.id == profiles.mobile_low_memory,
            "{} is no phone profile",
            rec.entry.id
        );
    }
}

#[test]
fn a_phone_with_room_gets_the_phone_profile() {
    assert_eq!(suggestion(6 * GIB, ModelPresets::Phone), profiles().mobile);
}

#[test]
fn a_phone_with_little_memory_gets_the_low_memory_profile() {
    assert_eq!(
        suggestion(3 * GIB / 2, ModelPresets::Phone),
        profiles().mobile_low_memory
    );
}

#[test]
fn a_desktop_still_chooses_from_the_whole_catalog() {
    // The suggestion on a roomy desktop is the largest model of the build's catalog that is not only there
    // to try. With llama.cpp that is the desktop profile, Qwen3.5 4B (spec 070 US5). A build without local
    // inference filters nothing (it runs no model at all), so it lists Qwen3.5 as well.
    let largest = crate::catalog::entries()
        .iter()
        .filter(|e| !e.trial)
        .max_by_key(|e| (e.approx_size_bytes, e.id.as_str()))
        .expect("a catalog")
        .id
        .clone();
    assert_eq!(suggestion(64 * GIB, ModelPresets::Desktop), largest);
    #[cfg(feature = "llm-llamacpp")]
    if crate::llm::llamacpp::cpu::missing_features().is_empty() {
        assert_eq!(largest, profiles().desktop);
    }
}

#[test]
fn onboarding_never_suggests_a_model_that_is_only_there_to_try() {
    let trial: Vec<&str> = crate::catalog::entries()
        .iter()
        .filter(|e| e.trial)
        .map(|e| e.id.as_str())
        .collect();
    for available in [2 * GIB, 8 * GIB, 64 * GIB] {
        for presets in [ModelPresets::Desktop, ModelPresets::Phone] {
            let tiers = recommend_tiers(&cpu(available), presets).expect("non-empty catalog");
            for rec in &tiers {
                assert!(
                    !trial.contains(&rec.entry.id.as_str()),
                    "{} suggested",
                    rec.entry.id
                );
            }
        }
    }
}
