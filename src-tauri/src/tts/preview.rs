//! The samples of the settings pages (spec 066 T028, T030): a voice says two sentences in its
//! language at the chosen rate, a tone sounds at the chosen volume, through the assistant's mixer.
//! While the assistant runs, its stream carries a sample; otherwise a stream with only the speaker
//! opens for it, so a sample never turns the microphone on.

use std::sync::Arc;
use std::time::{Duration, Instant};

use tauri::{AppHandle, Manager};
use tokio_util::sync::CancellationToken;

use crate::audio::mixer::MixerInput;
use crate::audio::resample::resample_linear;
use crate::audio::source::{Read, Source};
use crate::audio::stream::{AudioStream, Output};
use crate::error::{HolziError, Result};
use crate::state::AppState;
use crate::vault_gate::VaultGate;
use crate::voice::VoiceState;

use super::local::{Engine, Reference};
use super::voices::{self, BuiltinVoice};
use super::{catalog, commands, files, TtsError};

/// Silence between the two sentences.
const GAP: Duration = Duration::from_millis(300);
/// How often waiting looks for a newer sample.
const POLL: Duration = Duration::from_millis(20);
/// The device still plays its own buffer when the mixer is empty; an own stream stays open this
/// much longer.
const TAIL: Duration = Duration::from_millis(250);

/// What a voice says as its sample, in its language. The listening check of the built-in voices
/// uses the same sentences.
pub fn sample(language: &str) -> [&'static str; 2] {
    match language {
        "de" => [
            "Hallo, so klinge ich.",
            "Soll ich dich an den Zahnarzttermin morgen um halb zehn erinnern?",
        ],
        "fr" => [
            "Bonjour, voici ma voix.",
            "Veux-tu que je te rappelle ton rendez-vous chez le dentiste demain à neuf heures et demie ?",
        ],
        "es" => [
            "Hola, así suena mi voz.",
            "¿Quieres que te recuerde la cita con el dentista mañana a las nueve y media?",
        ],
        "it" => [
            "Ciao, questa è la mia voce.",
            "Vuoi che ti ricordi l'appuntamento dal dentista domani alle nove e mezza?",
        ],
        "pt" => [
            "Olá, esta é a minha voz.",
            "Quer que eu te lembre da consulta no dentista amanhã às nove e meia?",
        ],
        _ => [
            "Hi, this is how I sound.",
            "Should I remind you of your dentist appointment tomorrow at half past nine?",
        ],
    }
}

/// A microphone that never delivers: the stream only plays.
struct NoInput;

impl Source for NoInput {
    fn read(&mut self, _out: &mut Vec<f32>) -> Read {
        Read::Idle
    }
}

/// Speaks the sample of `language` into `mixer` and waits until it has played. Returns `false`
/// when `cancel` fired (a newer sample, the vault closing) or speech was stopped elsewhere; what
/// is still queued is dropped then. The next sentence is synthesized while the previous plays.
pub fn play(
    engine: &dyn Engine,
    reference: &Reference,
    language: &str,
    rate: f32,
    mixer: &MixerInput,
    cancel: &CancellationToken,
) -> std::result::Result<bool, TtsError> {
    let out_rate = mixer.sample_rate();
    let sentences = sample(language);
    let mut ends_at = Instant::now();
    for (i, sentence) in sentences.iter().enumerate() {
        if cancel.is_cancelled() {
            mixer.stop_speech();
            return Ok(false);
        }
        let speech = engine
            .generate(sentence, reference, rate)
            .inspect_err(|_| mixer.stop_speech())?;
        let mut samples = resample_linear(&speech, engine.sample_rate(), out_rate);
        if i + 1 < sentences.len() {
            let gap = (GAP.as_secs_f64() * f64::from(out_rate)) as usize;
            samples.resize(samples.len() + gap, 0.0);
        }
        let length = Duration::from_secs_f64(samples.len() as f64 / f64::from(out_rate));
        ends_at = ends_at.max(Instant::now()) + length;
        if !mixer.queue_speech(&samples, || cancel.is_cancelled()) {
            mixer.stop_speech();
            return Ok(false);
        }
    }
    while Instant::now() < ends_at {
        if cancel.is_cancelled() {
            mixer.stop_speech();
            return Ok(false);
        }
        std::thread::sleep(POLL);
    }
    Ok(true)
}

/// `assistant_tts_preview`: downloads the speech model of `voice`'s language if it is missing,
/// then plays the sample. A newer sample stops this one; both calls resolve.
pub async fn run(
    app: &AppHandle,
    state: &AppState,
    voice: &'static BuiltinVoice,
    rate: f32,
) -> Result<()> {
    let speech = app.state::<VoiceState>();
    let cancel = speech.begin_preview(&state.gate().token());
    let entry = catalog::default_for(&voice.language)?.ok_or_else(|| TtsError::Unavailable {
        reason: format!("no speech model speaks {}", voice.language),
    })?;
    commands::ensure_installed(app, state, entry).await?;
    let app_for_dir = app.clone();
    let dir =
        tauri::async_runtime::spawn_blocking(move || files::resolve_model_dir(&app_for_dir, entry))
            .await
            .map_err(|e| HolziError::SpeechFailed {
                reason: format!("resolve speech model directory: {e}"),
            })??;
    let model = speech.speech_model(&entry.id, dir, state.gate()).await?;
    let reference = voices::reference(voice)?;
    let shared = speech.audio_stream().current();
    let gate = state.gate().clone();
    let language = voice.language.as_str();
    state
        .gate()
        .spawn_blocking(move || {
            on_speaker(&gate, shared, |mixer| {
                Ok(play(&*model, &reference, language, rate, mixer, &cancel)?)
            })
        })?
        .await
        .map_err(|e| HolziError::SpeechFailed {
            reason: format!("voice sample: {e}"),
        })?
}

/// Runs `sample` on the assistant's mixer while its stream runs (`shared`), otherwise on a stream
/// with only the speaker, which stays open until the device has played what `sample` queued.
/// `sample` returns whether it played to its end. Blocks; call it off the async runtime.
pub fn on_speaker(
    gate: &VaultGate,
    shared: Option<Arc<AudioStream>>,
    sample: impl FnOnce(&MixerInput) -> Result<bool>,
) -> Result<()> {
    if let Some(stream) = shared {
        sample(stream.mixer())?;
        return Ok(());
    }
    let stream = AudioStream::start(gate, Box::new(NoInput), Output::Device).map_err(|e| {
        log::warn!("open the speaker for a sample: {e}");
        HolziError::DeviceUnavailable
    })?;
    if sample(stream.mixer())? {
        std::thread::sleep(TAIL);
    }
    stream.stop();
    Ok(())
}
