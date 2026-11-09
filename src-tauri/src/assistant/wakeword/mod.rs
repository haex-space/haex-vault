//! The default wake word "Hex" / "Hey Hex" (spec 066 FR-011, research R4, T066).
//!
//! Two small models from `haex-space/haex-wakeword` (Apache-2.0, trained with the pipeline of the
//! same name), run in pure Rust through tract:
//! - `speech_embedding.onnx`: Google's `speech_embedding/1` with its log-mel front end. One
//!   96-value embedding per 80 ms of 16 kHz audio.
//! - `hex.onnx`: the classifier. It gets the last 16 embeddings (1.28 s) and returns how sure it is
//!   that the wake word just ended.
//!
//! [`detector::Detector`] runs them over the stream, only while the pause detection hears speech
//! (R4: the load in "bereit" stays small), and fires by the rule of the device's sensitivity
//! (`assistant.sensitivity`): a number of consecutive windows at or above a threshold, then 2 s of
//! quiet. The rules and what they measured are in the model card; "medium" meets SC-004.

pub mod detector;
pub mod model;

use tauri::AppHandle;

use super::pinned::PinnedFile;
use crate::error::Result;

/// The device setting that picks the rule.
pub const PREF_SENSITIVITY: &str = "assistant.sensitivity";

const REPO: &str = "haex-space/haex-wakeword";
const REVISION: &str = "d0d2dc6225c3bb1103756ebaa15a4e1d16cd77b8";
const SLUG: &str = "wakeword-hex";

pub const EMBEDDING: PinnedFile = PinnedFile {
    slug: SLUG,
    repo: REPO,
    revision: REVISION,
    file: "speech_embedding.onnx",
    bytes: 2_183_273,
    sha256: "092fa584098a0c0e5c5382d028eed7e7e7d5e41ccf23786e7417248d0aabe8c6",
    label: "the wake word's speech model",
};

pub const CLASSIFIER: PinnedFile = PinnedFile {
    slug: SLUG,
    repo: REPO,
    revision: REVISION,
    file: "hex.onnx",
    bytes: 857_934,
    sha256: "c67e7a3686741c9ff2ea14de714611b54c64e7ddae3eb4522015e02c0e9ec70a",
    label: "the wake word model",
};

/// Both files, as one download.
pub const TOTAL_BYTES: u64 = EMBEDDING.bytes + CLASSIFIER.bytes;

/// Whether both files are downloaded and checked.
pub fn is_installed(app: &AppHandle) -> bool {
    EMBEDDING.is_installed(app) && CLASSIFIER.is_installed(app)
}

/// Downloads what is missing; progress as one `model-download-progress` with `modelId`
/// "wakeword-hex".
pub async fn download(app: &AppHandle) -> Result<()> {
    EMBEDDING.download_with(app, SLUG, 0, TOTAL_BYTES).await?;
    CLASSIFIER
        .download_with(app, SLUG, EMBEDDING.bytes, TOTAL_BYTES)
        .await
}

/// When the detector fires: `windows` consecutive scores at or above `threshold`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rule {
    pub windows: usize,
    pub threshold: f32,
}

/// `assistant.sensitivity` (data-model.md: `"low" | "medium" | "high"`, default `"medium"`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Sensitivity {
    Low,
    #[default]
    Medium,
    High,
}

impl Sensitivity {
    /// The stored value; anything unknown is the default.
    pub fn parse(value: Option<&str>) -> Self {
        match value.map(str::trim) {
            Some("low") => Self::Low,
            Some("high") => Self::High,
            _ => Self::Medium,
        }
    }

    /// The rules of the model card (measured on 37 h of held-out speech, run7 2026-11-09):
    ///
    /// | | "Hey Hex" | "Hex" alone | false accepts per hour |
    /// |---|---|---|---|
    /// | low | 88.7 % | 47.1 % | 0.00 |
    /// | medium | 90.5 % | 49.6 % | 0.05 |
    /// | high | 94.2 % | 72.3 % | 0.48 |
    pub fn rule(self) -> Rule {
        match self {
            Self::Low => Rule {
                windows: 3,
                threshold: 0.9,
            },
            Self::Medium => Rule {
                windows: 2,
                threshold: 0.98,
            },
            Self::High => Rule {
                windows: 2,
                threshold: 0.5,
            },
        }
    }
}

/// Whether the wake word models are there (`{installed, sizeBytes}`).
#[tauri::command]
pub async fn assistant_wake_word_model(app: AppHandle) -> Result<serde_json::Value> {
    Ok(serde_json::json!({ "installed": is_installed(&app), "sizeBytes": TOTAL_BYTES }))
}

/// Downloads the wake word models; progress as the shared `model-download-progress` (`modelId`
/// "wakeword-hex"). The assistant uses them from the next time it is switched on.
#[tauri::command]
pub async fn assistant_wake_word_model_download(
    app: AppHandle,
    state: tauri::State<'_, crate::state::AppState>,
) -> Result<()> {
    state.gate().run(download(&app)).await?
}
