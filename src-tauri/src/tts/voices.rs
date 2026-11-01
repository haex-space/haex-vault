//! Built-in voices (spec 066 T140, research R8). Pocket TTS has no voices of its own: it imitates a
//! short reference recording. A built-in voice is such a recording of a native speaker, shipped in
//! the binary next to `voices/voices.json`, two to three per language.
//!
//! The device preference `assistant.voice.<sprache>` holds a [`VoiceRef`]. Whatever it points at
//! that does not exist (any more) speaks with the default voice of the language, so a stale or
//! foreign preference never leaves the assistant mute.

use std::fmt;
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

use super::TtsError;

const VOICES_JSON: &str = include_str!("voices/voices.json");

macro_rules! voice_audio {
    ($($file:literal),* $(,)?) => {
        &[$(($file, include_bytes!(concat!("voices/", $file)) as &[u8])),*]
    };
}

/// The reference recordings by file name; `voices_tests` checks this matches `voices.json`.
const AUDIO: &[(&str, &[u8])] = voice_audio![
    "de-lena.wav",
    "de-jonas.wav",
    "en-caro.wav",
    "en-bill.wav",
    "fr-camille.wav",
    "fr-louis.wav",
    "es-lucia.wav",
    "es-diego.wav",
    "it-giulia.wav",
    "it-marco.wav",
    "pt-ana.wav",
    "pt-pedro.wav",
];

/// Licences a built-in voice may carry (R8: nothing non-commercial).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
pub enum VoiceLicense {
    #[serde(rename = "CC0-1.0")]
    Cc0,
    #[serde(rename = "CC-BY-4.0")]
    CcBy4,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all(serialize = "camelCase", deserialize = "snake_case"))]
pub struct BuiltinVoice {
    /// Stable id, the `<id>` of `builtin:<id>`.
    pub id: String,
    /// Display name; invented for anonymous dataset speakers.
    pub name: String,
    /// ISO 639-1 code, one of [`super::LANGUAGES`].
    pub language: String,
    /// The voice a language speaks with until someone picks another; one per language.
    pub default: bool,
    /// File name in `voices/`.
    pub file: String,
    pub source: String,
    pub source_url: String,
    /// Where exactly the recording comes from, for the provenance in `voices/README.md`.
    pub origin: String,
    pub license: VoiceLicense,
    /// Credit shown with the app's licences.
    pub attribution: String,
}

#[derive(Debug, Deserialize)]
struct RawVoices {
    voices: Vec<BuiltinVoice>,
}

static VOICES: OnceLock<Result<Vec<BuiltinVoice>, String>> = OnceLock::new();

pub fn builtin() -> Result<&'static [BuiltinVoice], TtsError> {
    VOICES
        .get_or_init(|| {
            serde_json::from_str::<RawVoices>(VOICES_JSON)
                .map(|raw| raw.voices)
                .map_err(|e| format!("parse built-in voices: {e}"))
        })
        .as_ref()
        .map(Vec::as_slice)
        .map_err(|reason| TtsError::Failed {
            reason: reason.clone(),
        })
}

/// The reference recording of `voice` (WAV).
pub fn audio(voice: &BuiltinVoice) -> Option<&'static [u8]> {
    AUDIO
        .iter()
        .find(|(file, _)| *file == voice.file)
        .map(|(_, bytes)| *bytes)
}

/// The file names that ship a recording, for the tests.
pub fn audio_files() -> impl Iterator<Item = &'static str> {
    AUDIO.iter().map(|(file, _)| *file)
}

pub fn default_voice(language: &str) -> Result<Option<&'static BuiltinVoice>, TtsError> {
    Ok(builtin()?
        .iter()
        .find(|v| v.language == language && v.default))
}

/// A voice as the preferences store it (data-model.md): `builtin:<id>`, `clone:<id>` (own voice,
/// PR K), `model:<id>` or `model:<id>#<speaker>` (imported model, PR K).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VoiceRef {
    Builtin(String),
    Clone(String),
    Model { id: String, speaker: Option<String> },
}

fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && !id
            .chars()
            .any(|c| c.is_whitespace() || c.is_control() || c == '#')
}

