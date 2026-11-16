//! Where the default wake word models come from, and how a newer version reaches a device without
//! a new app release.
//!
//! The version the app was built with is inside the binary ([`bundled`], 3 MB). So the default
//! wake word works at once, offline, and without a download. When the assistant is switched on,
//! [`check`] looks (once per run, best effort) for `update.json` in the model repository
//! `haex-space/haex-wakeword`:
//!
//! ```json
//! { "version": 2, "revision": "<40 hex>",
//!   "embedding": { "bytes": 2183273, "sha256": "<64 hex>" },
//!   "classifier": { "bytes": 857934, "sha256": "<64 hex>" } }
//! ```
//!
//! A manifest with a higher `version` than what is in use makes the device fetch both files at
//! that `revision`, check them against the manifest and keep them in the models folder
//! (`wakeword-hex/v<version>/`, recorded last in `current.json`). The next time the assistant is
//! switched on, [`select`] takes them. A file that is damaged or missing, one that does not
//! load, or one older than the binary's is ignored and the bundled version is used, so a device never ends without a wake
//! word. Publishing a new version = pushing the files and a manifest with the next `version`;
//! `BUNDLED_VERSION` follows with the next app release.

use std::borrow::Cow;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tauri::AppHandle;

use crate::error::{HolziError, Result};
use crate::file_sync::place_device::hex;
use crate::models::download::download_to_file;

use super::model::Models;

pub const EMBEDDING_FILE: &str = "speech_embedding.onnx";
pub const CLASSIFIER_FILE: &str = "hex.onnx";

/// `models::paths` slug of the folder the updated files go in.
const SLUG: &str = "wakeword-hex";
const REPO: &str = "haex-space/haex-wakeword";
const CURRENT: &str = "current.json";
/// A sanity limit for a file the manifest names; the models are a few MB.
const MAX_BYTES: u64 = 32 * 1024 * 1024;

/// The version of the models in the binary: `haex-space/haex-wakeword` revision
/// `d0d2dc6225c3bb1103756ebaa15a4e1d16cd77b8`, SHA-256 in `models/README.md`.
pub const BUNDLED_VERSION: u32 = 1;
const BUNDLED_EMBEDDING: &[u8] = include_bytes!("models/speech_embedding.onnx");
const BUNDLED_CLASSIFIER: &[u8] = include_bytes!("models/hex.onnx");

/// One file of a version.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Part {
    pub bytes: u64,
    pub sha256: String,
}

impl Part {
    fn matches(&self, content: &[u8]) -> bool {
        content.len() as u64 == self.bytes && hex(&Sha256::digest(content)) == self.sha256
    }

    fn is_well_formed(&self) -> bool {
        (1..=MAX_BYTES).contains(&self.bytes) && is_hex(&self.sha256, 64)
    }
}

/// `update.json` of the model repository, and `current.json` of the device (the same shape).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    /// Counts up with every published version; only a higher one replaces what is in use.
    pub version: u32,
    /// The commit of the model repository that holds the files.
    pub revision: String,
    pub embedding: Part,
    pub classifier: Part,
}

