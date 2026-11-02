//! Local Whisper transcription (spec 008-voice-control-stt), running
//! `candle-transformers`' Whisper on the same candle-core/candle-nn runtime
//! `mistralrs` already pulls in for chat inference (research.md §1).
//!
//! **Deviation from spec.md FR-010/SC-003 and quickstart.md, decided with
//! the operator 2026-09-17**: the spec calls for the model to ship inside
//! the installer with zero first-run download. This implementation instead
//! downloads the bundled model on demand (mirroring how `models::huggingface`/
//! `models::download` already fetch GGUF chat models), to avoid committing
//! a large binary checkpoint into git history. After that first download,
//! transcription is fully offline (FR-003/SC-004 still hold).
//!
//! **Spec 010 update**: which tier (`tiny`/`base`/`small`) is loaded is now a
//! per-device choice (`voice.stt_model_id` preference, resolved in
//! `voice.rs`) instead of hardcoded to `tiny` for every device — the
//! previous per-device-tiering gap this module used to document is closed by
//! `stt::catalog`'s generalized tier selector. Model files also moved off a
//! Whisper-specific path onto the same generic `models::paths` root chat
//! models already use.

use std::path::Path;
use std::sync::{Arc, Mutex as StdMutex};

use async_trait::async_trait;
use candle::{Device, IndexOp, Tensor};
use candle_nn::VarBuilder;
use candle_transformers::models::whisper::{self as m, audio, Config};
use tauri::{AppHandle, Manager};
use tokenizers::Tokenizer;

use crate::models::paths as model_paths;
use crate::state::AppState;

use super::catalog::SttCatalogEntry;
use super::{CanonicalPcm, SttAdapter, SttError};

use super::files::{
    ensure_model_files, resolve_model_dir, WHISPER_CONFIG as CONFIG_FILENAME,
    WHISPER_TOKENIZER as TOKENIZER_FILENAME, WHISPER_WEIGHTS as WEIGHTS_FILENAME,
};

/// Candidate languages for `detect_language`: the spoken languages of the word list
/// (`assistant::lexicon`, spec 066 research R22) instead of Whisper's full 99-language set.
///
/// The `tiny` model's language-ID is weak on short, largely-silence-padded
/// push-to-talk clips (verified 2026-09-17: a spoken German command was
/// detected as English) — scoring all 99 `<|xx|>` tokens gives a short,
/// ambiguous utterance 98 ways to lose to some other language by a small
/// margin. Restricting the argmax to only the languages we actually care
/// about can only help: it removes irrelevant distractors, never the
/// correct answer, from the comparison. Six candidates instead of de and en
/// cost no accuracy in `tests/stt_compare_eval.rs` (research R7, T130).
fn supported_languages() -> Vec<String> {
    crate::assistant::lexicon::languages().to_vec()
}

/// The bundled local STT adapter, resolved via
/// `providers::local::ensure_local_transcription_provider`'s `"whisper-local"`
/// discriminator. Expensive to construct (model load, ~seconds) — callers
/// load it once and keep it warm (see `VoiceState` in `lib.rs`) rather than
/// reconstructing it per transcription.
pub struct LocalWhisperAdapter {
    inner: Arc<StdMutex<Inner>>,
}

struct Inner {
    model: m::model::Whisper,
    tokenizer: Tokenizer,
    mel_filters: Vec<f32>,
    device: Device,
    /// Candidates for `detect_language`; [`supported_languages`] unless
    /// [`LocalWhisperAdapter::with_languages`] says otherwise.
    languages: Vec<String>,
}

impl LocalWhisperAdapter {
    /// Ensures `entry`'s files are present under its `models::paths` slug
    /// directory (downloading any missing/incomplete file) and loads it
    /// into memory.
    pub async fn load(app: &AppHandle, entry: &SttCatalogEntry) -> Result<Self, SttError> {
        let app_for_dir = app.clone();
        let entry_for_dir = entry.clone();
        let dir = tauri::async_runtime::spawn_blocking(move || {
            resolve_model_dir(&app_for_dir, &entry_for_dir)
        })
        .await
        .map_err(|e| SttError::LocalUnavailable {
            reason: format!("resolve Whisper model directory: {e}"),
        })??;
        let token = app.state::<AppState>().gate().token();
        let _publication_lock = model_paths::acquire_model_publication_lock(app, &entry.id, &token)
            .await
            .map_err(|error| SttError::LocalUnavailable {
                reason: format!("acquire Whisper model lock: {error}"),
            })?;
        ensure_model_files(&dir, entry, |_| {}).await?;
        drop(_publication_lock);
        tauri::async_runtime::spawn_blocking(move || Self::load_from_dir(&dir))
            .await
            .map_err(|e| SttError::LocalUnavailable {
                reason: format!("whisper load task: {e}"),
            })?
    }