impl VoiceRef {
    pub fn parse(text: &str) -> Option<Self> {
        let (kind, rest) = text.split_once(':')?;
        match kind {
            "builtin" => valid_id(rest).then(|| Self::Builtin(rest.to_owned())),
            "clone" => valid_id(rest).then(|| Self::Clone(rest.to_owned())),
            "model" => {
                let (id, speaker) = match rest.split_once('#') {
                    Some((id, speaker)) => (id, Some(speaker)),
                    None => (rest, None),
                };
                let speaker_ok = speaker.is_none_or(valid_id);
                (valid_id(id) && speaker_ok).then(|| Self::Model {
                    id: id.to_owned(),
                    speaker: speaker.map(str::to_owned),
                })
            }
            _ => None,
        }
    }
}

impl fmt::Display for VoiceRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Builtin(id) => write!(f, "builtin:{id}"),
            Self::Clone(id) => write!(f, "clone:{id}"),
            Self::Model { id, speaker: None } => write!(f, "model:{id}"),
            Self::Model {
                id,
                speaker: Some(speaker),
            } => write!(f, "model:{id}#{speaker}"),
        }
    }
}

/// The voice `language` speaks with when its preference is `preference`. Anything that does not
/// name a built-in voice of that language falls back to the language's default: an unknown or
/// malformed ref, a voice of another language, and for now own voices and imported models, which
/// PR K brings.
pub fn resolve(
    preference: Option<&str>,
    language: &str,
) -> Result<&'static BuiltinVoice, TtsError> {
    let chosen = match preference.and_then(VoiceRef::parse) {
        Some(VoiceRef::Builtin(id)) => builtin()?
            .iter()
            .find(|v| v.id == id && v.language == language),
        _ => None,
    };
    match chosen {
        Some(voice) => Ok(voice),
        None => default_voice(language)?.ok_or_else(|| TtsError::Unavailable {
            reason: format!("no voice speaks {language}"),
        }),
    }
}

/// A decoded reference recording.
#[derive(Debug, Clone)]
pub struct Pcm {
    pub samples: Vec<f32>,
    pub sample_rate: u32,
    pub channels: u16,
}

/// Decodes a 16-bit PCM WAV, the format of the built-in recordings.
pub fn decode_wav(bytes: &[u8]) -> Result<Pcm, TtsError> {
    let bad = |what: &str| TtsError::Failed {
        reason: format!("voice recording: {what}"),
    };
    if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err(bad("not a WAV file"));
    }
    let mut format = None;
    let mut data = None;
    let mut pos = 12;
    while pos + 8 <= bytes.len() {
        let id = &bytes[pos..pos + 4];
        let size = u32::from_le_bytes([
            bytes[pos + 4],
            bytes[pos + 5],
            bytes[pos + 6],
            bytes[pos + 7],
        ]) as usize;
        let body = bytes
            .get(pos + 8..pos + 8 + size)
            .ok_or_else(|| bad("truncated chunk"))?;
        match id {
            b"fmt " if body.len() >= 16 => {
                let tag = u16::from_le_bytes([body[0], body[1]]);
                let channels = u16::from_le_bytes([body[2], body[3]]);
                let rate = u32::from_le_bytes([body[4], body[5], body[6], body[7]]);
                let bits = u16::from_le_bytes([body[14], body[15]]);
                format = Some((tag, channels, rate, bits));
            }
            b"data" => data = Some(body),
            _ => {}
        }
        // Chunks are padded to an even number of bytes.
        pos += 8 + size + (size % 2);
    }
    let (tag, channels, sample_rate, bits) = format.ok_or_else(|| bad("no fmt chunk"))?;
    if tag != 1 || bits != 16 || channels == 0 || sample_rate == 0 {
        return Err(bad("only 16-bit PCM is supported"));
    }
    let data = data.ok_or_else(|| bad("no data chunk"))?;
    let frames = data.as_chunks::<2>().0;
    let samples = frames
        .chunks(usize::from(channels))
        .map(|frame| {
            frame
                .iter()
                .map(|b| f32::from(i16::from_le_bytes(*b)) / 32768.0)
                .sum::<f32>()
                / f32::from(channels)
        })
        .collect();
    Ok(Pcm {
        samples,
        sample_rate,
        channels,
    })
}

/// The reference Pocket imitates for `voice`.
#[cfg(feature = "voice")]
pub fn reference(voice: &BuiltinVoice) -> Result<super::local::Reference, TtsError> {
    let bytes = audio(voice).ok_or_else(|| TtsError::Failed {
        reason: format!("voice {} has no recording", voice.id),
    })?;
    let pcm = decode_wav(bytes)?;
    Ok(super::local::Reference {
        samples: pcm.samples,
        sample_rate: pcm.sample_rate,
    })
}
