//! Sidecar persistence for resumable chunk-hashed downloads.
//!
//! For each in-progress download we keep two files next to the eventual
//! destination:
//!
//! - `<dest>.haex-partial`      — the partial byte stream being assembled
//! - `<dest>.haex-partial.meta` — JSON-encoded [`PartialState`] tracking
//!   which chunks have already arrived
//!
//! On retry the caller invokes [`PartialState::load_if_matches`] with the
//! manifest's current `file_hash`. If the stored hash matches, the caller
//! resumes by requesting only the [`PartialState::missing_ranges`]; if it
//! doesn't (manifest changed underfoot), `None` is returned and the
//! caller starts from scratch.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tokio::fs;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PartialState {
    pub file_hash: String,
    pub chunk_size: u32,
    pub completed: Vec<bool>,
}

impl PartialState {
    fn meta_path(target: &Path) -> PathBuf {
        let mut p = target.as_os_str().to_owned();
        p.push(".haex-partial.meta");
        PathBuf::from(p)
    }

    pub fn partial_path(target: &Path) -> PathBuf {
        let mut p = target.as_os_str().to_owned();
        p.push(".haex-partial");
        PathBuf::from(p)
    }

    /// Persist the sidecar atomically by writing to a uniquely-named sibling
    /// `.tmp.<nonce>` file first and renaming over the final path. Without
    /// this dance, two concurrent multi-stream workers (or any process crash
    /// mid-write) can land a torn JSON payload that fails to parse on the
    /// next resume. `tokio::fs::rename` is atomic within a filesystem, so a
    /// reader observes either the previous payload or the new one — never a
    /// tear. The per-call nonce keeps two concurrent saves from clobbering
    /// each other's tmp file before either rename completes.
    ///
    /// `load()` additionally treats any unparseable sidecar as missing —
    /// that's the safety net for sidecars written by older versions of this
    /// code that predate the atomic write dance below.
    pub async fn save(&self, target: &Path) -> std::io::Result<()> {
        use std::sync::atomic::{AtomicU64, Ordering};
        static NONCE: AtomicU64 = AtomicU64::new(0);
        let nonce = NONCE.fetch_add(1, Ordering::Relaxed);

        let json = serde_json::to_vec(self)?;
        let final_path = Self::meta_path(target);
        let mut tmp_path = final_path.clone().into_os_string();
        tmp_path.push(format!(".tmp.{nonce}"));
        let tmp_path = PathBuf::from(tmp_path);
        match fs::write(&tmp_path, json).await {
            Ok(()) => {}
            Err(e) => {
                let _ = fs::remove_file(&tmp_path).await;
                return Err(e);
            }
        }
        match fs::rename(&tmp_path, &final_path).await {
            Ok(()) => Ok(()),
            Err(e) => {
                let _ = fs::remove_file(&tmp_path).await;
                Err(e)
            }
        }
    }

    pub async fn load(target: &Path) -> std::io::Result<Option<Self>> {
        match fs::read(Self::meta_path(target)).await {
            Ok(bytes) => match serde_json::from_slice(&bytes) {
                Ok(state) => Ok(Some(state)),
                // Corrupt sidecar (e.g. torn write from a version predating
                // save()'s atomic tmp+rename dance, bit-rot, or an external
                // edit) — treat as missing so the caller falls back to a
                // fresh download instead of getting stuck on a sidecar
                // that can never be parsed.
                Err(_) => Ok(None),
            },
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e),
        }
    }

    pub async fn load_if_matches(
        target: &Path,
        expected_hash: &str,
    ) -> std::io::Result<Option<Self>> {
        Ok(Self::load(target)
            .await?
            .filter(|s| s.file_hash == expected_hash))
    }

    pub async fn clear(target: &Path) -> std::io::Result<()> {
        for path in [Self::meta_path(target), Self::partial_path(target)] {
            match fs::remove_file(&path).await {
                Ok(()) => {}
                // Acceptable: file was already absent.
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                // Real failure (permission denied, EBUSY, etc.) — surface it
                // so callers can decide whether to retry, log, or escalate.
                // A failed clear would otherwise leave stale sidecar bytes
                // that a future resume could read and trust.
                Err(e) => return Err(e),
            }
        }
        // A completed download also clears any temp files an interrupted
        // save() left behind, so the download directory doesn't keep them.
        Self::sweep_tmp(target).await?;
        Ok(())
    }

    /// Remove orphaned atomic-write temp files (`<meta>.tmp.<nonce>`).
    ///
    /// `save()` writes to a uniquely-named `.tmp.<nonce>` sibling and renames
    /// it over the final `.meta`. If the future is dropped *between* the write
    /// and the rename — which happens routinely when a worker is torn down on
    /// cancellation or a connection timeout — the temp file is orphaned.
    /// `save()`'s own error path only cleans up on a returned error, not on a
    /// drop, and `clear()` historically removed only the `.meta`/`.haex-partial`
    /// pair. Without this sweep those orphans accumulated without bound in the
    /// download directory across repeated interrupted downloads.
    ///
    /// Call it at the start of a download attempt (the previous attempt's
    /// worker pool has fully unwound, so no save is in flight and every
    /// remaining `.tmp.*` is garbage) and from `clear()` on success. Keeping
    /// the `.meta`/`.haex-partial` pair lets resume still work; only the
    /// orphaned temp files are removed.
    pub async fn sweep_tmp(target: &Path) -> std::io::Result<()> {
        let meta = Self::meta_path(target);
        let (Some(dir), Some(meta_name)) =
            (meta.parent(), meta.file_name().and_then(|n| n.to_str()))
        else {
            return Ok(());
        };
        let tmp_prefix = format!("{meta_name}.tmp.");
        let mut entries = match fs::read_dir(dir).await {
            Ok(e) => e,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(e) => return Err(e),
        };
        while let Some(entry) = entries.next_entry().await? {
            let name = entry.file_name();
            if name.to_str().is_some_and(|n| n.starts_with(&tmp_prefix)) {
                match fs::remove_file(entry.path()).await {
                    Ok(()) => {}
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                    Err(e) => return Err(e),
                }
            }
        }
        Ok(())
    }

    pub fn missing_ranges(&self) -> Vec<(u64, u64)> {
        let mut ranges = Vec::new();
        let cs = self.chunk_size as u64;
        let mut i = 0;
        while i < self.completed.len() {
            if !self.completed[i] {
                let start = i as u64 * cs;
                let mut j = i;
                while j < self.completed.len() && !self.completed[j] {
                    j += 1;
                }
                let end = j as u64 * cs;
                ranges.push((start, end));
                i = j;
            } else {
                i += 1;
            }
        }
        ranges
    }
}

#[cfg(test)]
mod tests;
