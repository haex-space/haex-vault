//! Where a local STT model lives on disk, whether it is completely there, and how its missing files
//! are fetched (spec 010, spec 066 T133). Shared by every engine so listing, downloading and
//! loading always agree on the same installed state, and compiled without either engine so a
//! model can be listed and downloaded in any build.

use std::path::{Path, PathBuf};

use tauri::AppHandle;

use crate::models::download::{download_to_file, DownloadProgress};
use crate::models::paths as model_paths;

use super::catalog::{SttCatalogEntry, SttEngine};
use super::SttError;

pub const WHISPER_CONFIG: &str = "config.json";
pub const WHISPER_TOKENIZER: &str = "tokenizer.json";
pub const WHISPER_WEIGHTS: &str = "model.safetensors";

/// The files of a candle Whisper checkpoint.
const WHISPER_FILES: [&str; 3] = [WHISPER_CONFIG, WHISPER_TOKENIZER, WHISPER_WEIGHTS];

/// The files of a sherpa-onnx Parakeet export with external encoder weights (the layout of
/// `parakeet-primeline-onnx`).
pub const PARAKEET_FILES: [&str; 5] = [
    "encoder.int8.onnx",
    "encoder.int8.onnx.data",
    "decoder.int8.onnx",
    "joiner.int8.onnx",
    "tokens.txt",
];

/// The files `engine` needs in its model directory.
pub fn files_for(engine: SttEngine) -> &'static [&'static str] {
    match engine {
        SttEngine::CandleWhisper => &WHISPER_FILES,
        SttEngine::SherpaParakeet => &PARAKEET_FILES,
    }
}

/// A file counts as a complete download only if it exists, is a regular
/// file, and has nonzero size — guards against a previous run's
/// interrupted/truncated download (or a hand-deleted/corrupted file) being
/// mistaken for an installed one. No content validation beyond that.
pub fn is_complete_file(path: &Path) -> bool {
    std::fs::metadata(path)
        .map(|m| m.is_file() && m.len() > 0)
        .unwrap_or(false)
}

/// A model directory is complete only when every file its engine needs is
/// complete (see [`is_complete_artifact`]).
pub fn is_complete_model(dir: &Path, engine: SttEngine) -> bool {
    files_for(engine)
        .iter()
        .all(|filename| is_complete_artifact(&dir.join(filename), filename))
}

/// Checks the lightweight JSON metadata files deeply enough to reject a
/// truncated or otherwise corrupt document. Weight files intentionally keep
/// the cheap non-empty check; hashing them on every listing would turn
/// discovery into a full model read.
fn is_complete_artifact(path: &Path, filename: &str) -> bool {
    if !is_complete_file(path) {
        return false;
    }
    if filename.ends_with(".json") {
        return std::fs::read(path)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
            .is_some();
    }
    true
}

/// Resolves `entry`'s canonical `models::paths` slug directory, creating it
/// if missing.
///
/// Called before every disk-touching operation (load, install-status
/// listing, download) so all three agree on the same installed state.
pub fn resolve_model_dir(app: &AppHandle, entry: &SttCatalogEntry) -> Result<PathBuf, SttError> {
    model_paths::slug_dir(app, &entry.id).map_err(|e| SttError::LocalUnavailable {
        reason: format!("resolve model dir for {}: {e}", entry.id),
    })
}

/// Downloads whichever of `entry`'s files isn't already complete under `dir`
/// from its pinned Hugging Face revision. Idempotent — a second call with
/// every file complete makes no network request at all.
///
/// `on_progress` sees the whole model, not one file: the bytes of the files
/// already complete plus the running count of the current one, against the
/// catalog's `approx_size_bytes` (raised to the bytes seen if that falls short).
pub async fn ensure_model_files<F>(
    dir: &Path,
    entry: &SttCatalogEntry,
    mut on_progress: F,
) -> Result<(), SttError>
where
    F: FnMut(DownloadProgress),
{
    let mut done: u64 = 0;
    for filename in files_for(entry.engine) {
        let dest = dir.join(filename);
        if is_complete_artifact(&dest, filename) {
            done += std::fs::metadata(&dest).map(|m| m.len()).unwrap_or(0);
            continue;
        }
        let url = format!(
            "https://huggingface.co/{}/resolve/{}/{filename}",
            entry.hf_repo, entry.hf_revision
        );
        let before = done;
        let downloaded = download_to_file(&url, dest, |progress| {
            let bytes_downloaded = before + progress.bytes_downloaded;
            on_progress(DownloadProgress {
                bytes_downloaded,
                bytes_total: Some(entry.approx_size_bytes.max(bytes_downloaded)),
            });
        })
        .await
        .map_err(|e| SttError::LocalUnavailable {
            reason: format!("download {filename}: {e}"),
        })?;
        done += downloaded.bytes_written;
    }
    Ok(())
}