    /// Loads an already-downloaded model from `dir`. Split out from
    /// [`Self::load`] so a test can point it at a pre-populated fixture
    /// directory without a network call.
    pub fn load_from_dir(dir: &Path) -> Result<Self, SttError> {
        let config: Config =
            serde_json::from_str(&std::fs::read_to_string(dir.join(CONFIG_FILENAME)).map_err(
                |e| SttError::LocalUnavailable {
                    reason: format!("read {CONFIG_FILENAME}: {e}"),
                },
            )?)
            .map_err(|e| SttError::LocalUnavailable {
                reason: format!("parse {CONFIG_FILENAME}: {e}"),
            })?;

        let tokenizer = Tokenizer::from_file(dir.join(TOKENIZER_FILENAME)).map_err(|e| {
            SttError::LocalUnavailable {
                reason: format!("load {TOKENIZER_FILENAME}: {e}"),
            }
        })?;

        let device = Device::Cpu;
        let vb = unsafe {
            VarBuilder::from_mmaped_safetensors(&[dir.join(WEIGHTS_FILENAME)], m::DTYPE, &device)
        }
        .map_err(|e| SttError::LocalUnavailable {
            reason: format!("load {WEIGHTS_FILENAME}: {e}"),
        })?;
        let mel_filters = mel_filterbank(m::SAMPLE_RATE as u32, m::N_FFT, config.num_mel_bins);
        let model =
            m::model::Whisper::load(&vb, config).map_err(|e| SttError::LocalUnavailable {
                reason: format!("build whisper model: {e}"),
            })?;

        Ok(Self {
            inner: Arc::new(StdMutex::new(Inner {
                model,
                tokenizer,
                mel_filters,
                device,
                languages: supported_languages(),
            })),
        })
    }

    /// Restricts language detection to `codes` (Whisper language codes such
    /// as `"de"`) instead of [`supported_languages`]. Spec 066 (research
    /// R22) widens it to the six spoken languages; the comparison in
    /// `tests/stt_compare_eval.rs` measures what that costs.
    pub fn with_languages(self, codes: &[&str]) -> Self {
        if let Ok(mut inner) = self.inner.lock() {
            inner.languages = codes.iter().map(|code| code.to_string()).collect();
        }
        self
    }
}

#[async_trait]
impl SttAdapter for LocalWhisperAdapter {
    async fn transcribe(&self, audio_in: &CanonicalPcm) -> Result<String, SttError> {
        let inner = Arc::clone(&self.inner);
        // Candle inference is synchronous/CPU-bound; `samples` is cloned so
        // the closure can be `'static` for `spawn_blocking` (matches this
        // codebase's established blocking-work convention, e.g.
        // `storage::preferences_commands`).
        let samples = audio_in.samples.clone();
        tauri::async_runtime::spawn_blocking(move || {
            let mut inner = inner.lock().map_err(|e| SttError::Failed {
                reason: format!("whisper state poisoned: {e}"),
            })?;
            transcribe_samples(&mut inner, &samples)
        })
        .await
        .map_err(|e| SttError::Failed {
            reason: format!("whisper inference task: {e}"),
        })?
    }
}

