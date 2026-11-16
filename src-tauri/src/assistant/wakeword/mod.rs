//! The default wake word "Haex" / "Hey Haex" (spoken "Hex" / "Hey Hex"; spec 066 FR-011, research
//! R4, T066). A custom wake word (US7) is meant to replace it per device; this is only the default.
//!
//! Two small models from `haex-space/haex-wakeword` (Apache-2.0, trained with the pipeline of the
//! same name), run in pure Rust through tract. They ship inside the binary (3 MB, [`update`]), so
//! the default wake word needs no download, and [`update`] replaces them with a newer published
//! version without a new app release:
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
pub mod update;

/// The device setting that picks the rule.
pub const PREF_SENSITIVITY: &str = "assistant.sensitivity";

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
