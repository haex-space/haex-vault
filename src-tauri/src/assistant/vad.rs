//! Pause detection (spec 066, research R6): Silero VAD through sherpa-onnx cuts the 16 kHz mono
//! microphone stream into utterances.

use std::path::Path;

use sherpa_onnx::{SileroVadModelConfig, VadModelConfig, VoiceActivityDetector};

use crate::stt::SAMPLE_RATE_HZ;

/// Shorter sounds are no utterance (research R6).
pub const MIN_SPEECH_SECS: f32 = 0.25;
/// One utterance is cut after this long, as a recording in spec 008.
pub const MAX_SPEECH_SECS: f32 = 60.0;
/// Range of the device preference `assistant.pauseMs` (data-model.md).
pub const PAUSE_MS_MIN: u32 = 500;
pub const PAUSE_MS_MAX: u32 = 2000;

const THRESHOLD: f32 = 0.5;
/// Silero's window at 16 kHz.
const WINDOW_SAMPLES: i32 = 512;

/// One utterance: where it started in the stream (samples since the detector was created or last
/// finished) and its samples.
#[derive(Debug, Clone, PartialEq)]
pub struct Utterance {
    pub start_sample: u64,
    pub samples: Vec<f32>,
}

#[derive(Debug, thiserror::Error)]
pub enum VadError {
    #[error("the pause detection model is missing")]
    ModelMissing,
    #[error("the pause detection model could not be loaded")]
    ModelLoad,
}

pub struct PauseDetector {
    vad: VoiceActivityDetector,
}

impl PauseDetector {
    /// `pause_ms` is clamped to the range of `assistant.pauseMs`.
    pub fn new(model: &Path, pause_ms: u32) -> Result<Self, VadError> {
        // sherpa-onnx gets the path as text; check it here so a missing or non-UTF-8 path is an
        // error of ours and never reaches the native loader.
        let model = model
            .is_file()
            .then(|| model.to_str())
            .flatten()
            .ok_or(VadError::ModelMissing)?;
        let pause_ms = pause_ms.clamp(PAUSE_MS_MIN, PAUSE_MS_MAX);
        let config = VadModelConfig {
            silero_vad: SileroVadModelConfig {
                model: Some(model.to_owned()),
                threshold: THRESHOLD,
                min_silence_duration: pause_ms as f32 / 1000.0,
                min_speech_duration: MIN_SPEECH_SECS,
                window_size: WINDOW_SAMPLES,
                max_speech_duration: MAX_SPEECH_SECS,
            },
            sample_rate: SAMPLE_RATE_HZ as i32,
            num_threads: 1,
            ..Default::default()
        };
        VoiceActivityDetector::create(&config, MAX_SPEECH_SECS + 2.0)
            .map(|vad| Self { vad })
            .ok_or(VadError::ModelLoad)
    }

    /// Feeds 16 kHz mono samples and returns the utterances that ended in them.
    pub fn push(&mut self, samples: &[f32]) -> Vec<Utterance> {
        self.vad.accept_waveform(samples);
        self.drain()
    }

    /// Ends the stream: returns an utterance still running and starts counting from zero again.
    pub fn finish(&mut self) -> Vec<Utterance> {
        self.vad.flush();
        let utterances = self.drain();
        self.vad.reset();
        utterances
    }

    /// Whether speech is going on right now.
    pub fn in_speech(&self) -> bool {
        self.vad.detected()
    }

    fn drain(&mut self) -> Vec<Utterance> {
        let mut utterances = Vec::new();
        while let Some(segment) = self.vad.front() {
            utterances.push(Utterance {
                start_sample: segment.start().max(0) as u64,
                samples: segment.samples().to_vec(),
            });
            self.vad.pop();
        }
        utterances
    }
}
