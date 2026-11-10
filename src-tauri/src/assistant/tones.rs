//! The assistant's tones (spec 066 FR-043/FR-044, research R16): synthesized at runtime from short
//! notes of one pentatonic scale, so they sound like one family, need no file and carry no foreign
//! licence. Within the family each tone has its own colour (bell, wood, soft, buzz, tick), shape
//! (rising, falling, gliding, repeated), rhythm and register, so they are easy to tell apart.

use std::f32::consts::TAU;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State};
use tokio_util::sync::CancellationToken;

use crate::audio::mixer::MixerInput;
use crate::audio::resample::resample_linear;
use crate::error::{HolziError, Result};
use crate::state::AppState;
use crate::voice::VoiceState;

use super::custom_tones::{self, ToneBank};

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
/// Steps of the D major pentatonic over two and a half octaves, in semitones from [`BASE_HZ`];
/// every note of every tone starts and ends on one of them, so the set stays one family.
pub const SCALE: [i8; 14] = [-12, -10, -8, -5, -3, 0, 2, 4, 7, 9, 12, 14, 16, 19];
/// "Working" sounds again after this long while a round runs without a spoken answer (FR-043).
pub const WORKING_REPEAT: Duration = Duration::from_secs(2);

/// The colour of a note. Tones differ in colour as well as in melody, rhythm and register, so two
/// of them never sound alike (FR-043 "unterscheidbar").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Timbre {
    /// A bright bell with a long ring: beginnings and good news.
    Bell,
    /// A short wooden knock: something was taken in.
    Wood,
    /// A pure, soft tone with a gentle start: quiet state changes.
    Soft,
    /// A rough, buzzing tone: something went wrong or was not understood.
    Buzz,
    /// A tiny tick: the working pulse.
    Tick,
}

/// One note: from one scale step to another (equal steps hold the pitch, different ones glide),
/// how long it sounds, how loud (0–1) and in which colour.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Note {
    pub step: i8,
    pub to: i8,
    pub millis: u16,
    pub gain: f32,
    pub timbre: Timbre,
}

/// A [`Note`] literal; plain struct literals keep the slices in [`notes`] `'static`.
macro_rules! note {
    ($timbre:ident, $step:expr, $millis:expr, $gain:expr) => {
        note!($timbre, $step => $step, $millis, $gain)
    };
    ($timbre:ident, $step:expr => $to:expr, $millis:expr, $gain:expr) => {
        Note {
            step: $step,
            to: $to,
            millis: $millis,
            gain: $gain,
            timbre: Timbre::$timbre,
        }
    };
}

/// Silence between two notes of a tone.
const GAP_MILLIS: u16 = 25;
/// Overall level; with gain 1.0 the peak stays well below clipping.
const LEVEL: f32 = 0.45;

