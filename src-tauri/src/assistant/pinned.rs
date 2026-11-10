//! A model file the assistant downloads without a choice, pinned by Hugging Face revision, size and
//! SHA-256: the speaker model (FR-016a) and the wake word models (FR-011). A file counts as
//! installed once it was checked against its hash; the check is recorded next to it, so it is not
//! hashed again on every start.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use tauri::{AppHandle, Emitter};

use crate::error::{HolziError, Result};
use crate::models::download::download_to_file;
use crate::models::hash::sha256_file;

/// The lock of a destination: two downloads of one file would write the same `.part` file, and a
/// retry would truncate what the other had already checked.
fn lock_of(path: &Path) -> Arc<tokio::sync::Mutex<()>> {
    static LOCKS: OnceLock<Mutex<HashMap<PathBuf, Arc<tokio::sync::Mutex<()>>>>> = OnceLock::new();
    let mut locks = LOCKS
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    Arc::clone(locks.entry(path.to_path_buf()).or_default())
}

#[derive(Debug, Clone, Copy)]
pub struct PinnedFile {
    /// `models::paths` slug of the folder the file goes in; also the `modelId` of its progress.
    pub slug: &'static str,
    pub repo: &'static str,
    pub revision: &'static str,
    pub file: &'static str,
    pub bytes: u64,
    pub sha256: &'static str,
    /// What the file is, for errors ("the speaker model").
    pub label: &'static str,
}

impl PinnedFile {
    pub fn path(&self, app: &AppHandle) -> Result<PathBuf> {
        crate::models::paths::model_file_path(app, self.slug, self.file)
    }

    fn checked_marker(path: &Path) -> PathBuf {
        let mut name = path.file_name().unwrap_or_default().to_os_string();
        name.push(".sha256");
        path.with_file_name(name)
    }

    /// Whether the file is there and was checked against its pinned hash.
    pub fn is_installed(&self, app: &AppHandle) -> bool {
        self.path(app).is_ok_and(|path| self.is_installed_at(&path))
    }

    pub fn is_installed_at(&self, path: &Path) -> bool {
        std::fs::read_to_string(Self::checked_marker(path)).is_ok_and(|h| h.trim() == self.sha256)
            && std::fs::metadata(path).is_ok_and(|m| m.len() == self.bytes)
    }

    fn failed(&self, what: &str, error: impl std::fmt::Display) -> HolziError {
        HolziError::SpeechFailed {
            reason: format!("{what} {}: {error}", self.label),
        }
    }

    /// Downloads the file unless it is installed, reporting progress as the shared
    /// `model-download-progress` with `modelId` = `progress_id` and `total` bytes, then checks it.
    pub async fn download(&self, app: &AppHandle, progress_id: &str, total: u64) -> Result<()> {
        if self.is_installed(app) {
            return Ok(());
        }
        self.download_with(app, progress_id, 0, total).await
    }

    /// [`Self::download`] for one file of several that report as one: `before` bytes of the
    /// others are already counted in the progress.
    pub async fn download_with(
        &self,
        app: &AppHandle,
        progress_id: &str,
        before: u64,
        total: u64,
    ) -> Result<()> {
        if self.is_installed(app) {
            return Ok(());
        }
        let path = self.path(app)?;
        // From the check of the installation to the marker, one download of this file at a time; the
        // second one finds the file installed and returns.
        let lock = lock_of(&path);
        let _turn = lock.lock().await;
        if self.is_installed_at(&path) {
            return Ok(());
        }
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| self.failed("create the folder of", e))?;
        }
        let url = format!(
            "https://huggingface.co/{}/resolve/{}/{}",
            self.repo, self.revision, self.file
        );
        let progress_id = progress_id.to_owned();
        download_to_file(&url, path.clone(), |progress| {
            let _ = app.emit(
                crate::models::commands::EVENT_PROGRESS,
                crate::models::commands::ProgressEvent {
                    model_id: progress_id.clone(),
                    bytes_downloaded: before + progress.bytes_downloaded,
                    bytes_total: Some(total),
                },
            );
        })
        .await
        .map_err(|e| self.failed("download", e))?;
        let checked = path.clone();
        let hash = tauri::async_runtime::spawn_blocking(move || sha256_file(&checked))
            .await
            .map_err(|e| self.failed("check", e))?
            .map_err(|e| self.failed("check", e))?;
        if hash != self.sha256 {
            let _ = std::fs::remove_file(&path);
            return Err(HolziError::SpeechFailed {
                reason: format!("{} is damaged (its checksum does not match)", self.label),
            });
        }
        std::fs::write(Self::checked_marker(&path), self.sha256)
            .map_err(|e| self.failed("record the check of", e))
    }
}
