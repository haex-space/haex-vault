//! The assistant's tones (spec 066 FR-043/FR-044, research R16): synthesized at runtime from short
//! notes of one pentatonic scale, so they sound like one family, need no file and carry no foreign
//! licence. Each tone is a few soft sine/FM notes with a quick attack and an exponential decay.

use std::f32::consts::TAU;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State};
use tokio_util::sync::CancellationToken;

use crate::audio::mixer::MixerInput;
use crate::error::{HolziError, Result};
use crate::state::AppState;
use crate::voice::VoiceState;

/// Visible counterpart of every tone (FR-044).
pub const EVENT_TONE: &str = "assistant-tone";

/// `ToneId` of contracts/tauri-commands.md.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ToneId {
    Listening,
    Understood,
    Working,
    Question,
    Done,
    Error,
    Aborted,
    NotUnderstood,
    ConversationEnd,
    Attention,
    Secret,
    ModeOn,
    ModeOff,
    Muted,
    Unmuted,
}

impl ToneId {
    pub const ALL: [ToneId; 15] = [
        ToneId::Listening,
        ToneId::Understood,
        ToneId::Working,
        ToneId::Question,
        ToneId::Done,
        ToneId::Error,
        ToneId::Aborted,
        ToneId::NotUnderstood,
        ToneId::ConversationEnd,
        ToneId::Attention,
        ToneId::Secret,
        ToneId::ModeOn,
        ToneId::ModeOff,
        ToneId::Muted,
        ToneId::Unmuted,
    ];
}

/// The root of the family: D5.
pub const BASE_HZ: f32 = 587.33;
/// Steps of the D major pentatonic, in semitones from [`BASE_HZ`]; every note of every tone is
/// one of them.
pub const SCALE: [i8; 7] = [-5, 0, 2, 4, 7, 9, 12];
/// "Working" sounds again after this long while a round runs without a spoken answer (FR-043).
pub const WORKING_REPEAT: Duration = Duration::from_secs(2);

/// One note: a scale step, how long it sounds and how loud (0–1).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Note {
    pub step: i8,
    pub millis: u16,
    pub gain: f32,
}

/// A [`Note`] literal; plain struct literals keep the slices in [`notes`] `'static`.
macro_rules! note {
    ($step:expr, $millis:expr, $gain:expr) => {
        Note {
            step: $step,
            millis: $millis,
            gain: $gain,
        }
    };
}

/// Silence between two notes of a tone.
const GAP_MILLIS: u16 = 15;
const ATTACK_MILLIS: f32 = 4.0;
/// Overall level; with gain 1.0 and the FM partial the peak stays well below clipping.
const LEVEL: f32 = 0.5;
/// FM: a soft second partial for a bell-like colour.
const FM_RATIO: f32 = 2.0;
const FM_INDEX: f32 = 0.6;

/// The notes of `tone`.
pub fn notes(tone: ToneId) -> &'static [Note] {
    match tone {
        ToneId::Listening => &[note!(0, 70, 0.9), note!(7, 110, 1.0)],
        ToneId::Understood => &[note!(7, 80, 0.8)],
        ToneId::Working => &[note!(0, 90, 0.35)],
        ToneId::Question => &[note!(4, 80, 0.8), note!(9, 150, 0.9)],
        ToneId::Done => &[note!(0, 70, 0.8), note!(4, 70, 0.85), note!(7, 140, 0.9)],
        ToneId::Error => &[note!(4, 110, 0.9), note!(-5, 180, 0.9)],
        ToneId::Aborted => &[note!(7, 70, 0.8), note!(0, 120, 0.8)],
        ToneId::NotUnderstood => &[note!(2, 70, 0.7), note!(2, 70, 0.7)],
        ToneId::ConversationEnd => &[note!(7, 80, 0.6), note!(4, 80, 0.55), note!(0, 150, 0.5)],
        ToneId::Attention => &[note!(12, 80, 0.8), note!(7, 80, 0.75), note!(12, 120, 0.8)],
        ToneId::Secret => &[note!(9, 90, 0.8), note!(4, 160, 0.8)],
        ToneId::ModeOn => &[note!(0, 70, 0.8), note!(7, 70, 0.85), note!(12, 150, 0.9)],
        ToneId::ModeOff => &[note!(12, 90, 0.8), note!(0, 180, 0.7)],
        ToneId::Muted => &[note!(4, 80, 0.5), note!(0, 130, 0.45)],
        ToneId::Unmuted => &[note!(0, 80, 0.5), note!(4, 130, 0.55)],
    }
}

