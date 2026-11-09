//! Which speech model speaks a language on this device (spec 081 FR-006, FR-012, FR-014): the one
//! the user chose, else the default, and what actually speaks while the chosen or default model is
//! not downloaded yet. Pure: the caller passes the preference and what is installed, so the rules
//! are testable without an app.

use super::catalog::{self, TtsCatalogEntry, TtsEngine};
use super::TtsError;

/// The speech models of one language on this device.
#[derive(Debug, Clone, Copy)]
pub struct Resolved {
    /// The model the user chose for the language, if the choice is valid in this build.
    pub chosen: Option<&'static TtsCatalogEntry>,
    /// What speaks without a choice: the best-sounding model this build can run, which is
    /// Qwen3-TTS wherever it runs (operator 2026-11-05, FR-012).
    pub default: &'static TtsCatalogEntry,
    /// What speaks now: the chosen (else the default) model when it is installed, else the
    /// language's Pocket model (FR-014), which the caller may still have to download.
    pub speaking: &'static TtsCatalogEntry,
}

impl Resolved {
    /// The model that should be installed and speak: the chosen one, else the default.
    pub fn wanted(&self) -> &'static TtsCatalogEntry {
        self.chosen.unwrap_or(self.default)
    }
}

/// Resolves the speech models of `language`. `preference` is the device preference
/// `assistant.speechModel.<language>`; `enabled` says whether this build has an engine, and
/// `may_default` whether that engine may speak without a choice on this platform. Qwen3-TTS may
/// not on Android: it is far slower than real time on a phone's processor and needs 2.3 GB of
/// memory beside the chat model (operator 2026-11-05); a user may still choose it there.
pub fn resolve(
    language: &str,
    preference: Option<&str>,
    installed: &[String],
    enabled: impl Fn(TtsEngine) -> bool,
    may_default: impl Fn(TtsEngine) -> bool,
) -> Result<Resolved, TtsError> {
    let pocket = catalog::pocket_for(language)?.ok_or_else(|| TtsError::Unavailable {
        reason: format!("no speech model speaks {language}"),
    })?;
    let candidates: Vec<_> = catalog::for_language(language)?
        .into_iter()
        .filter(|entry| enabled(entry.engine))
        .collect();
    let chosen = preference.and_then(|id| candidates.iter().copied().find(|e| e.id == id));
    // `for_language` lists the best-sounding first.
    let default = candidates
        .iter()
        .copied()
        .find(|entry| may_default(entry.engine))
        .unwrap_or(pocket);
    let wanted = chosen.unwrap_or(default);
    let speaking = if installed.contains(&wanted.id) {
        wanted
    } else {
        pocket
    };
    Ok(Resolved {
        chosen,
        default,
        speaking,
    })
}
