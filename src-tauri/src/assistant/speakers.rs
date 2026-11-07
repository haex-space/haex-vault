//! Only enrolled voices (spec 066 FR-016a, research R24): haex follows the people enrolled in the
//! vault and nobody else.
//!
//! A person is enrolled by reading a few short sentences; each recording becomes an embedding (a
//! voice print) through 3D-Speaker's CAM++ model (VoxCeleb, Apache-2.0) in sherpa-onnx, and the
//! normalized mean of them is stored (`storage::assistant_speakers`, synced). During a
//! conversation every utterance is compared with the stored prints by cosine similarity; one that
//! matches nobody changes nothing. No recording is kept: the clone session that recorded a
//! sentence ends once its embedding is taken.
//!
//! The model (29.6 MB) is downloaded on first use, pinned by revision and SHA-256.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};

use serde::Serialize;
use sherpa_onnx::{SpeakerEmbeddingExtractor, SpeakerEmbeddingExtractorConfig};
use tauri::{AppHandle, Emitter, Manager, State};
use uuid::Uuid;

use crate::error::{HolziError, Result};
use crate::models::download::download_to_file;
use crate::models::hash::sha256_file;
use crate::state::AppState;
use crate::state_utils::active_database;
use crate::storage::assistant_speakers::{self, Speaker, MAX_NAME_CHARS};
use crate::stt::SAMPLE_RATE_HZ;

/// The embedding model, as noted with every print.
pub const MODEL_ID: &str = "campplus-en-voxceleb";
const MODEL_REPO: &str = "csukuangfj/speaker-embedding-models";
const MODEL_REVISION: &str = "0743f301363dec56491a490f6d6cbc9d67f9a3bf";
const MODEL_FILE: &str = "3dspeaker_speech_campplus_sv_en_voxceleb_16k.onnx";
pub const MODEL_BYTES: u64 = 29_596_978;
const MODEL_SHA256: &str = "357a834f702b80161e5b981182c038e18553c1f2ca752ed6cec2052365d4129b";
/// `models::paths` slug of the model.
const MODEL_SLUG: &str = "speaker-campplus";

/// Cosine similarity from which an utterance counts as an enrolled person's (research R24; tuned
/// by the SC-018 measurement, T177).
pub const MATCH_THRESHOLD: f32 = 0.5;
/// Every enrolment sentence must be at least this similar to the mean of all of them; otherwise
/// another person or noise got into one.
pub const CONSISTENT_THRESHOLD: f32 = 0.45;
/// Sentences an enrolment needs.
pub const ENROL_UTTERANCES: usize = 3;

/// `v` scaled to length 1 (unchanged when it is all zeros).
pub fn normalized(v: &[f32]) -> Vec<f32> {
    let length = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if length == 0.0 {
        return v.to_vec();
    }
    v.iter().map(|x| x / length).collect()
}

/// Cosine similarity of two prints; 0 when their lengths differ or one is all zeros.
pub fn cosine(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let (a, b) = (normalized(a), normalized(b));
    a.iter().zip(&b).map(|(x, y)| x * y).sum()
}

/// The print of several embeddings of one person: the normalized mean of their normalized forms.
pub fn mean(embeddings: &[Vec<f32>]) -> Vec<f32> {
    let Some(first) = embeddings.first() else {
        return Vec::new();
    };
    let mut sum = vec![0.0; first.len()];
    for e in embeddings.iter().filter(|e| e.len() == first.len()) {
        for (s, x) in sum.iter_mut().zip(normalized(e)) {
            *s += x;
        }
    }
    normalized(&sum)
}

/// Whether every embedding is close enough to the mean of all ([`CONSISTENT_THRESHOLD`]).
pub fn consistent(embeddings: &[Vec<f32>]) -> bool {
    let print = mean(embeddings);
    embeddings
        .iter()
        .all(|e| cosine(e, &print) >= CONSISTENT_THRESHOLD)
}