fn is_hex(text: &str, len: usize) -> bool {
    text.len() == len && text.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

fn failed(what: &str, error: impl std::fmt::Display) -> HolziError {
    HolziError::SpeechFailed {
        reason: format!("{what} of the wake word models: {error}"),
    }
}

impl Manifest {
    /// Reads and checks a manifest: a revision (never a branch), well-formed hashes and sizes.
    pub fn parse(text: &str) -> Result<Self> {
        let manifest: Self =
            serde_json::from_str(text).map_err(|e| failed("read the manifest", e))?;
        if !is_hex(&manifest.revision, 40) {
            return Err(failed("read the manifest", "the revision is not a commit"));
        }
        if !manifest.embedding.is_well_formed() || !manifest.classifier.is_well_formed() {
            return Err(failed(
                "read the manifest",
                "a file has no size or checksum",
            ));
        }
        Ok(manifest)
    }

    fn dir(&self, root: &Path) -> PathBuf {
        root.join(format!("v{}", self.version))
    }
}

/// The two model files of one version, as bytes.
pub struct Files {
    pub version: u32,
    pub embedding: Cow<'static, [u8]>,
    pub classifier: Cow<'static, [u8]>,
}

/// The version inside the binary.
pub fn bundled() -> Files {
    Files {
        version: BUNDLED_VERSION,
        embedding: Cow::Borrowed(BUNDLED_EMBEDDING),
        classifier: Cow::Borrowed(BUNDLED_CLASSIFIER),
    }
}

/// The updated version recorded in `root`, when it is complete and its files match the record.
fn installed(root: &Path) -> Option<Files> {
    let manifest = Manifest::parse(&std::fs::read_to_string(root.join(CURRENT)).ok()?).ok()?;
    let dir = manifest.dir(root);
    let embedding = std::fs::read(dir.join(EMBEDDING_FILE)).ok()?;
    let classifier = std::fs::read(dir.join(CLASSIFIER_FILE)).ok()?;
    (manifest.embedding.matches(&embedding) && manifest.classifier.matches(&classifier)).then_some(
        Files {
            version: manifest.version,
            embedding: Cow::Owned(embedding),
            classifier: Cow::Owned(classifier),
        },
    )
}

/// What to run: the updated version in `root` if it is newer than the binary's, else the
/// binary's. Reads and hashes 3 MB; callers run it off the async runtime.
pub fn select(root: Option<&Path>) -> Files {
    root.and_then(installed)
        .filter(|files| files.version > BUNDLED_VERSION)
        .unwrap_or_else(bundled)
}

/// The models to run: [`select`]ed ones, loaded. An update whose files match their checksums but
/// that do not load (an operator or format the runtime does not know) is not kept in use: the
/// bundled version is loaded instead, so a bad update never leaves a device without a wake word.
/// Takes a few hundred milliseconds; callers run it off the async runtime.
pub fn load(root: Option<&Path>) -> Result<Arc<Models>> {
    let files = select(root);
    match Models::load(&files.embedding, &files.classifier) {
        Err(error) if files.version != BUNDLED_VERSION => {
            log::warn!(
                "assistant: wake word models v{} did not load, using the bundled ones: {error}",
                files.version
            );
            let bundled = bundled();
            Models::load(&bundled.embedding, &bundled.classifier)
        }
        loaded => loaded,
    }
}

/// Makes `manifest` the version in `root`, once both its files are in `manifest.dir(root)` and
/// match the manifest; the record is written last, so a half-done update is never in use. Older
/// versions are removed.
fn commit(root: &Path, manifest: &Manifest) -> Result<()> {
    let dir = manifest.dir(root);
    for (file, part) in [
        (EMBEDDING_FILE, &manifest.embedding),
        (CLASSIFIER_FILE, &manifest.classifier),
    ] {
        let content = std::fs::read(dir.join(file)).map_err(|e| failed("read an update", e))?;
        if !part.matches(&content) {
            let _ = std::fs::remove_dir_all(&dir);
            return Err(failed("check an update", format!("{file} is damaged")));
        }
    }
    let record = serde_json::to_vec_pretty(manifest).map_err(|e| failed("record an update", e))?;
    let mut file =
        tempfile::NamedTempFile::new_in(root).map_err(|e| failed("record an update", e))?;
    std::io::Write::write_all(&mut file, &record).map_err(|e| failed("record an update", e))?;
    file.persist(root.join(CURRENT))
        .map_err(|e| failed("record an update", e.error))?;
    if let Ok(entries) = std::fs::read_dir(root) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() && path != dir {
                let _ = std::fs::remove_dir_all(path);
            }
        }
    }
    Ok(())
}

fn url(revision: &str, file: &str) -> String {
    format!("https://huggingface.co/{REPO}/resolve/{revision}/{file}")
}

async fn fetch_manifest(root: &Path) -> Result<Manifest> {
    let path = root.join("update.json");
    download_to_file(&url("main", "update.json"), path.clone(), |_| {})
        .await
        .map_err(|e| failed("look for an update", e))?;
    let text = std::fs::read_to_string(&path);
    let _ = std::fs::remove_file(&path);
    Manifest::parse(&text.map_err(|e| failed("look for an update", e))?)
}

async fn update(root: PathBuf) -> Result<()> {
    std::fs::create_dir_all(&root).map_err(|e| failed("create the folder", e))?;
    let manifest = fetch_manifest(&root).await?;
    let in_use = tauri::async_runtime::spawn_blocking({
        let root = root.clone();
        move || select(Some(&root)).version
    })
    .await
    .map_err(|e| failed("look for an update", e))?;
    if manifest.version <= in_use {
        return Ok(());
    }
    let dir = manifest.dir(&root);
    for file in [EMBEDDING_FILE, CLASSIFIER_FILE] {
        download_to_file(&url(&manifest.revision, file), dir.join(file), |_| {})
            .await
            .map_err(|e| failed("download an update", e))?;
    }
    tauri::async_runtime::spawn_blocking(move || commit(&root, &manifest))
        .await
        .map_err(|e| failed("record an update", e))??;
    log::info!("assistant: a newer wake word model is installed; it is used from the next start of the assistant");
    Ok(())
}

/// The folder the updated files live in.
pub fn root(app: &AppHandle) -> Result<PathBuf> {
    crate::models::paths::slug_dir(app, SLUG)
}

/// Looks for a newer version once per run and downloads it; a device without a connection keeps
/// what it has and asks again the next time the assistant is switched on. Errors are only logged:
/// the default wake word works without it.
pub async fn check(app: AppHandle) {
    static CHECKED: AtomicBool = AtomicBool::new(false);
    if CHECKED.swap(true, Ordering::SeqCst) {
        return;
    }
    let result = match root(&app) {
        Ok(root) => update(root).await,
        Err(error) => Err(error),
    };
    if let Err(error) = result {
        CHECKED.store(false, Ordering::SeqCst);
        log::info!("assistant: no wake word update: {error}");
    }
}

#[cfg(test)]
#[path = "update_tests.rs"]
mod tests;
