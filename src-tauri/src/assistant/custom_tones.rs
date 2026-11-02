//! The user's own tones (spec 066 FR-045a, data-model.md "Eigene Töne", T157, T158).
//!
//! A tone is imported once from an audio file (MP3, WAV, OGG, FLAC, M4A, read with symphonia):
//! mixed to mono, trimmed of silence at both ends, held to [`MAX_DURATION`], its peak matched to
//! the built-in tones, and stored as a 16-bit mono WAV in `haex_assistant_tones`. The original
//! file is not kept. The table syncs, so every device can choose the tone; which tone sounds for
//! which occasion is the device preference [`PREF_CUSTOM`]. A tone that is gone (deleted on
//! another device) leaves the built-in one in its place.
//!
//! [`ToneBank`] holds the sounds of the tones assigned on this device, so `tones::play` can use
//! them without a database read; [`refresh_bank`] fills it after every change.

use std::collections::HashMap;
use std::io::Cursor;
use std::sync::{Arc, RwLock};
use std::time::Duration;

use haex_crdt::rusqlite::params;
use haex_crdt::CrdtTransaction;
use serde::Serialize;
use sha2::{Digest, Sha256};
use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::{DecoderOptions, CODEC_TYPE_NULL};
use symphonia::core::errors::Error as DecodeError;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;
use tauri::{AppHandle, Manager, Runtime, State};
use uuid::Uuid;

use crate::audio::resample::resample_linear;
use crate::error::{HolziError, Result};
use crate::files::picked::{self, PickedFile};
use crate::state::AppState;
use crate::state_utils::active_database;
use crate::storage::preferences::{self, PrefScope};
use crate::storage::query::Query;

use super::tones::ToneId;

/// Device preference: which own tone sounds for which occasion, JSON `{ToneId: id}`; a missing
/// occasion uses the built-in tone (data-model.md).
pub const PREF_CUSTOM: &str = "assistant.tones.custom";
/// The largest file an import reads.
pub const MAX_FILE_BYTES: u64 = 20 * 1024 * 1024;
/// The longest own tone after trimming silence (FR-045a).
pub const MAX_DURATION: Duration = Duration::from_secs(3);
/// "Working" repeats every two seconds, so its own tone may last one (FR-045a).
pub const MAX_WORKING_DURATION: Duration = Duration::from_secs(1);
/// The peak of every own tone: that of the built-in ones, so the tone volume treats them alike.
pub const TARGET_PEAK: f32 = 0.45;
/// Stored sounds have at most this rate.
pub const MAX_RATE: u32 = 48_000;
/// A sample below this counts as silence when trimming (about −50 dBFS).
const SILENCE: f32 = 0.003;
/// How much audio an import decodes at most; anything longer is too long anyway once it carries
/// sound past the first three seconds.
const MAX_DECODE: Duration = Duration::from_secs(30);
/// Fades at both ends after trimming, so a cut never clicks.
const FADE: Duration = Duration::from_millis(5);

/// Why an import was refused (`assistant_tone_import`, contracts/tauri-commands.md).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ImportProblem {
    /// No audio this build can read.
    Unsupported,
    /// Only silence.
    Silent,
    /// Longer than [`MAX_DURATION`] after trimming.
    TooLong,
}

impl ImportProblem {
    fn as_str(self) -> &'static str {
        match self {
            Self::Unsupported => "unsupported",
            Self::Silent => "silent",
            Self::TooLong => "tooLong",
        }
    }
}

impl From<ImportProblem> for HolziError {
    fn from(problem: ImportProblem) -> Self {
        HolziError::InvalidInput {
            reason: problem.as_str().to_owned(),
        }
    }
}

/// Mono samples at a rate.
#[derive(Debug, Clone, PartialEq)]
pub struct Sound {
    pub samples: Vec<f32>,
    pub sample_rate: u32,
}

impl Sound {
    pub fn duration(&self) -> Duration {
        Duration::from_secs_f64(self.samples.len() as f64 / f64::from(self.sample_rate.max(1)))
    }
}

