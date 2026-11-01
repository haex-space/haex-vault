//! Speech output (spec 066, research R8): the catalog of local speech models, their download, and
//! synthesis into the assistant's mixer. Catalog, files and commands compile in every build so a
//! model can be listed without the `voice` feature; synthesis needs sherpa-onnx and is behind it.

pub mod catalog;
#[cfg(test)]
mod catalog_tests;
pub mod commands;
pub mod credits;
#[cfg(test)]
mod credits_tests;
pub mod files;
#[cfg(test)]
mod files_tests;
#[cfg(feature = "voice")]
pub mod local;
#[cfg(all(test, feature = "voice"))]
mod local_tests;
#[cfg(feature = "voice")]
pub mod preview;
#[cfg(all(test, feature = "voice"))]
mod preview_tests;
pub mod voices;
#[cfg(test)]
mod voices_tests;

/// The six languages haex speaks and parses (research R22).
pub const LANGUAGES: [&str; 6] = ["de", "en", "fr", "es", "it", "pt"];

/// Range and default of the device preference `assistant.speechRate` (data-model.md).
pub const RATE_MIN: f32 = 0.75;
pub const RATE_MAX: f32 = 1.5;
pub const RATE_DEFAULT: f32 = 1.0;

/// `rate` if it is a speaking rate in [`RATE_MIN`]..=[`RATE_MAX`].
pub fn checked_rate(rate: f32) -> Result<f32, TtsError> {
    if (RATE_MIN..=RATE_MAX).contains(&rate) {
        Ok(rate)
    } else {
        Err(TtsError::Invalid {
            reason: format!("speaking rate {rate} is outside {RATE_MIN}–{RATE_MAX}"),
        })
    }
}

#[derive(Debug, thiserror::Error)]
pub enum TtsError {
    #[error("speech model {id} is not in the catalog")]
    UnknownModel { id: String },
    #[error("speech model {id} is not installed")]
    NotInstalled { id: String },
    #[error("speech output is unavailable: {reason}")]
    Unavailable { reason: String },
    #[error("speech output failed: {reason}")]
    Failed { reason: String },
    #[error("invalid speech request: {reason}")]
    Invalid { reason: String },
}

impl From<TtsError> for crate::error::HolziError {
    fn from(err: TtsError) -> Self {
        crate::error::HolziError::SpeechFailed {
            reason: err.to_string(),
        }
    }
}
