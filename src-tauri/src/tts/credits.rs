//! Credits for the speech models and voices (spec 066 T032, FR-046 in spirit for the voices): what
//! the page "Lizenzen" lists besides the icon sets. CC BY asks for a credit wherever the work is
//! used; the built-in voices ship in the binary and the speech models are downloaded by haex
//! itself, so every one of them is listed, with the credit its catalog names.

use serde::Serialize;

use super::voices::{self, VoiceLicense};
use super::{catalog, TtsError};

/// What a credit is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum CreditKind {
    /// A built-in voice: a reference recording in the binary.
    Voice,
    /// A speech model the assistant speaks with.
    SpeechModel,
    /// A speech recognition model whose licence asks for a credit.
    RecognitionModel,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpeechCredit {
    pub kind: CreditKind,
    pub name: String,
    /// ISO 639-1 code for voices and speech models; absent for recognition models.
    pub language: Option<String>,
    /// SPDX identifier.
    pub license: String,
    pub attribution: String,
    /// Where the work comes from, a web address.
    pub source: String,
}

fn spdx(license: VoiceLicense) -> &'static str {
    match license {
        VoiceLicense::Cc0 => "CC0-1.0",
        VoiceLicense::CcBy4 => "CC-BY-4.0",
    }
}

/// Every built-in voice, every speech model and the recognition models with a credit, in that
/// order.
pub fn credits() -> Result<Vec<SpeechCredit>, TtsError> {
    let voices = voices::builtin()?.iter().map(|voice| SpeechCredit {
        kind: CreditKind::Voice,
        name: voice.name.clone(),
        language: Some(voice.language.clone()),
        license: spdx(voice.license).to_owned(),
        attribution: voice.attribution.clone(),
        source: voice.source_url.clone(),
    });
    let models = catalog::entries()?.iter().map(|entry| SpeechCredit {
        kind: CreditKind::SpeechModel,
        name: entry.name.clone(),
        language: Some(entry.language.clone()),
        license: entry.license.clone(),
        attribution: entry.attribution.clone(),
        source: format!(
            "https://huggingface.co/{}/tree/{}/{}",
            entry.hf_repo, entry.hf_revision, entry.hf_folder
        ),
    });
    let recognition = crate::stt::catalog::entries()
        .map_err(|e| TtsError::Failed {
            reason: format!("speech recognition catalog: {e}"),
        })?
        .iter()
        .filter_map(|entry| {
            let attribution = entry.attribution.clone()?;
            Some(SpeechCredit {
                kind: CreditKind::RecognitionModel,
                name: entry.name.clone(),
                language: None,
                license: entry.license.clone(),
                attribution,
                source: format!(
                    "https://huggingface.co/{}/tree/{}",
                    entry.hf_repo, entry.hf_revision
                ),
            })
        });
    Ok(voices.chain(models).chain(recognition).collect())
}

/// The credits for the page "Lizenzen".
#[tauri::command]
pub fn speech_credits() -> crate::error::Result<Vec<SpeechCredit>> {
    Ok(credits()?)
}