/// Reads any audio file symphonia knows and mixes it to mono. `extension` helps the probe.
pub fn decode(
    bytes: Vec<u8>,
    extension: Option<&str>,
) -> std::result::Result<Sound, ImportProblem> {
    let source = MediaSourceStream::new(Box::new(Cursor::new(bytes)), Default::default());
    let mut hint = Hint::new();
    if let Some(extension) = extension {
        hint.with_extension(extension);
    }
    let probed = symphonia::default::get_probe()
        .format(
            &hint,
            source,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )
        .map_err(|_| ImportProblem::Unsupported)?;
    let mut format = probed.format;
    let track = format
        .tracks()
        .iter()
        .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
        .ok_or(ImportProblem::Unsupported)?;
    let track_id = track.id;
    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .map_err(|_| ImportProblem::Unsupported)?;
    let mut sample_rate = track.codec_params.sample_rate.unwrap_or(0);
    let mut mono = Vec::new();
    // Ends at the end of the stream, or where a stream breaks off: what was read stays.
    while let Ok(packet) = format.next_packet() {
        if packet.track_id() != track_id {
            continue;
        }
        let decoded = match decoder.decode(&packet) {
            Ok(decoded) => decoded,
            Err(DecodeError::DecodeError(_)) => continue,
            Err(_) => break,
        };
        let spec = *decoded.spec();
        sample_rate = spec.rate;
        let channels = spec.channels.count().max(1);
        let mut buffer = SampleBuffer::<f32>::new(decoded.capacity() as u64, spec);
        buffer.copy_interleaved_ref(decoded);
        mono.extend(
            buffer
                .samples()
                .chunks(channels)
                .map(|frame| frame.iter().sum::<f32>() / channels as f32),
        );
        if mono.len() as f64 > MAX_DECODE.as_secs_f64() * f64::from(sample_rate) {
            break;
        }
    }
    if mono.is_empty() || sample_rate == 0 {
        return Err(ImportProblem::Unsupported);
    }
    Ok(Sound {
        samples: mono,
        sample_rate,
    })
}

/// Trims silence at both ends, holds the length to `max`, lowers the rate to [`MAX_RATE`],
/// matches the peak to [`TARGET_PEAK`] and fades both ends.
pub fn prepare(sound: Sound, max: Duration) -> std::result::Result<Sound, ImportProblem> {
    let loud = |s: &f32| s.abs() >= SILENCE;
    let start = sound
        .samples
        .iter()
        .position(loud)
        .ok_or(ImportProblem::Silent)?;
    let end = sound
        .samples
        .iter()
        .rposition(loud)
        .map_or(start, |i| i + 1);
    let mut trimmed = Sound {
        samples: sound.samples[start..end].to_vec(),
        sample_rate: sound.sample_rate,
    };
    if trimmed.duration() > max {
        return Err(ImportProblem::TooLong);
    }
    if trimmed.sample_rate > MAX_RATE {
        trimmed.samples = resample_linear(&trimmed.samples, trimmed.sample_rate, MAX_RATE);
        trimmed.sample_rate = MAX_RATE;
    }
    let peak = trimmed
        .samples
        .iter()
        .fold(0.0f32, |peak, s| peak.max(s.abs()));
    let gain = TARGET_PEAK / peak;
    let fade = ((FADE.as_secs_f64() * f64::from(trimmed.sample_rate)) as usize)
        .min(trimmed.samples.len() / 2)
        .max(1);
    let last = trimmed.samples.len().saturating_sub(1);
    for (i, sample) in trimmed.samples.iter_mut().enumerate() {
        let edge = i.min(last - i);
        let ramp = (edge as f32 / fade as f32).min(1.0);
        *sample *= gain * ramp;
    }
    Ok(trimmed)
}

