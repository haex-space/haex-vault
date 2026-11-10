//! Reading an answer of the agent aloud from the chat (spec 066 FR-022c, operator 2026-11-07): the
//! speaker button of a message says the whole answer with the speech model and voice chosen for its
//! language, prepared the same way as in a spoken conversation (no formatting marks, pointers for
//! tables and code, numbers and dates in words). A newer reading or voice sample stops this one, and
//! so does the stop button.
//!
//! The language is the one the answer is written in, found by the word list's `detect` words, the
//! interface language when the text gives nothing to go on; a passage in another language is
//! spoken with that language's voice (FR-022d). Nothing is downloaded: a speech model
//! that is missing is reported, the settings offer the download.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use tauri::{AppHandle, Manager};

use crate::assistant::lexicon::lexicon;
use crate::assistant::passage::PassageLanguage;
use crate::assistant::speakable::{speak_all, Clock};
use crate::audio::stream::{AudioStream, Output};
use crate::error::{HolziError, Result};
use crate::state::AppState;
use crate::voice::VoiceState;

use super::check::{Level, SpeechShare};
use super::languages::AppVoices;
use super::local::{LanguageVoices, Passage, Reference, Sentence, Speaker, SpeechOut};
use super::preview::NoInput;
use super::speed::Meter;
use super::{commands, voices};

/// How often waiting looks whether the reading is over or stopped.
const POLL: Duration = Duration::from_millis(20);
/// The device still plays its own buffer when the mixer is empty; an own stream stays open this
/// much longer.
const TAIL: Duration = Duration::from_millis(250);

/// What is read: the language of `text` (the interface language `ui_language` when the text gives
/// nothing to go on) and its sentences as they are spoken.
pub fn reading(text: &str, ui_language: &str, clock: Clock) -> (String, Vec<String>) {
    let words = lexicon();
    let fallback = if words.languages().iter().any(|l| l == ui_language) {
        ui_language
    } else {
        crate::assistant::lexicon::FALLBACK_LANGUAGE
    };
    let language = words.detect(text, fallback).to_owned();
    let sentences = speak_all(text, &language, clock);
    (language, sentences)
}

/// Reads `text` aloud. Resolves `true` once everything has sounded, `false` when it was stopped (a
/// newer reading or sample, the stop button, the vault closing).
pub async fn run(
    app: &AppHandle,
    state: &AppState,
    text: &str,
    ui_language: &str,
    utc_offset_minutes: i32,
) -> Result<bool> {
    let speech = app.state::<VoiceState>();
    let cancel = speech.begin_preview(&state.gate().token());
    let (language, sentences) = reading(text, ui_language, Clock::now(utc_offset_minutes));
    let mut passage = PassageLanguage::new(&language);
    let passages: Vec<String> = sentences.iter().map(|s| passage.of(s).to_owned()).collect();
    let other_voices: Arc<dyn LanguageVoices> = AppVoices::new(app.clone());
    if sentences.is_empty() {
        return Ok(true);
    }
    let engines = commands::load_installed_speech_engines(app, state.gate(), &language).await?;
    let meter = Arc::new(Meter::new(&engines.dir, "cpu"));
    let check = engines
        .engine
        .part_words()
        .map(|_| Arc::new(Level) as Arc<dyn SpeechShare>);
    let (preference, rate) = commands::voice_settings(app, &language).await?;
    let own = commands::own_recording(state, preference.as_deref()).await?;
    let reference = Arc::new(voices::choose(preference.as_deref(), &language, own)?.reference()?);
    if cancel.is_cancelled() {
        return Ok(false);
    }

    let shared = speech.audio_stream().current();
    let gate = state.gate().clone();
    state
        .gate()
        .spawn_blocking(move || {
            let (out, own_stream): (Arc<dyn SpeechOut>, Option<Arc<AudioStream>>) = match shared {
                Some(stream) => (stream, None),
                None => {
                    let stream = Arc::new(
                        AudioStream::start(&gate, Box::new(NoInput), Output::Device).map_err(
                            |e| {
                                log::warn!("open the speaker to read aloud: {e}");
                                HolziError::DeviceUnavailable
                            },
                        )?,
                    );
                    (Arc::clone(&stream) as Arc<dyn SpeechOut>, Some(stream))
                }
            };
            let played = Arc::new(AtomicU64::new(0));
            let counter = Arc::clone(&played);
            let speaker = Speaker::start_reporting(
                &gate,
                out,
                Arc::new(move |_| {
                    counter.fetch_add(1, Ordering::AcqRel);
                }),
            )?;
            let total = sentences.len() as u64;
            for (text, passage) in sentences.into_iter().zip(passages) {
                speaker.speak(Sentence {
                    text,
                    engine: Arc::clone(&engines.engine),
                    reference: Arc::clone(&reference) as Arc<Reference>,
                    rate,
                    reserve: engines.reserve.clone(),
                    speech: check.clone(),
                    meter: Some(Arc::clone(&meter)),
                    reserve_notice: None,
                    passage: Some(Passage {
                        language: passage,
                        voices: Arc::clone(&other_voices),
                    }),
                });
            }
            speaker.finish_answer();
            let finished = loop {
                if cancel.is_cancelled() {
                    speaker.stop();
                    break false;
                }
                if played.load(Ordering::Acquire) >= total {
                    break true;
                }
                std::thread::sleep(POLL);
            };
            drop(speaker);
            if let Some(stream) = own_stream {
                if finished {
                    std::thread::sleep(TAIL);
                }
                stream.stop();
            }
            Ok::<bool, HolziError>(finished)
        })?
        .await
        .map_err(|e| HolziError::SpeechFailed {
            reason: format!("read aloud: {e}"),
        })?
}