/// The enrolled person `embedding` belongs to, with the similarity: the best match at or above
/// `threshold`, prints of another model left out.
pub fn best_match(embedding: &[f32], prints: &[Speaker], threshold: f32) -> Option<(Uuid, f32)> {
    prints
        .iter()
        .filter(|p| p.model == MODEL_ID)
        .map(|p| (p.id, cosine(embedding, &p.embedding)))
        .filter(|(_, score)| *score >= threshold)
        .max_by(|a, b| a.1.total_cmp(&b.1))
}

/// The embedding model, loaded.
pub struct Extractor(SpeakerEmbeddingExtractor);

impl Extractor {
    pub fn load(path: &Path) -> Result<Self> {
        let config = SpeakerEmbeddingExtractorConfig {
            model: Some(path.to_string_lossy().into_owned()),
            num_threads: 1,
            ..Default::default()
        };
        SpeakerEmbeddingExtractor::create(&config)
            .map(Self)
            .ok_or_else(|| HolziError::SpeechFailed {
                reason: "the speaker model could not be loaded".to_owned(),
            })
    }

    /// The embedding of `samples` (16 kHz mono); `None` when they are too short for one.
    pub fn embed(&self, samples: &[f32]) -> Option<Vec<f32>> {
        let stream = self.0.create_stream()?;
        stream.accept_waveform(SAMPLE_RATE_HZ as i32, samples);
        stream.input_finished();
        if !self.0.is_ready(&stream) {
            return None;
        }
        self.0.compute(&stream)
    }
}

/// Who may speak in a conversation: the prints of the enrolled people and the model to compare
/// with. Without one, everyone may.
pub struct Gate {
    extractor: Arc<Extractor>,
    prints: Vec<Speaker>,
}

impl Gate {
    /// Whether `samples` (16 kHz mono) are an enrolled person's voice. What gives no embedding
    /// (too short) is nobody's.
    pub fn admits(&self, samples: &[f32]) -> bool {
        self.extractor
            .embed(samples)
            .and_then(|e| best_match(&e, &self.prints, MATCH_THRESHOLD))
            .is_some()
    }
}

/// The device setting: follow only enrolled voices (default on while someone is enrolled).
pub const PREF_ENROLLED_ONLY: &str = "assistant.enrolledOnly";

/// The gate of a conversation starting now: `None` when nobody is enrolled, the setting is off,
/// or the model is not there (then everyone may speak, as before FR-016a).
pub async fn gate(app: &AppHandle) -> Result<Option<Gate>> {
    let state = app.state::<AppState>();
    let db = active_database(&state)?;
    let prints = db.read(|r| assistant_speakers::list(r)).await?;
    if prints.is_empty() {
        return Ok(None);
    }
    let device = crate::device::commands::resolve_vault_device_uuid(app, &db).await?;
    let setting = db
        .read(move |r| {
            crate::storage::preferences::get(
                r,
                crate::storage::preferences::PrefScope::Device(device),
                PREF_ENROLLED_ONLY,
            )
        })
        .await?;
    if setting.is_some_and(|v| matches!(v.trim(), "false" | "0")) {
        return Ok(None);
    }
    if prints.iter().all(|p| p.model != MODEL_ID) {
        return Err(HolziError::SpeechFailed {
            reason: "the enrolled voice prints use an older model; enrol them again".to_owned(),
        });
    }
    if !is_installed(app) {
        return Err(HolziError::SpeechFailed {
            reason: "the speaker model is missing; download it again before listening".to_owned(),
        });
    }
    Ok(Some(Gate {
        extractor: extractor(app).await?,
        prints,
    }))
}

/// The loaded model, kept for the process.
#[derive(Default)]
pub struct SpeakerModel(Mutex<Option<Arc<Extractor>>>);

async fn extractor(app: &AppHandle) -> Result<Arc<Extractor>> {
    let held = app.state::<SpeakerModel>();
    if let Some(e) = held
        .0
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone()
    {
        return Ok(e);
    }
    let path = model_path(app)?;
    let loaded = tauri::async_runtime::spawn_blocking(move || Extractor::load(&path))
        .await
        .map_err(|e| HolziError::SpeechFailed {
            reason: format!("load the speaker model: {e}"),
        })??;
    let loaded = Arc::new(loaded);
    *held.0.lock().unwrap_or_else(PoisonError::into_inner) = Some(Arc::clone(&loaded));
    Ok(loaded)
}

