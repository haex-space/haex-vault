//! Built-in catalog of local speech models (spec 066, research R8; spec 081): one Pocket TTS model
//! per language, for French only the larger 24-layer variant, and Qwen3-TTS for all languages at
//! once. Same shape and rules as `stt/catalog.rs`: every `hf_revision` is a pinned commit, never
//! `"main"`.

use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

use crate::hardware::{classify, Fit, HardwareInfo, ModelFitInputs};

use super::TtsError;

const TTS_CATALOG_JSON: &str = include_str!("tts_catalog.json");

/// The engine that speaks with an entry's files.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum TtsEngine {
    /// Kyutai Pocket TTS through sherpa-onnx (spec 066).
    Pocket,
    /// Qwen3-TTS through llama.cpp's audio generation helper (spec 081).
    #[serde(rename = "qwen3-tts")]
    Qwen3Tts,
}

/// Which export of a model an entry is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
pub enum TtsVariant {
    #[serde(rename = "standard")]
    Standard,
    #[serde(rename = "24l")]
    Layers24,
}

/// One file of an entry whose engine has no fixed file list, pinned by size and SHA-256 (spec
/// 081 FR-003).
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CatalogFile {
    pub name: String,
    pub bytes: u64,
    pub sha256: String,
}

/// One downloadable speech model (spec 081 data-model.md "Speech model entry").
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all(serialize = "camelCase", deserialize = "snake_case"))]
pub struct TtsCatalogEntry {
    pub id: String,
    pub name: String,
    pub hf_repo: String,
    pub hf_revision: String,
    /// Folder inside the repository; empty for the repository root.
    pub hf_folder: String,
    pub approx_size_bytes: u64,
    pub license: String,
    pub engine: TtsEngine,
    /// The languages of `tts::LANGUAGES` the model speaks.
    pub languages: Vec<String>,
    pub variant: TtsVariant,
    pub sample_rate: u32,
    pub attribution: String,
    /// Higher sounds better; the automatic choice prefers it (spec 081 FR-012).
    pub quality_rank: u32,
    /// The files of an engine without a fixed list (Qwen3-TTS), with their hashes.
    #[serde(default)]
    pub files: Option<Vec<CatalogFile>>,
    /// Pocket TTS sampling temperature (spec 081 FR-015).
    #[serde(default)]
    pub temperature: Option<String>,
}

impl TtsCatalogEntry {
    /// Whether the model speaks `language`.
    pub fn speaks(&self, language: &str) -> bool {
        self.languages.iter().any(|l| l == language)
    }
}

#[derive(Debug, Clone, Deserialize)]
struct RawTtsCatalog {
    models: Vec<TtsCatalogEntry>,
}

static TTS_CATALOG: OnceLock<Result<Vec<TtsCatalogEntry>, String>> = OnceLock::new();

/// Every entry of the built-in catalog.
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

/// The entry `id`.
pub fn get(id: &str) -> Result<&'static TtsCatalogEntry, TtsError> {
    entries()?
        .iter()
        .find(|e| e.id == id)
        .ok_or_else(|| TtsError::UnknownModel { id: id.to_owned() })
}

/// Every entry that speaks `language`, best `quality_rank` first.
pub fn for_language(language: &str) -> Result<Vec<&'static TtsCatalogEntry>, TtsError> {
    let mut found: Vec<_> = entries()?.iter().filter(|e| e.speaks(language)).collect();
    found.sort_by_key(|e| std::cmp::Reverse(e.quality_rank));
    Ok(found)
}

/// The Pocket TTS entry of `language`: the standard export, else the only one there is. It speaks
/// when nothing else was chosen or the chosen model cannot (spec 081 FR-014).
pub fn pocket_for(language: &str) -> Result<Option<&'static TtsCatalogEntry>, TtsError> {
    let of_language = || {
        entries().map(|all| {
            all.iter()
                .filter(|e| e.engine == TtsEngine::Pocket && e.speaks(language))
        })
    };
    Ok(of_language()?
        .find(|e| e.variant == TtsVariant::Standard)
        .or(of_language()?.next()))
}

/// An entry with its fit for this machine.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TtsCatalogEntryWithFit {
    #[serde(flatten)]
    pub entry: TtsCatalogEntry,
    pub fit: Fit,
}

/// Every entry with its fit for `hw`.
pub fn entries_with_fit(hw: &HardwareInfo) -> Result<Vec<TtsCatalogEntryWithFit>, TtsError> {
    Ok(entries()?
        .iter()
        .map(|entry| TtsCatalogEntryWithFit {
            entry: entry.clone(),
            fit: fit(entry, hw),
        })
        .collect())
}

/// Whether `entry` fits this machine, judged like chat and speech recognition models.
pub fn fit(entry: &TtsCatalogEntry, hw: &HardwareInfo) -> Fit {
    classify(
        hw,
        ModelFitInputs {
            file_size_bytes: entry.approx_size_bytes,
            // No context window; see `stt/catalog.rs`.
            context_window: None,
        },
    )
}