/// A prepared sound as a 16-bit mono WAV.
pub fn encode_wav(sound: &Sound) -> Vec<u8> {
    let data_len = (sound.samples.len() * 2) as u32;
    let mut out = Vec::with_capacity(44 + data_len as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes()); // PCM
    out.extend_from_slice(&1u16.to_le_bytes()); // mono
    out.extend_from_slice(&sound.sample_rate.to_le_bytes());
    out.extend_from_slice(&(sound.sample_rate * 2).to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for sample in &sound.samples {
        let value = (sample.clamp(-1.0, 1.0) * f32::from(i16::MAX)) as i16;
        out.extend_from_slice(&value.to_le_bytes());
    }
    out
}

/// A stored sound back as samples.
pub fn decode_stored(wav: &[u8]) -> std::result::Result<Sound, ImportProblem> {
    let pcm = crate::tts::voices::decode_wav(wav).map_err(|_| ImportProblem::Unsupported)?;
    let channels = usize::from(pcm.channels.max(1));
    Ok(Sound {
        samples: pcm
            .samples
            .chunks(channels)
            .map(|frame| frame.iter().sum::<f32>() / channels as f32)
            .collect(),
        sample_rate: pcm.sample_rate,
    })
}

/// One own tone as the settings page lists it (`CustomTone` of contracts/tauri-commands.md).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CustomTone {
    pub id: Uuid,
    pub name: String,
    pub duration_ms: i64,
}

/// Stores a prepared sound as a new tone.
pub fn insert(
    tx: &mut CrdtTransaction<'_>,
    name: &str,
    sound: &Sound,
    now: i64,
) -> haex_crdt::Result<CustomTone> {
    let wav = encode_wav(sound);
    let tone = CustomTone {
        id: Uuid::new_v4(),
        name: name.to_owned(),
        duration_ms: sound.duration().as_millis() as i64,
    };
    let sha = crate::file_sync::place_device::hex(&Sha256::digest(&wav));
    tx.execute(
        "INSERT INTO haex_assistant_tones \
           (id, name, duration_ms, sample_rate, sha256, sound, created_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            tone.id.to_string(),
            tone.name,
            tone.duration_ms,
            sound.sample_rate,
            sha,
            wav,
            now
        ],
    )?;
    Ok(tone)
}

/// Every own tone, by name.
pub fn list(q: &mut impl Query) -> haex_crdt::Result<Vec<CustomTone>> {
    q.query_map(
        "SELECT id, name, duration_ms FROM haex_assistant_tones ORDER BY name COLLATE NOCASE, id",
        &[],
        |row| {
            Ok(CustomTone {
                id: crate::storage::uuid::from_row(row, 0)?,
                name: row.get(1)?,
                duration_ms: row.get(2)?,
            })
        },
    )
}

/// The stored WAV of a tone, if it exists.
pub fn sound_of(q: &mut impl Query, id: Uuid) -> haex_crdt::Result<Option<Vec<u8>>> {
    q.query_row(
        "SELECT sound FROM haex_assistant_tones WHERE id = ?1",
        params![id.to_string()],
        |row| row.get(0),
    )
}

/// Deletes a tone everywhere. Returns whether it existed.
pub fn delete(tx: &mut CrdtTransaction<'_>, id: Uuid) -> haex_crdt::Result<bool> {
    Ok(tx.execute(
        "DELETE FROM haex_assistant_tones WHERE id = ?1",
        params![id.to_string()],
    )? > 0)
}

/// Reads [`PREF_CUSTOM`]; anything unreadable counts as "all built-in", unknown occasions and
/// ids are dropped.
pub fn parse_assigned(value: Option<&str>) -> HashMap<ToneId, Uuid> {
    let Some(value) = value else {
        return HashMap::new();
    };
    let Ok(map) = serde_json::from_str::<HashMap<String, String>>(value) else {
        return HashMap::new();
    };
    map.into_iter()
        .filter_map(|(tone, id)| {
            let tone = serde_json::from_value::<ToneId>(serde_json::Value::String(tone)).ok()?;
            Some((tone, Uuid::parse_str(&id).ok()?))
        })
        .collect()
}

/// [`PREF_CUSTOM`] for `assigned`, with the occasions in a stable order.
pub fn format_assigned(assigned: &HashMap<ToneId, Uuid>) -> String {
    let ordered: std::collections::BTreeMap<String, String> = assigned
        .iter()
        .filter_map(|(tone, id)| {
            let name = serde_json::to_value(tone).ok()?.as_str()?.to_owned();
            Some((name, id.to_string()))
        })
        .collect();
    serde_json::to_string(&ordered).unwrap_or_else(|_| "{}".to_owned())
}

