//! The MTP drafter of a model (spec 079): a second GGUF in `models/<slug>/drafter/`, pinned by the
//! catalog (research R2, R3). `paths::canonical_model_file` skips directories, so the subfolder never
//! counts as a second model file, and deleting the slug's model takes the folder along.
//!
//! Whether a drafter is installed is read from the file system, like an installed model since migration
//! 0012: the file exists with exactly the pinned size (data-model InstalledDrafter).

use std::path::{Path, PathBuf};

use super::hash::sha256_file;
use super::paths::{validate_filename, validate_slug};
use crate::catalog::CatalogDrafter;
use crate::error::{HolziError, Result};

/// The subfolder of a model's slug folder that holds its drafter.
pub const DRAFTER_DIRECTORY: &str = "drafter";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DrafterState {
    Installed,
    Missing,
}

/// `models/<slug>/drafter/`.
pub fn drafter_dir(models_root: &Path, slug: &str) -> Result<PathBuf> {
    validate_slug(slug)?;
    Ok(models_root.join(slug).join(DRAFTER_DIRECTORY))
}

/// `models/<slug>/drafter/<hf_filename>`.
pub fn drafter_file(models_root: &Path, slug: &str, drafter: &CatalogDrafter) -> Result<PathBuf> {
    validate_filename(&drafter.hf_filename)?;
    Ok(drafter_dir(models_root, slug)?.join(&drafter.hf_filename))
}

/// Installed when the file exists with exactly the pinned size. The hash is checked at load
/// ([`path_for_load`]), not here: the model list must not read hundreds of megabytes.
pub fn state(models_root: &Path, slug: &str, drafter: &CatalogDrafter) -> Result<DrafterState> {
    let file = drafter_file(models_root, slug, drafter)?;
    let installed = std::fs::metadata(&file)
        .is_ok_and(|meta| meta.is_file() && meta.len() == drafter.size_bytes);
    Ok(if installed {
        DrafterState::Installed
    } else {
        DrafterState::Missing
    })
}

/// The drafter to load with the model, or `None` with the reason in the log (spec FR-008). A file whose
/// hash is not the pinned one is removed, so the model list offers the download again instead of showing
/// a damaged file as an active speed-up (data-model InstalledDrafter).
///
/// Blocking: it hashes the file. Call it through `spawn_blocking`.
pub fn path_for_load(models_root: &Path, slug: &str, drafter: &CatalogDrafter) -> Option<PathBuf> {
    let file = match drafter_file(models_root, slug, drafter) {
        Ok(file) => file,
        Err(e) => {
            log::warn!("drafter of {slug} not used: {e}");
            return None;
        }
    };
    if !matches!(
        state(models_root, slug, drafter),
        Ok(DrafterState::Installed)
    ) {
        log::info!("drafter of {slug} not installed; loading without it");
        return None;
    }
    match sha256_file(&file) {
        Ok(hash) if hash == drafter.sha256 => Some(file),
        Ok(hash) => {
            log::warn!(
                "drafter of {slug} has hash {hash}, expected {}; removing it and loading without it",
                drafter.sha256
            );
            if let Err(e) = remove(models_root, slug) {
                log::warn!("could not remove the damaged drafter of {slug}: {e}");
            }
            None
        }
        Err(e) => {
            log::warn!("drafter of {slug} not used: {e}");
            None
        }
    }
}

/// The drafter to load with the installed model `model_id`, or `None` when its catalog entry names none or
/// [`path_for_load`] refuses it. Hashes on the blocking pool.
pub async fn for_load(models_root: PathBuf, model_id: &str) -> Option<PathBuf> {
    let drafter = crate::catalog::get(model_id)?.drafter.clone()?;
    let slug = model_id.to_owned();
    tauri::async_runtime::spawn_blocking(move || path_for_load(&models_root, &slug, &drafter))
        .await
        .unwrap_or_else(|e| {
            log::warn!("drafter of {model_id} not checked: {e}");
            None
        })
}

/// Deletes `models/<slug>/drafter/`; a missing folder is no error.
pub fn remove(models_root: &Path, slug: &str) -> Result<()> {
    let dir = drafter_dir(models_root, slug)?;
    match std::fs::remove_dir_all(&dir) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(HolziError::Io {
            reason: format!("remove {}: {e}", dir.display()),
        }),
    }
}

#[cfg(test)]
#[path = "drafter_tests.rs"]
mod tests;