fn model_path(app: &AppHandle) -> Result<PathBuf> {
    crate::models::paths::model_file_path(app, MODEL_SLUG, MODEL_FILE)
}

fn checked_marker(path: &Path) -> PathBuf {
    path.with_extension("onnx.sha256")
}

/// Whether the model is downloaded and was checked against its pinned hash.
pub fn is_installed(app: &AppHandle) -> bool {
    model_path(app).is_ok_and(|path| {
        std::fs::read_to_string(checked_marker(&path)).is_ok_and(|h| h.trim() == MODEL_SHA256)
            && std::fs::metadata(&path).is_ok_and(|m| m.len() == MODEL_BYTES)
    })
}

async fn download(app: &AppHandle) -> Result<()> {
    if is_installed(app) {
        return Ok(());
    }
    let path = model_path(app)?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| HolziError::SpeechFailed {
            reason: format!("create the speaker model folder: {e}"),
        })?;
    }
    let url = format!("https://huggingface.co/{MODEL_REPO}/resolve/{MODEL_REVISION}/{MODEL_FILE}");
    download_to_file(&url, path.clone(), |progress| {
        let _ = app.emit(
            crate::models::commands::EVENT_PROGRESS,
            crate::models::commands::ProgressEvent {
                model_id: MODEL_SLUG.to_owned(),
                bytes_downloaded: progress.bytes_downloaded,
                bytes_total: Some(MODEL_BYTES),
            },
        );
    })
    .await
    .map_err(|e| HolziError::SpeechFailed {
        reason: format!("download the speaker model: {e}"),
    })?;
    let checked = path.clone();
    let hash = tauri::async_runtime::spawn_blocking(move || sha256_file(&checked))
        .await
        .map_err(|e| HolziError::SpeechFailed {
            reason: format!("check the speaker model: {e}"),
        })?
        .map_err(|e| HolziError::SpeechFailed {
            reason: format!("check the speaker model: {e}"),
        })?;
    if hash != MODEL_SHA256 {
        let _ = std::fs::remove_file(&path);
        return Err(HolziError::SpeechFailed {
            reason: "the speaker model is damaged (its checksum does not match)".to_owned(),
        });
    }
    std::fs::write(checked_marker(&path), MODEL_SHA256).map_err(|e| HolziError::SpeechFailed {
        reason: format!("record the check of the speaker model: {e}"),
    })
}

/// An enrolled person as the window sees them.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpeakerWire {
    pub id: Uuid,
    pub name: String,
    pub utterances: i64,
    pub created_at: i64,
    /// Made with another model than this build compares with: enrol again.
    pub outdated: bool,
}

impl From<Speaker> for SpeakerWire {
    fn from(s: Speaker) -> Self {
        Self {
            id: s.id,
            name: s.name,
            utterances: s.utterances,
            created_at: s.created_at,
            outdated: s.model != MODEL_ID,
        }
    }
}

/// The sentences of the enrolment running in this process.
#[derive(Default)]
pub struct Enrolment(Mutex<Vec<Vec<f32>>>);

impl Enrolment {
    fn take(&self) -> Vec<Vec<f32>> {
        std::mem::take(&mut *self.0.lock().unwrap_or_else(PoisonError::into_inner))
    }
}

fn invalid(reason: &str) -> HolziError {
    HolziError::InvalidInput {
        reason: reason.to_owned(),
    }
}

/// The enrolled people (FR-016a).
#[tauri::command]
pub async fn assistant_speakers_list(state: State<'_, AppState>) -> Result<Vec<SpeakerWire>> {
    let db = active_database(&state)?;
    let list = db.read(|r| assistant_speakers::list(r)).await?;
    Ok(list.into_iter().map(SpeakerWire::from).collect())
}

/// Whether the speaker model is there (`{installed, sizeBytes}`).
#[tauri::command]
pub async fn assistant_speaker_model(app: AppHandle) -> Result<serde_json::Value> {
    Ok(serde_json::json!({ "installed": is_installed(&app), "sizeBytes": MODEL_BYTES }))
}