fn transcribe_samples(inner: &mut Inner, samples: &[f32]) -> Result<String, SttError> {
    let num_mel_bins = inner.model.config.num_mel_bins;
    let mel = audio::pcm_to_mel(&inner.model.config, samples, &inner.mel_filters);
    let mel_len = mel.len();
    let mel = Tensor::from_vec(
        mel,
        (1, num_mel_bins, mel_len / num_mel_bins),
        &inner.device,
    )
    .map_err(tensor_err)?;

    let sot_token = token_id(&inner.tokenizer, m::SOT_TOKEN)?;
    let transcribe_token = token_id(&inner.tokenizer, m::TRANSCRIBE_TOKEN)?;
    let no_timestamps_token = token_id(&inner.tokenizer, m::NO_TIMESTAMPS_TOKEN)?;
    let eot_token = token_id(&inner.tokenizer, m::EOT_TOKEN)?;

    let (_, _, frame_count) = mel.dims3().map_err(tensor_err)?;
    let mut decoded_chunks = Vec::new();
    for start in (0..frame_count).step_by(m::N_FRAMES) {
        let chunk_len = (frame_count - start).min(m::N_FRAMES);
        let mel_chunk = mel.narrow(2, start, chunk_len).map_err(tensor_err)?;

        // Flush the encoder and decoder caches for every independent audio
        // segment. Whisper's positional embedding only supports N_FRAMES,
        // while Capture permits recordings up to 60 seconds.
        let audio_features = inner
            .model
            .encoder
            .forward(&mel_chunk, true)
            .map_err(tensor_err)?;
        let language_token = detect_language(inner, &audio_features, sot_token)?;

        let mut tokens = vec![
            sot_token,
            language_token,
            transcribe_token,
            no_timestamps_token,
        ];
        let sample_len = inner.model.config.max_target_positions / 2;

        for i in 0..sample_len {
            let tokens_t = Tensor::new(tokens.as_slice(), &inner.device)
                .and_then(|t| t.unsqueeze(0))
                .map_err(tensor_err)?;
            let ys = inner
                .model
                .decoder
                .forward(&tokens_t, &audio_features, i == 0)
                .map_err(tensor_err)?;
            let (_, seq_len, _) = ys.dims3().map_err(tensor_err)?;
            let logits = inner
                .model
                .decoder
                .final_linear(&ys.i((..1, seq_len - 1..)).map_err(tensor_err)?)
                .map_err(tensor_err)?
                .i(0)
                .map_err(tensor_err)?
                .i(0)
                .map_err(tensor_err)?;
            let logits_v: Vec<f32> = logits.to_vec1().map_err(tensor_err)?;
            let next_token = argmax(&logits_v);
            tokens.push(next_token);
            if next_token == eot_token || tokens.len() > inner.model.config.max_target_positions {
                break;
            }
        }

        let text = inner
            .tokenizer
            .decode(&tokens, true)
            .map_err(|e| SttError::Failed {
                reason: format!("tokenizer decode: {e}"),
            })?;
        if !text.trim().is_empty() {
            decoded_chunks.push(text.trim().to_string());
        }
    }

    Ok(decoded_chunks.join(" "))
}

/// Mirrors candle's whisper example `multilingual::detect_language`: a
/// single decoder step over just `[sot_token]`, picking whichever `<|xx|>`
/// language token scores highest — restricted to `Inner::languages`
/// rather than Whisper's full language set.
fn detect_language(
    inner: &mut Inner,
    audio_features: &Tensor,
    sot_token: u32,
) -> Result<u32, SttError> {
    let device = &inner.device;
    let language_token_ids = inner
        .languages
        .iter()
        .map(|code| token_id(&inner.tokenizer, &format!("<|{code}|>")))
        .collect::<Result<Vec<_>, _>>()?;
    let tokens = Tensor::new(&[[sot_token]], device).map_err(tensor_err)?;
    let ys = inner
        .model
        .decoder
        .forward(&tokens, audio_features, true)
        .map_err(tensor_err)?;
    let logits = inner
        .model
        .decoder
        .final_linear(&ys.i(..1).map_err(tensor_err)?)
        .map_err(tensor_err)?
        .i(0)
        .map_err(tensor_err)?
        .i(0)
        .map_err(tensor_err)?;
    let ids_tensor = Tensor::new(language_token_ids.as_slice(), device).map_err(tensor_err)?;
    let logits = logits.index_select(&ids_tensor, 0).map_err(tensor_err)?;
    let logits_v: Vec<f32> = logits.to_vec1().map_err(tensor_err)?;
    Ok(language_token_ids[argmax(&logits_v) as usize])
}

