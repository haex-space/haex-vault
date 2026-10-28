//! Built-in curated catalog of downloadable GGUF models.
//!
//! Per operator decision 2026-09-09: the catalog is a **suggestion**
//! list, not a gate. The user can also enter an arbitrary HuggingFace
//! `repo` + `filename` pair, or import a local GGUF. Model bytes are
//! NEVER shipped with the app; the catalog only carries metadata (repo
//! id, filename, approximate size, license) and the download itself
//! runs at pick time.
//!
//! The JSON blob is embedded at compile time via `include_str!` and
//! parsed once into a `OnceLock` so listing costs nothing after the
//! first call.

use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

use crate::hardware::{classify, Fit, HardwareInfo, ModelFitInputs};

pub use crate::hardware::tiers::Tier;

const CATALOG_JSON: &str = include_str!("model_catalog.json");

#[cfg(test)]
mod catalog_tests;
pub mod commands;
pub mod profiles;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CatalogEntry {
    /// Stable id used across `models.id` and
    /// `device_downloaded_models_no_sync.id`.
    pub id: String,
    pub name: String,
    pub family: String,
    pub parameters: String,
    pub quantization: String,
    pub hf_repo: String,
    pub hf_filename: String,
    pub tokenizer_repo: String,
    /// Approximate file size from the Hugging Face repo page. Rounded — the
    /// actual `Content-Length` from the HuggingFace CDN is authoritative
    /// and is captured on download into
    /// `device_downloaded_models_no_sync.size_bytes`.
    pub approx_size_bytes: u64,
    pub context_window: u64,
    pub license: String,
    /// Measured once for a curated model (spec 032); absent until the first evaluation run.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_use: Option<crate::model_capabilities::ToolSupport>,
}

#[derive(Debug, Clone, Deserialize)]
struct RawCatalog {
    models: Vec<CatalogEntry>,
}

static CATALOG: OnceLock<Vec<CatalogEntry>> = OnceLock::new();

/// Returns the curated list, cheap-copied per call.
pub fn entries() -> &'static [CatalogEntry] {
    CATALOG
        .get_or_init(|| {
            let raw: RawCatalog = serde_json::from_str(CATALOG_JSON)
                .expect("built-in catalog JSON must be valid at compile time");
            raw.models
        })
        .as_slice()
}

/// Fetches one entry by id, the e2e suite's stand-ins included (`models::stand_in`).
pub fn get(id: &str) -> Option<&'static CatalogEntry> {
    entries()
        .iter()
        .chain(crate::models::stand_in::entries())
        .find(|e| e.id == id)
}

/// Builds the HuggingFace resolve URL for a repo + filename. Uses the
/// `main` revision on purpose — GGUFs in bartowski's repos are
/// append-only per quantization, and pinning a revision requires
/// enumerating the LFS pointer, which we do not do in the MVP.
pub fn hf_resolve_url(repo: &str, filename: &str) -> String {
    format!("https://huggingface.co/{repo}/resolve/main/{filename}")
}

/// Catalog entry enriched with the hardware fit classification. Shown
/// on the onboarding suggestion list so the operator sees at a glance
/// whether a model matches this device.
#[derive(Debug, Clone, Serialize)]
pub struct CatalogEntryWithFit {
    #[serde(flatten)]
    pub entry: CatalogEntry,
    pub fit: crate::hardware::Fit,
}

/// Tauri command: list the built-in catalog, annotated with a fit
/// verdict against the current hardware. The hardware probe runs on the
/// blocking pool (`nvidia-smi` on CUDA hosts is capped by
/// [`crate::hardware::CUDA_PROBE_TIMEOUT`]).
#[tauri::command]
pub async fn list_catalog() -> Vec<CatalogEntryWithFit> {
    let hw = crate::hardware::probe_async().await;
    entries()
        .iter()
        .chain(crate::models::stand_in::entries())
        .cloned()
        .map(|entry| {
            let fit = classify(
                &hw,
                ModelFitInputs {
                    file_size_bytes: entry.approx_size_bytes,
                    context_window: Some(entry.context_window),
                },
            );
            CatalogEntryWithFit { entry, fit }
        })
        .collect()
}

/// One tier recommendation shown as a chip in the onboarding wizard.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TierRecommendation {
    pub tier: Tier,
    pub entry: CatalogEntry,
    pub fit: Fit,
}

/// Picks three tier-labelled model suggestions for the onboarding
/// wizard from the compiled catalog against the current hardware
/// snapshot.
///
/// Algorithm (see `contracts/tauri-commands.md` §catalog_recommend_tiers):
/// 1. Sort candidates ascending by `(approx_size_bytes, id)`.
/// 2. `Easy` = smallest `Fits`; fallback smallest `Tight`, then
///    `Unknown`, then `TooBig`, in that total order.
/// 3. `Sweet` = largest `Fits`; fallback: median candidate at
///    `(n - 1) / 2`.
/// 4. `Max` = largest `Fits` or `Tight`; fallback: the resolved
///    `Sweet`.
/// 5. Missing tiers reuse their fallback so the return is always three
///    recommendations for a non-empty catalog.
///
/// Under the phone presets the candidates are only the two phone profiles
/// (`profiles.rs`, spec 043 FR-027); the fit to the memory chooses between
/// them.
///
/// Returns `None` only when the catalog is empty (which never happens
/// in production because `entries()` reads the built-in JSON blob).
pub fn recommend_tiers(
    hw: &HardwareInfo,
    presets: crate::platform::ModelPresets,
) -> Option<[TierRecommendation; 3]> {
    let allowed = profiles::profiles().candidates(presets);
    let candidates: Vec<(CatalogEntry, Fit)> = entries()
        .iter()
        .filter(|entry| allowed.is_none_or(|ids| ids.contains(&entry.id.as_str())))
        .cloned()
        .map(|entry| {
            let fit = classify(
                hw,
                ModelFitInputs {
                    file_size_bytes: entry.approx_size_bytes,
                    context_window: Some(entry.context_window),
                },
            );
            (entry, fit)
        })
        .collect();
    let picked =
        crate::hardware::tiers::pick_three(candidates, |e| e.approx_size_bytes, |e| e.id.as_str())?;
    Some(picked.map(|(tier, entry, fit)| TierRecommendation { tier, entry, fit }))
}
