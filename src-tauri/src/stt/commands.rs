//! STT-catalog Tauri commands (spec 010): list the built-in catalog, get
//! tier recommendations, list installed tiers, and download one. Setting
//! the active tier is a plain preference write (`voice.stt_model_id`) via
//! the existing generic `set_pref` command — no dedicated command for that.

#[cfg(any(feature = "llm-cpu", feature = "voice"))]
use std::path::PathBuf;

#[cfg(any(feature = "llm-cpu", feature = "voice"))]
use tauri::{AppHandle, State};

use crate::error::{HolziError, Result};
use crate::hardware;
#[cfg(any(feature = "llm-cpu", feature = "voice"))]
use crate::state::AppState;

use super::catalog::{self, SttCatalogEntry, SttCatalogEntryWithFit, SttTierRecommendation};

/// One installed (fully downloaded) STT catalog entry.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledSttModel {
    pub id: String,
    pub name: String,
}

/// Tauri command: list the built-in STT catalog, annotated with a fit
/// verdict against the current hardware. Mirrors `list_catalog`
/// (`catalog/commands.rs`) for the LLM catalog.
#[tauri::command]
pub async fn list_stt_catalog() -> Result<Vec<SttCatalogEntryWithFit>> {
    let hw = hardware::probe_async().await;
    Ok(catalog::entries_with_fit(&hw)?)
}

/// Tauri command: exactly three tier-labelled STT recommendations for the
/// onboarding wizard's STT step. Mirrors `catalog_recommend_tiers`.
#[tauri::command]
pub async fn stt_recommend_tiers() -> Result<[SttTierRecommendation; 3]> {
    let hw = hardware::probe_async().await;
    catalog::recommend_tiers(&hw, crate::platform::capabilities().model_presets)?.ok_or_else(|| {
        HolziError::CatalogEntryNotFound {
            id: "<empty stt catalog>".to_string(),
        }
    })
}

/// Pure scanning logic behind `list_installed_stt_models`, parameterized
/// over how to resolve each entry's directory so it's unit-testable
/// without a Tauri `AppHandle` — this codebase has no mocking convention
/// for that type (every other `AppHandle`-taking function is likewise
/// tested only through its `&Path`-based helpers). `resolve_dir` returning
/// `None` (a resolution failure, e.g. `AppLocalData` itself unresolvable)
/// is treated the same as "not installed" for that one entry rather than
/// failing the whole listing.
/// `pub` (rather than private) so `commands_tests.rs` can exercise it
/// directly with fake directories, without a Tauri `AppHandle`.
#[cfg(any(feature = "llm-cpu", feature = "voice"))]
pub fn scan_installed(
    entries: &[SttCatalogEntry],
    resolve_dir: impl Fn(&SttCatalogEntry) -> Option<PathBuf>,
) -> Vec<InstalledSttModel> {
    entries
        .iter()
        .filter_map(|entry| {
            let dir = resolve_dir(entry)?;
            super::files::is_complete_model(&dir, entry.engine).then(|| InstalledSttModel {
                id: entry.id.clone(),
                name: entry.name.clone(),
            })
        })
        .collect()
}

/// Tauri command: catalog entries whose files are already fully installed
/// on this device. No DB table behind this — a directory-existence scan
/// (data-model.md).
#[cfg(any(feature = "llm-cpu", feature = "voice"))]
#[tauri::command]
pub async fn list_installed_stt_models(app: AppHandle) -> Result<Vec<InstalledSttModel>> {
    let entries = catalog::entries()?;
    let app_for_scan = app.clone();
    let resolved = tauri::async_runtime::spawn_blocking(move || {
        entries
            .iter()
            .map(|entry| {
                super::files::resolve_model_dir(&app_for_scan, entry)
                    .map(|dir| (entry.id.clone(), dir))
            })
            .collect::<std::result::Result<Vec<_>, _>>()
    })
    .await
    .map_err(|e| HolziError::CrdtInit {
        reason: format!("resolve installed STT model directories: {e}"),
    })??;

    Ok(scan_installed(entries, |entry| {
        resolved
            .iter()
            .find(|(id, _)| id == &entry.id)
            .map(|(_, dir)| dir.clone())
    }))
}

/// Builds without either local engine (`llm-cpu` for Whisper, `voice` for
/// Parakeet) can run no local model, so nothing can ever be installed — an empty list is the correct answer, not an
/// error (mirrors how `voice::invalidate_stt_model_cache` is a successful
/// no-op in the same build).
#[cfg(not(any(feature = "llm-cpu", feature = "voice")))]
#[tauri::command]
pub async fn list_installed_stt_models() -> Result<Vec<InstalledSttModel>> {
    Ok(Vec::new())
}

/// Resolves `catalog_id` to its catalog entry, or `CatalogEntryNotFound`.
/// Pure/`AppHandle`-free on purpose so the error-mapping is directly
/// unit-testable. `pub` for the same reason as [`scan_installed`].
pub fn resolve_catalog_entry(catalog_id: &str) -> Result<&'static SttCatalogEntry> {
    catalog::get(catalog_id)?.ok_or_else(|| HolziError::CatalogEntryNotFound {
        id: catalog_id.to_string(),
    })
}

/// Tauri command: downloads (or repairs an incomplete download of)
/// `catalog_id`'s files. Idempotent. Does **not** change the active
/// `voice.stt_model_id` preference — the frontend does that separately
/// after a successful download, mirroring `download_model_from_catalog`'s
/// contract for chat models (`ModelChoiceStep.vue` calls
/// `downloadFromCatalogAsync` then `setPrefAsync` as two separate steps).
#[cfg(any(feature = "llm-cpu", feature = "voice"))]
#[tauri::command]
pub async fn download_stt_model(
    app: AppHandle,
    state: State<'_, AppState>,
    catalog_id: String,
) -> Result<InstalledSttModel> {
    let entry = resolve_catalog_entry(&catalog_id)?;
    let _publication_lock = crate::models::paths::acquire_model_publication_lock(
        &app,
        &entry.id,
        &state.gate().token(),
    )
    .await?;
    let app_for_dir = app.clone();
    let dir = tauri::async_runtime::spawn_blocking(move || {
        super::files::resolve_model_dir(&app_for_dir, entry)
    })
    .await
    .map_err(|e| HolziError::CrdtInit {
        reason: format!("resolve STT model directory: {e}"),
    })??;
    state
        .gate()
        .run(super::files::ensure_model_files(&dir, entry))
        .await??;
    Ok(InstalledSttModel {
        id: entry.id.clone(),
        name: entry.name.clone(),
    })
}

/// Builds without either local engine can't download or load a local model at all —
/// surfaced as the same `LocalUnavailable`-backed error the transcription
/// path itself would report if it tried.
#[cfg(not(any(feature = "llm-cpu", feature = "voice")))]
#[tauri::command]
pub async fn download_stt_model(_catalog_id: String) -> Result<InstalledSttModel> {
    Err(crate::stt::SttError::LocalUnavailable {
        reason: "local transcription is not enabled in this build".into(),
    }
    .into())
}
