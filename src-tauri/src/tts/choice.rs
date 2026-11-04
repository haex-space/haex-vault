//! Which speech model speaks a language on this device (spec 081 FR-006, FR-012, FR-014): the one
//! the user chose, else the automatic proposal, and what actually speaks when the chosen model is
//! not there yet. Pure: the caller passes the preference, what is installed, the hardware and the
//! measured speeds, so the rules are testable without an app.

use super::catalog::{self, TtsCatalogEntry, TtsEngine};
use super::TtsError;

/// Free graphics memory below which Qwen3-TTS is not presumed to keep up before it was measured
/// (spec 081 FR-012).
pub const QWEN_PRESUMED_GPU_BYTES: u64 = 3 << 30;

/// What haex has measured about a model's speed on this device (`tts::speed`).
pub trait Speeds {
    /// `Some(true)` when the model keeps up where it would run now, `Some(false)` when it does not,
    /// `None` when there are not enough measurements yet.
    fn keeps_up(&self, model_id: &str) -> Option<bool>;
}

/// No measurements at all: every model is judged by the presumption of FR-012.
pub struct Unmeasured;

impl Speeds for Unmeasured {
    fn keeps_up(&self, _model_id: &str) -> Option<bool> {
        None
    }
}

/// The device facts the automatic choice needs.
#[derive(Debug, Clone, Copy, Default)]
pub struct Device {
    /// Free memory of a graphics card this build can run Qwen3-TTS on; `None` without one.
    pub gpu_free_bytes: Option<u64>,
}

/// The speech models of one language on this device.
#[derive(Debug, Clone, Copy)]
pub struct Resolved {
    /// The model the user chose for the language, if the choice is valid in this build.
    pub chosen: Option<&'static TtsCatalogEntry>,
    /// What "automatic" picks: the best-sounding model that keeps up (FR-012).
    pub proposed: &'static TtsCatalogEntry,
    /// What speaks now: the chosen model when it is installed, else the proposed one when it is
    /// installed, else the language's Pocket model (FR-014), which the caller may still have to
    /// download.
    pub speaking: &'static TtsCatalogEntry,
}

/// Resolves the speech models of `language`. `preference` is the device preference
/// `assistant.speechModel.<language>`; `enabled` says whether this build has an engine.
pub fn resolve(
    language: &str,
    preference: Option<&str>,
    installed: &[String],
    enabled: impl Fn(TtsEngine) -> bool,
    device: Device,
    speeds: &dyn Speeds,
) -> Result<Resolved, TtsError> {
    let pocket = catalog::pocket_for(language)?.ok_or_else(|| TtsError::Unavailable {
        reason: format!("no speech model speaks {language}"),
    })?;
    let candidates: Vec<_> = catalog::for_language(language)?
        .into_iter()
        .filter(|entry| enabled(entry.engine))
        .collect();
    let chosen = preference.and_then(|id| candidates.iter().copied().find(|e| e.id == id));
    let proposed = candidates
        .iter()
        .copied()
        .find(|entry| {
            speeds
                .keeps_up(&entry.id)
                .unwrap_or_else(|| presumed_to_keep_up(entry, device))
        })
        .unwrap_or(pocket);
    let is_installed = |entry: &TtsCatalogEntry| installed.contains(&entry.id);
    let speaking = match chosen {
        Some(entry) if is_installed(entry) => entry,
        Some(_) => pocket,
        None if is_installed(proposed) => proposed,
        None => pocket,
    };
    Ok(Resolved {
        chosen,
        proposed,
        speaking,
    })
}

/// Before any measurement: Pocket TTS keeps up everywhere it runs; Qwen3-TTS only on a graphics
/// card with enough free memory (spec 081 FR-012).
fn presumed_to_keep_up(entry: &TtsCatalogEntry, device: Device) -> bool {
    match entry.engine {
        TtsEngine::Pocket => true,
        TtsEngine::Qwen3Tts => device
            .gpu_free_bytes
            .is_some_and(|free| free >= QWEN_PRESUMED_GPU_BYTES),
    }
}