/// Downloads the speaker model; progress as the shared `model-download-progress`
/// (`modelId` "speaker-campplus").
#[tauri::command]
pub async fn assistant_speaker_model_download(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<()> {
    state.gate().run(download(&app)).await?
}

/// Takes the recording of own-voice session `session_id` (`tts_clone_record_*`, one read-aloud
/// sentence) as one sentence of the enrolment and ends that session; nothing of the audio is
/// kept. Returns how many sentences the enrolment has.
#[tauri::command]
pub async fn assistant_speaker_add_sentence(
    app: AppHandle,
    state: State<'_, AppState>,
    session_id: Uuid,
) -> Result<usize> {
    let (wav, _, _) = crate::tts::clone_commands::recording_of(&app, session_id)?;
    crate::tts::clone_commands::end_session(&app, session_id);
    let sound =
        crate::assistant::custom_tones::decode_stored(&wav).map_err(|_| invalid("unreadable"))?;
    let samples = crate::audio::resample::resample_filtered(
        &sound.samples,
        sound.sample_rate,
        SAMPLE_RATE_HZ,
    );
    let extractor = extractor(&app).await?;
    let embedding = state
        .gate()
        .spawn_blocking(move || extractor.embed(&samples))?
        .await
        .map_err(|e| HolziError::SpeechFailed {
            reason: format!("take the voice print: {e}"),
        })?
        .ok_or_else(|| invalid("tooShort"))?;
    let enrolment = app.state::<Enrolment>();
    let mut sentences = enrolment.0.lock().unwrap_or_else(PoisonError::into_inner);
    sentences.push(embedding);
    Ok(sentences.len())
}

/// Forgets the sentences of the running enrolment (the dialog was closed or starts over).
#[tauri::command]
pub async fn assistant_speaker_enrolment_reset(app: AppHandle) -> Result<()> {
    app.state::<Enrolment>().take();
    Ok(())
}

/// Stores the running enrolment as person `name`. Needs [`ENROL_UTTERANCES`] sentences that agree
/// with each other ("inconsistent" otherwise: someone else or noise got into one; start over).
#[tauri::command]
pub async fn assistant_speaker_save(
    app: AppHandle,
    state: State<'_, AppState>,
    name: String,
) -> Result<SpeakerWire> {
    let name = name.trim().to_owned();
    if name.is_empty() || name.chars().count() > MAX_NAME_CHARS {
        return Err(invalid("name"));
    }
    let sentences = app.state::<Enrolment>().take();
    if sentences.len() < ENROL_UTTERANCES {
        return Err(invalid("tooFewSentences"));
    }
    if !consistent(&sentences) {
        return Err(invalid("inconsistent"));
    }
    let print = mean(&sentences);
    let count = sentences.len() as i64;
    let db = active_database(&state)?;
    let now = crate::passwords::clock::unix_millis(std::time::SystemTime::now());
    let stored = db
        .write(move |tx| assistant_speakers::insert(tx, &name, MODEL_ID, &print, count, now))
        .await?;
    Ok(stored.into())
}

/// Renames person `id`.
#[tauri::command]
pub async fn assistant_speaker_rename(
    state: State<'_, AppState>,
    id: Uuid,
    name: String,
) -> Result<()> {
    let name = name.trim().to_owned();
    if name.is_empty() || name.chars().count() > MAX_NAME_CHARS {
        return Err(invalid("name"));
    }
    let db = active_database(&state)?;
    if !db
        .write(move |tx| assistant_speakers::rename(tx, id, &name))
        .await?
    {
        return Err(HolziError::NotFound {
            name: id.to_string(),
        });
    }
    Ok(())
}

/// Removes person `id` on every device.
#[tauri::command]
pub async fn assistant_speaker_delete(state: State<'_, AppState>, id: Uuid) -> Result<()> {
    let db = active_database(&state)?;
    db.write(move |tx| assistant_speakers::delete(tx, id))
        .await?;
    Ok(())
}
