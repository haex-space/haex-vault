//! Parakeet TDT 0.6B v3 through sherpa-onnx (spec 066, research R7): the second local STT engine
//! next to candle Whisper. It runs fully on the device and knows 25 European languages; it does not
//! report which one it heard (research R22).

use std::path::Path;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use sherpa_onnx::{
    OfflineModelConfig, OfflineRecognizer, OfflineRecognizerConfig, OfflineTransducerModelConfig,
};
use tauri::{AppHandle, Manager};

use crate::models::paths as model_paths;
use crate::state::AppState;

use super::catalog::SttCatalogEntry;
use super::files::{ensure_model_files, resolve_model_dir};
use super::{CanonicalPcm, SttAdapter, SttError, SAMPLE_RATE_HZ};

/// The files the recognizer is pointed at: encoder, decoder, joiner, tokens. An export with
/// external encoder weights keeps them next to the encoder, where onnxruntime finds them.
const RECOGNIZER_FILES: [&str; 4] = [
    "encoder.int8.onnx",
    "decoder.int8.onnx",
    "joiner.int8.onnx",
    "tokens.txt",
];

/// More threads stop paying off for one utterance at a time.
const MAX_THREADS: usize = 4;

pub struct ParakeetAdapter {
    recognizer: Arc<Mutex<OfflineRecognizer>>,
}

impl ParakeetAdapter {
    /// Ensures `entry`'s files are present under its `models::paths` slug directory (downloading
    /// any missing one) and loads it, like `LocalWhisperAdapter::load`.
    pub async fn load(app: &AppHandle, entry: &SttCatalogEntry) -> Result<Self, SttError> {
        let app_for_dir = app.clone();
        let entry_for_dir = entry.clone();
        let dir = tauri::async_runtime::spawn_blocking(move || {
            resolve_model_dir(&app_for_dir, &entry_for_dir)
        })
        .await
        .map_err(|e| SttError::LocalUnavailable {
            reason: format!("resolve Parakeet model directory: {e}"),
        })??;
        let token = app.state::<AppState>().gate().token();
        let publication_lock = model_paths::acquire_model_publication_lock(app, &entry.id, &token)
            .await
            .map_err(|error| SttError::LocalUnavailable {
                reason: format!("acquire Parakeet model lock: {error}"),
            })?;
        ensure_model_files(&dir, entry).await?;
        drop(publication_lock);
        tauri::async_runtime::spawn_blocking(move || Self::load_from_dir(&dir))
            .await
            .map_err(|e| SttError::LocalUnavailable {
                reason: format!("Parakeet load task: {e}"),
            })?
    }

    /// Loads the model from `dir`, which holds [`RECOGNIZER_FILES`].
    pub fn load_from_dir(dir: &Path) -> Result<Self, SttError> {
        let file = |name: &str| -> Result<String, SttError> {
            let path = dir.join(name);
            path.is_file()
                .then(|| path.to_str().map(str::to_owned))
                .flatten()
                .ok_or_else(|| SttError::LocalUnavailable {
                    reason: format!("missing {name}"),
                })
        };
        let threads = std::thread::available_parallelism()
            .map(|n| n.get().min(MAX_THREADS))
            .unwrap_or(1);
        let config = OfflineRecognizerConfig {
            model_config: OfflineModelConfig {
                transducer: OfflineTransducerModelConfig {
                    encoder: Some(file(RECOGNIZER_FILES[0])?),
                    decoder: Some(file(RECOGNIZER_FILES[1])?),
                    joiner: Some(file(RECOGNIZER_FILES[2])?),
                },
                tokens: Some(file(RECOGNIZER_FILES[3])?),
                model_type: Some("nemo_transducer".to_owned()),
                num_threads: threads as i32,
                ..Default::default()
            },
            decoding_method: Some("greedy_search".to_owned()),
            ..Default::default()
        };
        let recognizer =
            OfflineRecognizer::create(&config).ok_or_else(|| SttError::LocalUnavailable {
                reason: "load Parakeet model".to_owned(),
            })?;
        Ok(Self {
            recognizer: Arc::new(Mutex::new(recognizer)),
        })
    }
}

#[async_trait]
impl SttAdapter for ParakeetAdapter {
    async fn transcribe(&self, audio: &CanonicalPcm) -> Result<String, SttError> {
        let recognizer = Arc::clone(&self.recognizer);
        let samples = audio.samples.clone();
        tauri::async_runtime::spawn_blocking(move || {
            let recognizer = recognizer.lock().map_err(|e| SttError::Failed {
                reason: format!("Parakeet state poisoned: {e}"),
            })?;
            let stream = recognizer.create_stream();
            stream.accept_waveform(SAMPLE_RATE_HZ as i32, &samples);
            recognizer.decode(&stream);
            let result = stream.get_result().ok_or_else(|| SttError::Failed {
                reason: "Parakeet returned no result".to_owned(),
            })?;
            Ok(result.text.trim().to_owned())
        })
        .await
        .map_err(|e| SttError::Failed {
            reason: format!("Parakeet inference task: {e}"),
        })?
    }
}
