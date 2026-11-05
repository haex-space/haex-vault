//! Where a speech model lives on disk, whether it is completely there, and how its missing files
//! are fetched (spec 066 T022). Same rules as `stt/files.rs`, whose completeness check this
//! reuses, so the two catalogs never disagree on what "installed" means. Entries that pin their
//! files by SHA-256 (spec 081 FR-003) are checked once after the download; a marker beside each
//! file records the verified hash, so loading never hashes gigabytes again.

use std::path::{Path, PathBuf};

use tauri::AppHandle;

use crate::models::download::download_to_file;
use crate::models::hash::sha256_file;
use crate::models::paths as model_paths;
use crate::stt::files::is_complete_artifact;

use super::catalog::TtsCatalogEntry;
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

/// The file names of `entry`: its pinned `files` where it has them, the Pocket list otherwise.
pub fn files_for(entry: &TtsCatalogEntry) -> Vec<&str> {
    match &entry.files {
        Some(files) => files.iter().map(|f| f.name.as_str()).collect(),
        None => POCKET_FILES.to_vec(),
    }
}

/// The SHA-256 `entry` pins for `filename`, if it pins one.
fn pinned_hash<'a>(entry: &'a TtsCatalogEntry, filename: &str) -> Option<&'a str> {
    entry
        .files
        .as_ref()?
        .iter()
        .find(|f| f.name == filename)
        .map(|f| f.sha256.as_str())
}

/// The marker beside `path` that records the hash a download was verified against.
fn marker(path: &Path) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(".sha256");
    path.with_file_name(name)
}

/// Whether `filename` of `entry` is complete under `dir`: written in full and, when `entry` pins
/// its hash, verified against it.
fn is_complete_file(dir: &Path, entry: &TtsCatalogEntry, filename: &str) -> bool {
    let path = dir.join(filename);
    if !is_complete_artifact(&path, filename) {
        return false;
    }
    match pinned_hash(entry, filename) {
        Some(hash) => std::fs::read_to_string(marker(&path)).is_ok_and(|seen| seen.trim() == hash),
        None => true,
    }
}

/// Whether every file of `entry` is complete under `dir`.
pub fn is_complete_model(dir: &Path, entry: &TtsCatalogEntry) -> bool {
    files_for(entry)
        .iter()
        .all(|filename| is_complete_file(dir, entry, filename))
}

/// `entry`'s `models::paths` slug directory, created if missing.
pub fn resolve_model_dir(app: &AppHandle, entry: &TtsCatalogEntry) -> Result<PathBuf, TtsError> {
    model_paths::slug_dir(app, &entry.id).map_err(|e| TtsError::Unavailable {
        reason: format!("resolve model dir for {}: {e}", entry.id),
    })
}

/// Where `filename` of `entry` is downloaded from: its folder (or the repository root) at the
/// pinned revision.
pub fn file_url(entry: &TtsCatalogEntry, filename: &str) -> String {
    let folder = if entry.hf_folder.is_empty() {
        String::new()
    } else {
        format!("{}/", entry.hf_folder)
    };
    format!(
        "https://huggingface.co/{}/resolve/{}/{folder}{filename}",
        entry.hf_repo, entry.hf_revision
    )
}

/// Checks the file at `path` against `hash` and writes the marker; a mismatch deletes the file.
pub fn verify_file(path: &Path, filename: &str, hash: &str) -> Result<(), TtsError> {
    let seen = sha256_file(path).map_err(|e| TtsError::Unavailable {
        reason: format!("check {filename}: {e}"),
    })?;
    if seen != hash {
        let _ = std::fs::remove_file(path);
        return Err(TtsError::Unavailable {
            reason: format!("{filename} is damaged (its checksum does not match)"),
        });
    }
    std::fs::write(marker(path), hash).map_err(|e| TtsError::Unavailable {
        reason: format!("record the check of {filename}: {e}"),
    })
}

/// What [`ensure_model_files`] still has to do for one file of an entry.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Pending {
    /// Missing, partly written, or checked against another hash: fetch it.
    Fetch,
    /// Written in full but never checked (the app quit during the check): check it only.
    Check { bytes: u64 },
}

/// The bytes of `entry` already complete under `dir`, and what each other file still needs. A
/// marker of another hash (an earlier pin under the same file name) is removed, so that file is
/// fetched again rather than checked and deleted.
pub(crate) fn pending_files(dir: &Path, entry: &TtsCatalogEntry) -> (u64, Vec<(String, Pending)>) {
    let mut done = 0;
    let mut pending = Vec::new();
    for filename in files_for(entry) {
        let path = dir.join(filename);
        let bytes = || std::fs::metadata(&path).map(|meta| meta.len()).unwrap_or(0);
        if is_complete_file(dir, entry, filename) {
            done += bytes();
            continue;
        }
        let marker = marker(&path);
        let checked_before = marker.exists();
        if checked_before {
            let _ = std::fs::remove_file(&marker);
        }
        let next = if !checked_before && is_complete_artifact(&path, filename) {
            Pending::Check { bytes: bytes() }
        } else {
            Pending::Fetch
        };
        pending.push((filename.to_owned(), next));
    }
    (done, pending)
}

/// Downloads whichever of `entry`'s files is not complete under `dir`, checking pinned hashes.
/// Idempotent. `on_progress` gets the bytes there are so far and the expected total, files
/// already present included.
pub async fn ensure_model_files(
    dir: &Path,
    entry: &TtsCatalogEntry,
    mut on_progress: impl FnMut(u64, u64),
) -> Result<(), TtsError> {
    let total = entry.approx_size_bytes;
    let model_dir = dir.to_path_buf();
    let owned = entry.clone();
    let (mut done, pending) =
        tokio::task::spawn_blocking(move || pending_files(&model_dir, &owned))
            .await
            .map_err(|e| TtsError::Unavailable {
                reason: format!("inspect speech model files: {e}"),
            })?;
    on_progress(done, total);
    for (filename, next) in pending {
        let dest = dir.join(&filename);
        match next {
            Pending::Fetch => {
                let file =
                    download_to_file(&file_url(entry, &filename), dest.clone(), |progress| {
                        on_progress(done + progress.bytes_downloaded, total);
                    })
                    .await
                    .map_err(|e| TtsError::Unavailable {
                        reason: format!("download {filename}: {e}"),
                    })?;
                done += file.bytes_written;
            }
            Pending::Check { bytes } => done += bytes,
        }
        if let Some(hash) = pinned_hash(entry, &filename).map(str::to_owned) {
            let name = filename.clone();
            tokio::task::spawn_blocking(move || verify_file(&dest, &name, &hash))
                .await
                .map_err(|e| TtsError::Unavailable {
                    reason: format!("check {filename}: {e}"),
                })??;
        }
        on_progress(done, total);
    }
    Ok(())
}
