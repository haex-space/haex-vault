//! Built-in catalog of local speech models (spec 066, research R8): one Pocket TTS model per
//! language, for French only the larger 24-layer variant. Same shape and rules as
//! `stt/catalog.rs`: every `hf_revision` is a pinned commit, never `"main"`.

use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

use crate::hardware::{classify, Fit, HardwareInfo, ModelFitInputs};

use super::TtsError;

const TTS_CATALOG_JSON: &str = include_str!("tts_catalog.json");

/// Which engine speaks an entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum TtsEngine {
    /// Kyutai Pocket TTS through sherpa-onnx.
    Pocket,
}

/// Size class of a model of one language.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
pub enum TtsVariant {
    #[serde(rename = "standard")]
    Standard,
    /// 24 layers instead of 6: better, about twice as slow.
    #[serde(rename = "24l")]
    Layers24,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all(serialize = "camelCase", deserialize = "snake_case"))]
pub struct TtsCatalogEntry {
    /// Stable id, also the `models::paths` slug the files live under.
    pub id: String,
    pub name: String,
    pub hf_repo: String,
    /// Pinned commit SHA — never `"main"`.
    pub hf_revision: String,
    /// Folder of the repo that holds this entry's files.
    pub hf_folder: String,
    /// Sum of all files the engine needs (`files::files_for`).
    pub approx_size_bytes: u64,
    pub license: String,
    pub engine: TtsEngine,
    /// ISO 639-1 code, one of [`super::LANGUAGES`].
    pub language: String,
    pub variant: TtsVariant,
    pub sample_rate: u32,
    /// Credit the licence asks for (CC BY), shown with the app's licences.
    pub attribution: String,
}

#[derive(Debug, Clone, Deserialize)]
struct RawTtsCatalog {
    models: Vec<TtsCatalogEntry>,
}

static TTS_CATALOG: OnceLock<Result<Vec<TtsCatalogEntry>, String>> = OnceLock::new();

pub fn entries() -> Result<&'static [TtsCatalogEntry], TtsError> {
    TTS_CATALOG
        .get_or_init(|| {
            serde_json::from_str::<RawTtsCatalog>(TTS_CATALOG_JSON)
                .map(|raw| raw.models)
                .map_err(|e| format!("parse built-in speech catalog: {e}"))
        })
        .as_ref()
        .map(Vec::as_slice)
        .map_err(|reason| TtsError::Failed {
            reason: reason.clone(),
        })
}

pub fn get(id: &str) -> Result<&'static TtsCatalogEntry, TtsError> {
    entries()?
        .iter()
        .find(|e| e.id == id)
        .ok_or_else(|| TtsError::UnknownModel { id: id.to_owned() })
}

/// The model a language speaks with until someone picks another: its standard variant, or the
/// only one there is (French).
pub fn default_for(language: &str) -> Result<Option<&'static TtsCatalogEntry>, TtsError> {
    let of_language = || entries().map(|all| all.iter().filter(|e| e.language == language));
    Ok(of_language()?
        .find(|e| e.variant == TtsVariant::Standard)
        .or(of_language()?.next()))
}

/// Catalog entry with the hardware fit and whether it is its language's default.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TtsCatalogEntryWithFit {
    #[serde(flatten)]
    pub entry: TtsCatalogEntry,
    pub fit: Fit,
    pub is_default: bool,
}

pub fn entries_with_fit(hw: &HardwareInfo) -> Result<Vec<TtsCatalogEntryWithFit>, TtsError> {
    entries()?
        .iter()
        .map(|entry| {
            let is_default =
                default_for(&entry.language)?.map(|d| d.id.as_str()) == Some(entry.id.as_str());
            Ok(TtsCatalogEntryWithFit {
                entry: entry.clone(),
                fit: classify(
                    hw,
                    ModelFitInputs {
                        file_size_bytes: entry.approx_size_bytes,
                        // No context window; see `stt/catalog.rs`.
                        context_window: None,
                    },
                ),
                is_default,
            })
        })
        .collect()
}
