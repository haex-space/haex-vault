//! Built-in curated catalog of local speech-to-text model tiers (spec 010).
//!
//! Mirrors `catalog/mod.rs` (the LLM catalog) structurally, but for local
//! transcription models instead of GGUF chat models — no `hf_filename`/
//! `tokenizer_repo`/`context_window` fields, since those don't apply to the
//! multi-file load paths (see data-model.md). Each entry names its `engine`
//! (spec 066 T133); which files an engine needs is `files.rs`'s concern, not
//! a catalog attribute.
//!
//! Every `hf_revision` here is a pinned commit SHA, never `"main"` (see
//! research.md §6) — config, tokenizer, and weights must never drift apart
//! across an upstream repo's mutable default branch.

use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

use crate::hardware::tiers::{pick_three, Tier};
use crate::hardware::{classify, Fit, HardwareInfo, ModelFitInputs};
use crate::platform::ModelPresets;

const STT_CATALOG_JSON: &str = include_str!("stt_catalog.json");

/// Which local engine runs an entry (spec 066, research R7).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum SttEngine {
    /// candle-transformers Whisper (spec 008), behind `llm-cpu`.
    CandleWhisper,
    /// Parakeet TDT through sherpa-onnx (spec 066), behind `voice`.
    SherpaParakeet,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all(serialize = "camelCase", deserialize = "snake_case"))]
pub struct SttCatalogEntry {
    /// Stable id, also the `models::paths` slug this entry's files live
    /// under (e.g. `"whisper-tiny"`).
    pub id: String,
    pub name: String,
    pub hf_repo: String,
    /// Pinned commit SHA — never `"main"`.
    pub hf_revision: String,
    /// Sum of all files this entry's engine needs (`files::files_for`), not
    /// just the weights — the real on-disk/download footprint for the
    /// hardware-fit check.
    pub approx_size_bytes: u64,
    pub license: String,
    pub engine: SttEngine,
    /// Languages the model recognizes, as ISO 639-1 codes; empty when it is
    /// Whisper's open-ended list.
    #[serde(default)]
    pub languages: Vec<String>,
    /// Credit the licence asks for (CC BY), shown with the app's licences.
    #[serde(default)]
    pub attribution: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct RawSttCatalog {
    models: Vec<SttCatalogEntry>,
}

static STT_CATALOG: OnceLock<Result<Vec<SttCatalogEntry>, String>> = OnceLock::new();

/// Returns the curated list, cheap-copied per call.
pub fn entries() -> Result<&'static [SttCatalogEntry], crate::stt::SttError> {
    STT_CATALOG
        .get_or_init(|| {
            serde_json::from_str::<RawSttCatalog>(STT_CATALOG_JSON)
                .map(|raw| raw.models)
                .map_err(|e| format!("parse built-in STT catalog: {e}"))
        })
        .as_ref()
        .map(Vec::as_slice)
        .map_err(|reason| crate::stt::SttError::Failed {
            reason: reason.clone(),
        })
}

/// Fetches one entry by id.
pub fn get(id: &str) -> Result<Option<&'static SttCatalogEntry>, crate::stt::SttError> {
    Ok(entries()?.iter().find(|e| e.id == id))
}

/// Catalog entry enriched with the hardware fit classification. Shown on
/// the onboarding suggestion list and in Settings.
#[derive(Debug, Clone, Serialize)]
pub struct SttCatalogEntryWithFit {
    #[serde(flatten)]
    pub entry: SttCatalogEntry,
    pub fit: Fit,
}

/// One tier recommendation shown as a chip in the onboarding wizard's STT
/// step.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SttTierRecommendation {
    pub tier: Tier,
    pub entry: SttCatalogEntry,
    pub fit: Fit,
}

fn fit_for(hw: &HardwareInfo, entry: &SttCatalogEntry) -> Fit {
    classify(
        hw,
        ModelFitInputs {
            file_size_bytes: entry.approx_size_bytes,
            // No context-window concept for transcription; `fit.rs`
            // defaults this to a conservative 4096 tokens' worth of KV
            // cache, which is harmless here since Whisper tiers are tiny
            // relative to any realistic RAM/VRAM budget.
            context_window: None,
        },
    )
}

/// Lists every catalog entry annotated with a fit verdict against the given
/// hardware snapshot.
pub fn entries_with_fit(
    hw: &HardwareInfo,
) -> Result<Vec<SttCatalogEntryWithFit>, crate::stt::SttError> {
    Ok(entries()?
        .iter()
        .cloned()
        .map(|entry| {
            let fit = fit_for(hw, &entry);
            SttCatalogEntryWithFit { entry, fit }
        })
        .collect())
}

/// Picks three tier-labelled model suggestions for the onboarding wizard's
/// STT step, using the same generic algorithm the LLM catalog uses (see
/// `hardware::tiers::pick_three`). Under the phone presets the suggestion
/// (`Sweet`) is the smallest model, `whisper-tiny` (spec 043 FR-030); the
/// larger ones stay choosable as `Max` and in the settings. Returns `None`
/// only when the catalog is empty (never happens in production — the
/// built-in JSON always has three entries).
pub fn recommend_tiers(
    hw: &HardwareInfo,
    presets: ModelPresets,
) -> Result<Option<[SttTierRecommendation; 3]>, crate::stt::SttError> {
    let candidates: Vec<(SttCatalogEntry, Fit)> = entries()?
        .iter()
        .cloned()
        .map(|entry| {
            let fit = fit_for(hw, &entry);
            (entry, fit)
        })
        .collect();
    let smallest = candidates
        .iter()
        .min_by(|a, b| {
            a.0.approx_size_bytes
                .cmp(&b.0.approx_size_bytes)
                .then_with(|| a.0.id.cmp(&b.0.id))
        })
        .cloned();
    let Some(mut picked) = pick_three(candidates, |e| e.approx_size_bytes, |e| e.id.as_str())
    else {
        return Ok(None);
    };
    if let (ModelPresets::Phone, Some((entry, fit))) = (presets, smallest) {
        picked[1] = (Tier::Sweet, entry, fit);
    }
    Ok(Some(picked.map(|(tier, entry, fit)| {
        SttTierRecommendation { tier, entry, fit }
    })))
}