/// The sounds of the tones assigned on this device, ready for `tones::play`.
#[derive(Default)]
pub struct ToneBank {
    sounds: RwLock<HashMap<ToneId, Arc<Sound>>>,
}

impl ToneBank {
    /// The own sound for `tone`, if one is assigned and exists.
    pub fn get(&self, tone: ToneId) -> Option<Arc<Sound>> {
        self.sounds
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .get(&tone)
            .cloned()
    }

    fn replace(&self, sounds: HashMap<ToneId, Arc<Sound>>) {
        *self.sounds.write().unwrap_or_else(|e| e.into_inner()) = sounds;
    }

    /// Forgets every sound (the vault closed).
    pub fn clear(&self) {
        self.replace(HashMap::new());
    }
}

/// Reads this device's assignment and the assigned sounds into the [`ToneBank`].
pub async fn refresh_bank<R: Runtime>(app: &AppHandle<R>) -> Result<()> {
    let state = app.state::<AppState>();
    let (assigned, _) = read_assigned(app, &state).await?;
    let db = active_database(&state)?;
    let ids: Vec<(ToneId, Uuid)> = assigned.into_iter().collect();
    let stored = db
        .read(move |r| {
            let mut found = Vec::new();
            for (tone, id) in ids {
                if let Some(wav) = sound_of(r, id)? {
                    found.push((tone, wav));
                }
            }
            Ok::<_, haex_crdt::Error>(found)
        })
        .await?;
    let sounds = stored
        .into_iter()
        .filter_map(|(tone, wav)| Some((tone, Arc::new(decode_stored(&wav).ok()?))))
        .collect();
    app.state::<ToneBank>().replace(sounds);
    Ok(())
}

/// Loads and follows the assigned tones for the vault session that just opened. Assignments are
/// device preferences and sounds are synced vault rows, so either table can change on another
/// device while this vault is open. Forget the cached audio as soon as the vault closes.
pub fn start_bank_feed<R: Runtime>(app: &AppHandle<R>, state: &AppState) {
    if app.try_state::<ToneBank>().is_none() {
        return;
    }
    let changes = state.vault_changes().subscribe();
    let token = state.gate().token();
    let gate = state.gate().clone();
    let app = app.clone();
    if let Err(error) = gate.spawn(async move {
        if let Err(error) = refresh_bank(&app).await {
            log::warn!("assistant tones: could not load assigned tones: {error}");
        }
        let mut changes = changes;
        loop {
            tokio::select! {
                () = token.cancelled() => {
                    if let Some(bank) = app.try_state::<ToneBank>() {
                        bank.clear();
                    }
                    return;
                }
                event = changes.recv() => match event {
                    Ok(tables) if tables.iter().any(|table| {
                        table == "haex_assistant_tones" || table == "preferences"
                    }) => {
                        if let Err(error) = refresh_bank(&app).await {
                            log::warn!("assistant tones: could not refresh assigned tones: {error}");
                        }
                    }
                    Ok(_) => {}
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                        // We may have skipped an assignment or tone write.
                        if let Err(error) = refresh_bank(&app).await {
                            log::warn!("assistant tones: could not refresh assigned tones: {error}");
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => return,
                }
            }
        }
    }) {
        log::warn!("assistant tones: the tone feed did not start: {error}");
    }
}

async fn read_assigned<R: Runtime>(
    app: &AppHandle<R>,
    state: &State<'_, AppState>,
) -> Result<(HashMap<ToneId, Uuid>, PrefScope)> {
    let db = active_database(state)?;
    let device = crate::device::commands::resolve_vault_device_uuid(app, &db).await?;
    let scope = PrefScope::Device(device);
    let value = db
        .read(move |r| preferences::get(r, scope, PREF_CUSTOM))
        .await?;
    Ok((parse_assigned(value.as_deref()), scope))
}

/// `assistant_tones_list` answer.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TonesOverview {
    pub custom: Vec<CustomTone>,
    /// Occasions with an own tone that still exists on this device's view of the vault.
    pub assigned: HashMap<ToneId, Uuid>,
}

