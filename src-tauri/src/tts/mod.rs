//! Speech output (spec 066, research R8): the catalog of local speech models, their download, and
//! synthesis into the assistant's mixer. Catalog, files and commands compile in every build so a
//! model can be listed without the `voice` feature; synthesis needs sherpa-onnx and is behind it.

pub mod catalog;
#[cfg(test)]
mod catalog_tests;
pub mod commands;
pub mod files;
#[cfg(test)]
mod files_tests;
#[cfg(feature = "voice")]
pub mod local;
#[cfg(all(test, feature = "voice"))]
mod local_tests;

/// The six languages haex speaks and parses (research R22).
pub const LANGUAGES: [&str; 6] = ["de", "en", "fr", "es", "it", "pt"];

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
}

impl From<TtsError> for crate::error::HolziError {
    fn from(err: TtsError) -> Self {
        crate::error::HolziError::SpeechFailed {
            reason: err.to_string(),
        }
    }
}