/// The notes of `tone`. Each tone has its own colour and shape:
///
/// - listening: a bright bell gliding up an octave, "I'm all ears"
/// - understood: two quick wooden knocks, the second higher
/// - working: a faint high tick every two seconds
/// - question: a plucked rise with a longer, higher end, like a spoken question
/// - done: a fast bright arpeggio up
/// - error: a low buzz falling
/// - aborted: a wooden glide down an octave, cut short
/// - notUnderstood: the same buzzy note twice, "nn-nn"
/// - conversationEnd: a slow soft fall over three notes
/// - attention: a high bell, three times
/// - secret: two low soft notes, hushed
/// - modeOn / modeOff: a long soft glide up / down
/// - muted / unmuted: a bell and a soft note, falling / rising
pub fn notes(tone: ToneId) -> &'static [Note] {
    match tone {
        ToneId::Listening => &[note!(Bell, 0 => 12, 170, 0.9)],
        ToneId::Understood => &[note!(Wood, 0, 50, 0.85), note!(Wood, 7, 60, 0.9)],
        ToneId::Working => &[note!(Tick, 19, 30, 0.3)],
        ToneId::Question => &[note!(Wood, 0, 80, 0.85), note!(Bell, 9 => 12, 200, 0.85)],
        ToneId::Done => &[
            note!(Bell, 0, 60, 0.75),
            note!(Bell, 4, 60, 0.8),
            note!(Bell, 7, 60, 0.85),
            note!(Bell, 12, 120, 0.9),
        ],
        ToneId::Error => &[note!(Buzz, -5, 120, 0.8), note!(Buzz, -8 => -12, 200, 0.8)],
        ToneId::Aborted => &[note!(Wood, 12 => 0, 140, 0.85)],
        ToneId::NotUnderstood => &[note!(Buzz, 2, 90, 0.7), note!(Buzz, 2, 120, 0.7)],
        ToneId::ConversationEnd => &[
            note!(Soft, 7, 90, 0.7),
            note!(Soft, 4, 90, 0.65),
            note!(Soft, 0, 150, 0.6),
        ],
        ToneId::Attention => &[
            note!(Bell, 16, 60, 0.8),
            note!(Bell, 16, 60, 0.8),
            note!(Bell, 16, 120, 0.85),
        ],
        ToneId::Secret => &[note!(Soft, -3, 120, 0.6), note!(Soft, -8, 180, 0.55)],
        ToneId::ModeOn => &[note!(Soft, -12 => 7, 330, 0.75)],
        ToneId::ModeOff => &[note!(Soft, 7 => -12, 330, 0.7)],
        ToneId::Muted => &[note!(Bell, 4, 70, 0.6), note!(Soft, -5, 130, 0.6)],
        ToneId::Unmuted => &[note!(Soft, -5, 70, 0.6), note!(Bell, 4, 130, 0.65)],
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

/// Attack in milliseconds and how far a note has decayed by its end (as a power of e).
fn envelope_shape(timbre: Timbre) -> (f32, f32) {
    match timbre {
        Timbre::Bell => (3.0, 4.6),
        Timbre::Wood => (1.0, 7.0),
        Timbre::Soft => (18.0, 4.6),
        Timbre::Buzz => (6.0, 4.6),
        Timbre::Tick => (0.5, 8.0),
    }
}

/// One sample of `timbre` at `phase` (radians of the fundamental), `progress` 0–1 through the note.
fn voice(timbre: Timbre, phase: f32, progress: f32) -> f32 {
    match timbre {
        // FM with ratio 3.5 gives the inharmonic shimmer of a bell, fading as it rings.
        Timbre::Bell => (phase + 0.9 * (1.0 - progress) * (3.5 * phase).sin()).sin(),
        // A knock: the fundamental and a quickly vanishing fourth partial.
        Timbre::Wood => 0.8 * phase.sin() + 0.4 * (1.0 - progress).powi(4) * (4.0 * phase).sin(),
        Timbre::Soft => phase.sin(),
        // Strong FM at ratio 1: many harmonics, a reedy buzz.
        Timbre::Buzz => 0.8 * (phase + 2.2 * phase.sin()).sin(),
        Timbre::Tick => (2.0 * phase).sin(),
    }
}

/// `tone` as mono samples at `sample_rate`.
pub fn render(tone: ToneId, sample_rate: u32) -> Vec<f32> {
    let rate = sample_rate as f32;
    let mut out = Vec::new();
    for (i, n) in notes(tone).iter().enumerate() {
        if i > 0 {
            out.resize(out.len() + samples_for(GAP_MILLIS, rate), 0.0);
        }
        let from = BASE_HZ * 2f32.powf(f32::from(n.step) / 12.0);
        let to = BASE_HZ * 2f32.powf(f32::from(n.to) / 12.0);
        let count = samples_for(n.millis, rate);
        let (attack_ms, decay_to) = envelope_shape(n.timbre);
        let attack = (attack_ms / 1000.0 * rate).max(1.0);
        let decay = decay_to / count as f32;
        let mut phase = 0.0f32;
        out.extend((0..count).map(|k| {
            let progress = k as f32 / count as f32;
            // Glides move evenly in pitch: exponentially in frequency.
            let hz = from * (to / from).powf(progress);
            phase = (phase + TAU * hz / rate) % (TAU * 64.0);
            let envelope = (k as f32 / attack).min(1.0) * (-decay * k as f32).exp();
            LEVEL * n.gain * envelope * voice(n.timbre, phase, progress)
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

/// The samples of `tone` at `rate`: the own tone assigned on this device (FR-045a), otherwise the
/// built-in one.
pub fn samples_of(app: &AppHandle, tone: ToneId, rate: u32) -> Vec<f32> {
    let own = app.try_state::<ToneBank>().and_then(|bank| bank.get(tone));
    match own {
        Some(sound) => resample_linear(&sound.samples, sound.sample_rate, rate),
        None => render(tone, rate),
    }
}

/// Shows `tone` and plays it into `mixer`, unless tones are off and it is not one that always
/// sounds. The volume is the mixer's tone volume (`assistant.tones.volume`). Returns how long it
/// sounds: an own tone may be longer than the built-in one (up to three seconds), and the
/// assistant listens only once it is over.
pub fn play(app: &AppHandle, mixer: &MixerInput, tone: ToneId, tones_enabled: bool) -> Duration {
    let _ = app.emit(EVENT_TONE, ToneEvent { tone });
    if !(tones_enabled || always_audible(tone)) {
        return Duration::ZERO;
    }
    let rate = mixer.sample_rate();
    let samples = samples_of(app, tone, rate);
    mixer.play_tone(&samples);
    Duration::from_secs_f64(samples.len() as f64 / f64::from(rate.max(1)))
}

/// How often a tone sample looks for a newer sample while it waits.
const SAMPLE_POLL: Duration = Duration::from_millis(20);

/// Plays `samples` (at the mixer's rate) into `mixer` at `volume` percent and waits until they
/// have sounded (the settings page's "Anhören", T030). Returns `false` when `cancel` fired first; a
/// tone always plays to its end, so only the waiting stops. Sets the mixer's tone volume, which is
/// the setting being tried.
pub fn sample(mixer: &MixerInput, samples: &[f32], volume: u8, cancel: &CancellationToken) -> bool {
    mixer.set_tone_volume(volume);
    if !mixer.play_tone(samples) {
        return false;
    }
    let length = Duration::from_secs_f64(samples.len() as f64 / f64::from(mixer.sample_rate()));
    let ends_at = std::time::Instant::now() + length;
    while std::time::Instant::now() < ends_at {
        if cancel.is_cancelled() {
            return false;
        }
        std::thread::sleep(SAMPLE_POLL);
    }
    true
}

/// Plays `tone` once at `volume` (0–100) for the settings page, also while tones are switched off:
/// the own tone `custom_id` when given, otherwise what sounds for `tone` on this device (FR-045a).
/// A newer sample (tone or voice) ends the wait for this one.
#[tauri::command]
pub async fn assistant_tone_preview(
    app: AppHandle,
    state: State<'_, AppState>,
    tone: ToneId,
    volume: u8,
    custom_id: Option<uuid::Uuid>,
) -> Result<()> {
    if volume > 100 {
        return Err(HolziError::InvalidInput {
            reason: format!("tone volume {volume} is above 100"),
        });
    }
    let own = match custom_id {
        Some(id) => Some(custom_tones::stored_sound(&state, id).await?),
        None => {
            // The bank may not be filled yet (nothing has played since the vault opened).
            if app.state::<ToneBank>().get(tone).is_none() {
                let _ = custom_tones::refresh_bank(&app).await;
            }
            None
        }
    };
    let voice = app.state::<VoiceState>();
    let cancel = voice.begin_preview(&state.gate().token());
    let shared = voice.audio_stream().current();
    let gate = state.gate().clone();
    let app_for_samples = app.clone();
    let work = state.gate().spawn_blocking(move || {
        crate::tts::preview::on_speaker(&gate, shared, |mixer| {
            let rate = mixer.sample_rate();
            let samples = match &own {
                Some(sound) => resample_linear(&sound.samples, sound.sample_rate, rate),
                None => samples_of(&app_for_samples, tone, rate),
            };
            Ok(sample(mixer, &samples, volume, &cancel))
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