fn argmax(values: &[f32]) -> u32 {
    values
        .iter()
        .enumerate()
        .max_by(|(_, a), (_, b)| a.total_cmp(b))
        .map(|(i, _)| i as u32)
        .unwrap_or(0)
}

fn token_id(tokenizer: &Tokenizer, token: &str) -> Result<u32, SttError> {
    tokenizer
        .token_to_id(token)
        .ok_or_else(|| SttError::Failed {
            reason: format!("no token id for {token}"),
        })
}

fn tensor_err(e: candle::Error) -> SttError {
    SttError::Failed {
        reason: format!("whisper tensor op: {e}"),
    }
}

/// Computes the Slaney-normalized mel filterbank matrix Whisper expects
/// (`librosa.filters.mel(sr, n_fft, n_mels)` with its default `norm =
/// "slaney"`, `htk = False`) — `candle-transformers`' `audio::pcm_to_mel`
/// takes this as a caller-supplied argument rather than computing it
/// itself. candle's own whisper example ships this as a precomputed binary
/// asset (`melfilters.bytes`) fetched from its own repo; this computes it
/// on the fly instead, to avoid vendoring a second binary blob and a second
/// external download host. Verified bit-for-bit (up to sign-of-zero and one
/// ULP of summation-order noise) against `librosa.filters.mel` for the
/// tiny/base/small (80-bin) shape used here.
///
/// Row-major, shape `(n_mels, n_fft / 2 + 1)` — the layout
/// `audio::pcm_to_mel` expects.
pub(crate) fn mel_filterbank(sample_rate: u32, n_fft: usize, n_mels: usize) -> Vec<f32> {
    let n_freqs = n_fft / 2 + 1;
    let fmax = sample_rate as f64 / 2.0;

    let fft_freqs: Vec<f64> = (0..n_freqs)
        .map(|i| i as f64 * fmax / (n_freqs - 1) as f64)
        .collect();

    let mel_min = hz_to_mel(0.0);
    let mel_max = hz_to_mel(fmax);
    let hz_points: Vec<f64> = (0..n_mels + 2)
        .map(|i| mel_to_hz(mel_min + (mel_max - mel_min) * i as f64 / (n_mels + 1) as f64))
        .collect();
    let fdiff: Vec<f64> = hz_points.windows(2).map(|w| w[1] - w[0]).collect();

    let mut weights = vec![0f32; n_mels * n_freqs];
    for i in 0..n_mels {
        for (j, &freq) in fft_freqs.iter().enumerate() {
            let lower = (freq - hz_points[i]) / fdiff[i];
            let upper = (hz_points[i + 2] - freq) / fdiff[i + 1];
            weights[i * n_freqs + j] = lower.min(upper).max(0.0) as f32;
        }
        // Slaney-style area normalization so each filter integrates to
        // (roughly) the same energy regardless of its width.
        let enorm = (2.0 / (hz_points[i + 2] - hz_points[i])) as f32;
        for w in &mut weights[i * n_freqs..(i + 1) * n_freqs] {
            *w *= enorm;
        }
    }
    weights
}

/// Slaney mel scale (`librosa.core.convert.hz_to_mel`, `htk=False`):
/// linear below 1 kHz, logarithmic above.
fn hz_to_mel(hz: f64) -> f64 {
    const F_SP: f64 = 200.0 / 3.0;
    const MIN_LOG_HZ: f64 = 1000.0;
    let min_log_mel = MIN_LOG_HZ / F_SP;
    let logstep = 6.4f64.ln() / 27.0;
    if hz >= MIN_LOG_HZ {
        min_log_mel + (hz / MIN_LOG_HZ).ln() / logstep
    } else {
        hz / F_SP
    }
}

/// Inverse of [`hz_to_mel`] (`librosa.core.convert.mel_to_hz`, `htk=False`).
fn mel_to_hz(mel: f64) -> f64 {
    const F_SP: f64 = 200.0 / 3.0;
    const MIN_LOG_HZ: f64 = 1000.0;
    let min_log_mel = MIN_LOG_HZ / F_SP;
    let logstep = 6.4f64.ln() / 27.0;
    if mel >= min_log_mel {
        MIN_LOG_HZ * (logstep * (mel - min_log_mel)).exp()
    } else {
        F_SP * mel
    }
}