/// How long `tone` sounds.
pub fn duration(tone: ToneId) -> Duration {
    let notes = notes(tone);
    let sounding: u64 = notes.iter().map(|n| u64::from(n.millis)).sum();
    let gaps = u64::from(GAP_MILLIS) * notes.len().saturating_sub(1) as u64;
    Duration::from_millis(sounding + gaps)
}

/// After how long `tone` repeats on its own, if it does.
pub fn repeat_interval(tone: ToneId) -> Option<Duration> {
    (tone == ToneId::Working).then_some(WORKING_REPEAT)
}

/// Tones that sound even with tones switched off (FR-045).
pub fn always_audible(tone: ToneId) -> bool {
    matches!(tone, ToneId::Listening | ToneId::Secret)
}

/// `tone` as mono samples at `sample_rate`.
pub fn render(tone: ToneId, sample_rate: u32) -> Vec<f32> {
    let rate = sample_rate as f32;
    let mut out = Vec::new();
    for (i, n) in notes(tone).iter().enumerate() {
        if i > 0 {
            out.resize(out.len() + samples_for(GAP_MILLIS, rate), 0.0);
        }
        let hz = BASE_HZ * 2f32.powf(f32::from(n.step) / 12.0);
        let count = samples_for(n.millis, rate);
        let attack = ATTACK_MILLIS / 1000.0 * rate;
        // Decays to about 1 % by the end of the note, so notes never click.
        let decay = 4.6 / count as f32;
        out.extend((0..count).map(|k| {
            let t = k as f32 / rate;
            let envelope = (k as f32 / attack).min(1.0) * (-decay * k as f32).exp();
            let modulator = FM_INDEX * (TAU * hz * FM_RATIO * t).sin();
            LEVEL * n.gain * envelope * (TAU * hz * t + modulator).sin()
        }));
    }
    out
}

fn samples_for(millis: u16, rate: f32) -> usize {
    (f32::from(millis) / 1000.0 * rate) as usize
}

#[derive(Debug, Clone, Serialize)]
struct ToneEvent {
    tone: ToneId,
}

/// Shows `tone` and plays it into `mixer`, unless tones are off and it is not one that always
/// sounds. The volume is the mixer's tone volume (`assistant.tones.volume`).
pub fn play(app: &AppHandle, mixer: &MixerInput, tone: ToneId, tones_enabled: bool) {
    let _ = app.emit(EVENT_TONE, ToneEvent { tone });
    if tones_enabled || always_audible(tone) {
        mixer.play_tone(&render(tone, mixer.sample_rate()));
    }
}

/// How often a tone sample looks for a newer sample while it waits.
const SAMPLE_POLL: Duration = Duration::from_millis(20);

/// Plays `tone` into `mixer` at `volume` percent and waits until it has sounded (the settings
/// page's "Anhören", T030). Returns `false` when `cancel` fired first; a tone always plays to its
/// end, so only the waiting stops. Sets the mixer's tone volume, which is the setting being tried.
pub fn sample(mixer: &MixerInput, tone: ToneId, volume: u8, cancel: &CancellationToken) -> bool {
    mixer.set_tone_volume(volume);
    if !mixer.play_tone(&render(tone, mixer.sample_rate())) {
        return false;
    }
    let ends_at = std::time::Instant::now() + duration(tone);
    while std::time::Instant::now() < ends_at {
        if cancel.is_cancelled() {
            return false;
        }
        std::thread::sleep(SAMPLE_POLL);
    }
    true
}

/// Plays `tone` once at `volume` (0–100) for the settings page, also while tones are switched off.
/// A newer sample (tone or voice) ends the wait for this one.
#[tauri::command]
pub async fn assistant_tone_preview(
    app: AppHandle,
    state: State<'_, AppState>,
    tone: ToneId,
    volume: u8,
) -> Result<()> {
    if volume > 100 {
        return Err(HolziError::InvalidInput {
            reason: format!("tone volume {volume} is above 100"),
        });
    }
    let voice = app.state::<VoiceState>();
    let cancel = voice.begin_preview(&state.gate().token());
    let shared = voice.audio_stream().current();
    let gate = state.gate().clone();
    let work = state.gate().spawn_blocking(move || {
        crate::tts::preview::on_speaker(&gate, shared, |mixer| {
            Ok(sample(mixer, tone, volume, &cancel))
        })
    })?;
    state
        .gate()
        .run(work)
        .await?
        .map_err(|e| HolziError::SpeechFailed {
            reason: format!("tone sample: {e}"),
        })?
}