#[tauri::command]
pub async fn assistant_tones_list(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<TonesOverview> {
    let (mut assigned, _) = read_assigned(&app, &state).await?;
    let db = active_database(&state)?;
    let custom = db.read(|r| list(r)).await?;
    assigned.retain(|_, id| custom.iter().any(|t| t.id == *id));
    Ok(TonesOverview { custom, assigned })
}

/// Imports the chosen file as a new own tone (not yet assigned to anything).
#[tauri::command]
pub async fn assistant_tone_import(
    app: AppHandle,
    state: State<'_, AppState>,
    file: PickedFile,
) -> Result<CustomTone> {
    let name = picked::display_name(&app, &file);
    let bytes = picked::read(&app, &file, MAX_FILE_BYTES)?;
    if bytes.len() as u64 > MAX_FILE_BYTES {
        return Err(HolziError::InvalidInput {
            reason: "tooLarge".into(),
        });
    }
    let (stem, extension) = match name.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() => (stem.to_owned(), Some(ext.to_ascii_lowercase())),
        _ => (name.clone(), None),
    };
    let sound = state
        .gate()
        .spawn_blocking(move || {
            decode(bytes, extension.as_deref()).and_then(|s| prepare(s, MAX_DURATION))
        })?
        .await
        .map_err(|e| HolziError::SpeechFailed {
            reason: format!("prepare the tone: {e}"),
        })??;
    let db = active_database(&state)?;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64);
    let tone = db.write(move |tx| insert(tx, &stem, &sound, now)).await?;
    Ok(tone)
}

/// Assigns an own tone to `tone` on this device, or the built-in one again (`custom_id: None`).
#[tauri::command]
pub async fn assistant_tone_assign(
    app: AppHandle,
    state: State<'_, AppState>,
    tone: ToneId,
    custom_id: Option<Uuid>,
) -> Result<()> {
    let (mut assigned, scope) = read_assigned(&app, &state).await?;
    let db = active_database(&state)?;
    match custom_id {
        Some(id) => {
            let wav = db.read(move |r| sound_of(r, id)).await?.ok_or_else(|| {
                HolziError::InvalidInput {
                    reason: "unknownTone".into(),
                }
            })?;
            if tone == ToneId::Working {
                let sound = decode_stored(&wav)?;
                if sound.duration() > MAX_WORKING_DURATION {
                    return Err(ImportProblem::TooLong.into());
                }
            }
            assigned.insert(tone, id);
        }
        None => {
            assigned.remove(&tone);
        }
    }
    save_assigned(&state, scope, &assigned).await?;
    refresh_bank(&app).await
}

/// Deletes an own tone everywhere and takes it out of this device's assignment.
#[tauri::command]
pub async fn assistant_tone_delete(
    app: AppHandle,
    state: State<'_, AppState>,
    custom_id: Uuid,
) -> Result<()> {
    let (mut assigned, scope) = read_assigned(&app, &state).await?;
    let db = active_database(&state)?;
    db.write(move |tx| delete(tx, custom_id)).await?;
    let before = assigned.len();
    assigned.retain(|_, id| *id != custom_id);
    if assigned.len() != before {
        save_assigned(&state, scope, &assigned).await?;
    }
    refresh_bank(&app).await
}

async fn save_assigned(
    state: &State<'_, AppState>,
    scope: PrefScope,
    assigned: &HashMap<ToneId, Uuid>,
) -> Result<()> {
    let db = active_database(state)?;
    let value = format_assigned(assigned);
    db.write(move |tx| preferences::insert_or_update(tx, scope, PREF_CUSTOM, &value).map(|_| ()))
        .await?;
    Ok(())
}

/// The sound `assistant_tone_preview` plays for `custom_id`.
pub async fn stored_sound(state: &State<'_, AppState>, custom_id: Uuid) -> Result<Sound> {
    let db = active_database(state)?;
    let wav = db
        .read(move |r| sound_of(r, custom_id))
        .await?
        .ok_or_else(|| HolziError::InvalidInput {
            reason: "unknownTone".into(),
        })?;
    Ok(decode_stored(&wav)?)
}
