//! Qwen3-TTS through llama.cpp's audio generation helper (spec 081): the second local speech
//! model next to Pocket TTS, behind the same [`Engine`] trait. One model speaks every language of
//! `tts::LANGUAGES`; the voice's language decides which (research R3). Long sentences reach it in
//! parts (`part_words`, FR-010).

mod ffi;
pub mod generate;

use std::collections::VecDeque;
use std::path::Path;
use std::sync::{Arc, Mutex};

use crate::audio::resample::resample_filtered;
use crate::audio::stretch::stretch;

use super::catalog::TtsCatalogEntry;
use super::local::{speakable_text, Engine, Reference};
use super::{TtsError, RATE_MAX, RATE_MIN};

pub use ffi::Placement;
use ffi::{Handles, OpenOptions, VoiceBitmap};

/// The most words one part may have (spec 081 FR-010).
// ponytail: 20 words is about 6–8 s of German speech (research R5).
pub const PART_WORDS: usize = 20;

/// Voice bitmaps kept, one per voice used recently (as Pocket's voice cache).
const VOICE_CACHE: usize = 8;

/// A loaded Qwen3-TTS model.
pub struct QwenModel {
    inner: Mutex<Inner>,
    sample_rate: u32,
    placement: Placement,
    /// The catalog id, for the log.
    id: String,
}

struct Inner {
    handles: Handles,
    voices: VoicePrints<VoiceBitmap>,
}

/// Recently used voice prints, keyed by the reference's [`fingerprint`]: the newest last, at most
/// [`VOICE_CACHE`] of them (research R3).
pub(crate) struct VoicePrints<T> {
    prints: VecDeque<(u64, Arc<T>)>,
}

impl<T> VoicePrints<T> {
    pub(crate) fn new() -> Self {
        Self {
            prints: VecDeque::new(),
        }
    }

    /// The print kept under `key`, else the one `build` makes, which is kept in place of the
    /// least recently used when the cache is full.
    pub(crate) fn get_or_build(
        &mut self,
        key: u64,
        build: impl FnOnce() -> Result<T, TtsError>,
    ) -> Result<Arc<T>, TtsError> {
        if let Some(i) = self.prints.iter().position(|(k, _)| *k == key) {
            if let Some(entry) = self.prints.remove(i) {
                let print = Arc::clone(&entry.1);
                self.prints.push_back(entry);
                return Ok(print);
            }
        }
        let print = Arc::new(build()?);
        if self.prints.len() == VOICE_CACHE {
            self.prints.pop_front();
        }
        self.prints.push_back((key, Arc::clone(&print)));
        Ok(print)
    }
}

impl QwenModel {
    /// Loads the files of `entry` from `dir` (the backbone and the audio part, as the catalog
    /// names them), on the graphics card or the processor. Takes a few seconds, so callers load
    /// off the async runtime.
    pub fn load(
        dir: &Path,
        entry: &TtsCatalogEntry,
        placement: Placement,
    ) -> Result<Self, TtsError> {
        let files = entry.files.as_deref().unwrap_or_default();
        let named = |audio: bool| {
            files
                .iter()
                .find(|f| f.name.starts_with("mmproj-") == audio)
                .map(|f| dir.join(&f.name))
                .ok_or_else(|| TtsError::Unavailable {
                    reason: format!(
                        "{} names no {} file",
                        entry.id,
                        if audio { "audio" } else { "backbone" }
                    ),
                })
        };
        let (backbone, audio) = (named(false)?, named(true)?);
        let handles = Handles::open(&OpenOptions {
            backbone: &backbone,
            audio: &audio,
            placement,
            threads: i32::try_from(crate::llm::llamacpp::cpu::default_threads()).unwrap_or(4),
        })?;
        Ok(Self {
            sample_rate: handles.sample_rate(),
            placement,
            id: entry.id.clone(),
            inner: Mutex::new(Inner {
                handles,
                voices: VoicePrints::new(),
            }),
        })
    }

    /// Where the model runs.
    pub fn placement(&self) -> Placement {
        self.placement
    }
}

impl Inner {
    /// The bitmap of `reference`, built once per voice.
    fn voice(&mut self, reference: &Reference) -> Result<Arc<VoiceBitmap>, TtsError> {
        let rate = self.handles.reference_rate();
        self.voices.get_or_build(fingerprint(reference), || {
            VoiceBitmap::new(&resample_filtered(
                &reference.samples,
                reference.sample_rate,
                rate,
            ))
        })
    }
}

/// A fingerprint of a reference's content (spec 081 research R3): the same recording gives the
/// same voice print, whichever `Arc` carries it.
pub(crate) fn fingerprint(reference: &Reference) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64 ^ u64::from(reference.sample_rate);
    for sample in &reference.samples {
        for byte in sample.to_bits().to_le_bytes() {
            hash = (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3);
        }
    }
    hash ^ reference.samples.len() as u64
}

impl Engine for QwenModel {
    fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    fn generate(&self, text: &str, reference: &Reference, rate: f32) -> Result<Vec<f32>, TtsError> {
        // Numbers, times and abbreviations as words, as for Pocket (spec 066 FR-022b).
        let words = super::spoken::spoken(&speakable_text(text), &reference.language);
        self.speak_part(&words, reference, rate, 0, &|| false)
    }

    fn part_words(&self) -> Option<usize> {
        Some(PART_WORDS)
    }

    fn compute_unit(&self) -> crate::compute::Unit {
        match self.placement {
            Placement::Cpu => crate::compute::Unit::Processor,
            Placement::Gpu => crate::compute::Unit::GraphicsCard,
        }
    }

    fn log_name(&self) -> String {
        let place = match self.placement {
            Placement::Cpu => "processor",
            Placement::Gpu => "graphics card",
        };
        format!("{} on the {place}", self.id)
    }

    fn speak_part(
        &self,
        text: &str,
        reference: &Reference,
        rate: f32,
        attempt: u32,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Vec<f32>, TtsError> {
        if text.is_empty() {
            return Ok(Vec::new());
        }
        let mut inner = self.inner.lock().map_err(|_| TtsError::Failed {
            reason: "Qwen3-TTS failed earlier and is unusable".to_owned(),
        })?;
        let voice = inner.voice(reference)?;
        let generated = generate::part(
            &mut inner.handles,
            text,
            &reference.language,
            &voice,
            // The same words sound the same; a retry draws anew (research R4).
            generate::seed_for(text).wrapping_add(attempt.wrapping_mul(0x9e37_79b9)),
            cancelled,
        )?;
        // Qwen3-TTS has no speed setting; the rate is applied to its output (research R13).
        Ok(stretch(
            &generated.samples,
            self.sample_rate,
            rate.clamp(RATE_MIN, RATE_MAX),
        ))
    }
}

/// Free memory of the best graphics card llama.cpp sees, if any (spec 081 FR-012).
pub fn gpu_free_bytes() -> Option<u64> {
    use llama_cpp_2::{list_llama_ggml_backend_devices, LlamaBackendDeviceType};
    crate::llm::llamacpp::model::backend().ok()?;
    list_llama_ggml_backend_devices()
        .into_iter()
        .filter(|d| {
            matches!(
                d.device_type,
                LlamaBackendDeviceType::Gpu | LlamaBackendDeviceType::IntegratedGpu
            )
        })
        .map(|d| d.memory_free as u64)
        .max()
}
