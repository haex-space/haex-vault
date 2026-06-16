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

    pub async fn save(&self, target: &Path) -> std::io::Result<()> {
        let json = serde_json::to_vec(self)?;
        fs::write(Self::meta_path(target), json).await
    }

    pub async fn load(target: &Path) -> std::io::Result<Option<Self>> {
        match fs::read(Self::meta_path(target)).await {
            Ok(bytes) => Ok(Some(serde_json::from_slice(&bytes)?)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e),
        }
    }

    pub async fn load_if_matches(
        target: &Path,
        expected_hash: &str,
    ) -> std::io::Result<Option<Self>> {
        Ok(Self::load(target).await?.filter(|s| s.file_hash == expected_hash))
    }

    pub async fn clear(target: &Path) -> std::io::Result<()> {
        let _ = fs::remove_file(Self::meta_path(target)).await;
        let _ = fs::remove_file(Self::partial_path(target)).await;
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
