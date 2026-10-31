//! Where a speech model lives on disk, whether it is completely there, and how its missing files
//! are fetched (spec 066 T022). Same rules as `stt/files.rs`, whose completeness check this
//! reuses, so the two catalogs never disagree on what "installed" means.

use std::path::{Path, PathBuf};

use tauri::AppHandle;

use crate::models::download::download_to_file;
use crate::models::paths as model_paths;
use crate::stt::files::is_complete_artifact;

use super::catalog::{TtsCatalogEntry, TtsEngine};
use super::TtsError;

pub const POCKET_LM_FLOW: &str = "lm_flow.int8.onnx";
pub const POCKET_LM_MAIN: &str = "lm_main.int8.onnx";
pub const POCKET_ENCODER: &str = "encoder.onnx";
pub const POCKET_DECODER: &str = "decoder.int8.onnx";
pub const POCKET_TEXT_CONDITIONER: &str = "text_conditioner.onnx";
pub const POCKET_VOCAB: &str = "vocab.json";
pub const POCKET_TOKEN_SCORES: &str = "token_scores.json";

/// The files of a Pocket model, named as sherpa-onnx's own export names them.
const POCKET_FILES: [&str; 7] = [
    POCKET_LM_FLOW,
    POCKET_LM_MAIN,
    POCKET_ENCODER,
    POCKET_DECODER,
    POCKET_TEXT_CONDITIONER,
    POCKET_VOCAB,
    POCKET_TOKEN_SCORES,
];

pub fn files_for(engine: TtsEngine) -> &'static [&'static str] {
    match engine {
        TtsEngine::Pocket => &POCKET_FILES,
    }
}

pub fn is_complete_model(dir: &Path, engine: TtsEngine) -> bool {
    files_for(engine)
        .iter()
        .all(|filename| is_complete_artifact(&dir.join(filename), filename))
}

/// `entry`'s `models::paths` slug directory, created if missing.
pub fn resolve_model_dir(app: &AppHandle, entry: &TtsCatalogEntry) -> Result<PathBuf, TtsError> {
    model_paths::slug_dir(app, &entry.id).map_err(|e| TtsError::Unavailable {
        reason: format!("resolve model dir for {}: {e}", entry.id),
    })
}

/// Where `filename` of `entry` is downloaded from: its folder at the pinned revision.
pub fn file_url(entry: &TtsCatalogEntry, filename: &str) -> String {
    format!(
        "https://huggingface.co/{}/resolve/{}/{}/{filename}",
        entry.hf_repo, entry.hf_revision, entry.hf_folder
    )
}

/// Downloads whichever of `entry`'s files is not complete under `dir`. Idempotent. `on_progress`
/// gets the bytes there are so far and the expected total, files already present included.
pub async fn ensure_model_files(
    dir: &Path,
    entry: &TtsCatalogEntry,
    mut on_progress: impl FnMut(u64, u64),
) -> Result<(), TtsError> {
    let total = entry.approx_size_bytes;
    let model_dir = dir.to_path_buf();
    let engine = entry.engine;
    let (mut done, missing) = tokio::task::spawn_blocking(move || {
        let mut done = 0;
        let mut missing = Vec::new();
        for filename in files_for(engine) {
            let path = model_dir.join(filename);
            if is_complete_artifact(&path, filename) {
                done += std::fs::metadata(path).map(|meta| meta.len()).unwrap_or(0);
            } else {
                missing.push(*filename);
            }
        }
        (done, missing)
    })
    .await
    .map_err(|e| TtsError::Unavailable {
        reason: format!("inspect speech model files: {e}"),
    })?;
    on_progress(done, total);
    for filename in missing {
        let dest = dir.join(filename);
        let file = download_to_file(&file_url(entry, filename), dest, |progress| {
            on_progress(done + progress.bytes_downloaded, total);
        })
        .await
        .map_err(|e| TtsError::Unavailable {
            reason: format!("download {filename}: {e}"),
        })?;
        done += file.bytes_written;
        on_progress(done, total);
    }
    Ok(())
}
