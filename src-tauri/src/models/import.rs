//! Local file import: copy an operator-picked GGUF into
//! `<AppLocalData>/models/<slug>/`.
//!
//! Plan §"Datenmodell": "Wird eine eigene GGUF importiert, wird sie
//! kopiert, nicht referenziert." Referencing the original path would
//! break when the user moves or deletes the source, and would tie the
//! model into a location outside the app's sandbox on mobile.

use std::fs;
use std::path::{Path, PathBuf};

use crate::error::{HaexError, Result};
use crate::files::picked::{self, Opener, PickedFile};

/// Copies the chosen file into `destination`, returning the actual byte count copied. Publishes
/// only a completed copy (a `.tmp` beside it, renamed at the end), preserving an existing model on
/// failure. The destination directory is expected to exist (callers use [`super::paths::slug_dir`]
/// first). A chosen file is a path on a desktop and a provider address on Android (spec 043).
pub async fn copy_into_managed(
    opener: impl Opener + Send + 'static,
    source: PickedFile,
    destination: PathBuf,
) -> Result<u64> {
    let target = destination.clone();
    let copied =
        tauri::async_runtime::spawn_blocking(move || picked::copy_into(&opener, &source, &target))
            .await
            .map_err(|e| HaexError::ModelImport {
                reason: format!("import task: {e}"),
            })?;
    copied.map_err(|e| match e {
        HaexError::NotEnoughSpace | HaexError::Unreadable => e,
        other => HaexError::ModelImport {
            reason: format!("import -> {}: {other}", destination.display()),
        },
    })
}

/// Removes model-import staging files left behind by cancellation or a
/// process crash. Only direct children of managed model slug directories are
/// considered, so unrelated files elsewhere under the app data directory
/// are never touched.
pub fn cleanup_staging_in_dir(models_root: &Path) -> Result<usize> {
    if !models_root.exists() {
        return Ok(0);
    }

    let mut removed = 0usize;
    for slug_entry in fs::read_dir(models_root).map_err(HaexError::from)? {
        let slug_entry = slug_entry.map_err(HaexError::from)?;
        if !slug_entry.file_type().map_err(HaexError::from)?.is_dir() {
            continue;
        }
        // Skips `.locks/` (spec 013 T071): the publication lock files living there are never
        // staging leftovers.
        if slug_entry
            .file_name()
            .to_str()
            .is_some_and(|name| name.starts_with('.'))
        {
            continue;
        }
        // Collected first so the `.backup` decision below can see whether a
        // finalized `.gguf` is present in this slug directory.
        let mut files: Vec<(String, std::path::PathBuf)> = Vec::new();
        for entry in fs::read_dir(slug_entry.path()).map_err(HaexError::from)? {
            let entry = entry.map_err(HaexError::from)?;
            if !entry.file_type().map_err(HaexError::from)?.is_file() {
                continue;
            }
            let Some(filename) = entry.file_name().to_str().map(str::to_owned) else {
                continue;
            };
            files.push((filename, entry.path()));
        }
        let has_published_file = files
            .iter()
            .any(|(name, _)| name.to_ascii_lowercase().ends_with(".gguf"));

        for (filename, path) in files {
            let is_staging = filename.ends_with(".tmp")
                || filename.ends_with(".staging")
                || filename.ends_with(".staging.part");
            // A `.backup` is the pre-update file `publish_staged_file` moved
            // aside. Once a finalized `.gguf` exists the publication went
            // through and the backup is dead weight (a full-size model file).
            // Without one, the process died between the two renames and the
            // backup is the only remaining copy — leave it for recovery.
            let is_dead_backup = filename.ends_with(".backup") && has_published_file;
            if is_staging || is_dead_backup {
                fs::remove_file(&path).map_err(HaexError::from)?;
                removed += 1;
            }
        }
    }
    Ok(removed)
}
