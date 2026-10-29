//! Speech-to-text adapter contract (spec 008-voice-control-stt).
//!
//! Narrower than [`crate::adapters::ProviderAdapter`]: transcription is
//! "one utterance in, text out" — no streaming, no context window
//! (research.md §3). A `Provider` row can carry either capability
//! (`storage::providers::ProviderCapability`); this trait is what the
//! `transcription` side dispatches through, the way `ProviderAdapter` is
//! what `chat` dispatches through.

use async_trait::async_trait;

pub mod catalog;
#[cfg(test)]
mod catalog_tests;
pub mod commands;
#[cfg(test)]
mod commands_tests;
// Used by the Whisper tests (`llm-cpu`), the pause detection tests and the probe (`voice`).
#[cfg(any(
    all(test, any(feature = "llm-cpu", feature = "voice")),
    all(feature = "platform-probe", feature = "voice")
))]
pub(crate) mod fixture_wav;
pub mod interrupt;
#[cfg(test)]
mod interrupt_tests;
#[cfg(feature = "llm-cpu")]
pub mod local;
#[cfg(all(test, feature = "llm-cpu"))]
mod local_tests;

/// Every `SttAdapter` accepts exactly this format: 16 kHz, mono, normalized
/// `f32` samples in `[-1.0, 1.0]`. `audio::capture` converts each device's
/// native sample type, channel count, and sample rate at the recording
/// boundary — no adapter ever sees a native format. Backend-local only:
/// never crosses the Tauri IPC boundary or persistence (FR-020), and is
/// discarded once `stop_voice_recording` finishes, success or failure.
#[derive(Debug, Clone)]
pub struct CanonicalPcm {
    pub samples: Vec<f32>,
}

pub const SAMPLE_RATE_HZ: u32 = 16_000;

impl CanonicalPcm {
    pub fn duration_secs(&self) -> f64 {
        self.samples.len() as f64 / SAMPLE_RATE_HZ as f64
    }
}

/// STT-facing error taxonomy — the transcription analogue of
/// [`crate::adapters::AdapterError`], narrowed to what a transcription
/// backend can fail with. `stop_voice_recording` maps every variant to
/// `HolziError::TranscriptionFailed { reason }` (contracts/tauri-commands.md)
/// — the distinction between variants matters inside an adapter (e.g.
/// deciding whether a failure is credential-related) but not on the wire.
#[derive(Debug, thiserror::Error)]
pub enum SttError {
    /// An external service rejected the request as unauthenticated/
    /// forbidden (HTTP 401/403) — mirrors `AdapterError::InvalidCredentials`.
    /// Never produced by [`local::LocalWhisperAdapter`].
    #[error("invalid credentials")]
    InvalidCredentials,
    /// Transport-level failure reaching an external service (connection
    /// refused, timeout, DNS, ...) — mirrors `AdapterError::Http`.
    #[error("transport failure: {reason}")]
    Transport { reason: String },
    /// The local Whisper backend isn't compiled into this build (`voice`
    /// enabled without `llm-cpu`), or its bundled model failed to load.
    #[error("local transcription is unavailable: {reason}")]
    LocalUnavailable { reason: String },
    /// Catch-all for a backend-specific failure that doesn't fit the
    /// above (model inference error, malformed external response, ...).
    #[error("transcription failed: {reason}")]
    Failed { reason: String },
}

/// Narrower than [`crate::adapters::ProviderAdapter`]: transcription is
/// "one utterance in, text out" — no streaming, no context window
/// (research.md §3).
#[async_trait]
pub trait SttAdapter: Send + Sync {
    async fn transcribe(&self, audio: &CanonicalPcm) -> Result<String, SttError>;
}

impl From<SttError> for crate::error::HolziError {
    /// Every variant collapses to `TranscriptionFailed { reason }` on the
    /// wire (contracts/tauri-commands.md `stop_voice_recording`) — the
    /// distinction between variants is for adapter-internal logic only.
    fn from(err: SttError) -> Self {
        crate::error::HolziError::TranscriptionFailed {
            reason: err.to_string(),
        }
    }
}
