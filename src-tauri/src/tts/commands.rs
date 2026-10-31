//! Speech catalog commands (spec 066 T022, contracts/tauri-commands.md): list the catalog, list
//! what is installed, download a model. Which voice a language speaks with is a device preference
//! (`assistant.voice.<sprache>`) written through `set_pref`, not a command here.

use std::path::PathBuf;

use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

use crate::error::{HolziError, Result};
use crate::hardware;
use crate::state::AppState;

use super::catalog::{self, TtsCatalogEntry, TtsCatalogEntryWithFit, TtsEngine};
use super::files;
use super::TtsError;

/// Progress of every download the assistant starts (speech models, assistant assets).
pub const EVENT_DOWNLOAD_PROGRESS: &str = "assistant-download-progress";

#[derive(Debug, Clone, Serialize)]
pub struct DownloadProgressWire<'a> {
    pub id: &'a str,
    pub received: u64,
    pub total: u64,
}

/// Whether this build can speak with `engine`.
fn engine_enabled(engine: TtsEngine) -> bool {
    match engine {
        TtsEngine::Pocket => cfg!(feature = "voice"),
    }
}

#[tauri::command]
pub async fn list_tts_catalog() -> Result<Vec<TtsCatalogEntryWithFit>> {
    let hw = hardware::probe_async().await;
    Ok(catalog::entries_with_fit(&hw)?)
}

/// The ids of `entries` whose files are complete; `resolve_dir` returning `None` counts as not
/// installed. Without a Tauri `AppHandle` so the tests can pass plain directories.
pub fn scan_installed(
    entries: &[TtsCatalogEntry],
    resolve_dir: impl Fn(&TtsCatalogEntry) -> Option<PathBuf>,
) -> Vec<String> {
    entries
        .iter()
        .filter(|entry| engine_enabled(entry.engine))
        .filter(|entry| {
            resolve_dir(entry).is_some_and(|dir| files::is_complete_model(&dir, entry.engine))
        })
        .map(|entry| entry.id.clone())
        .collect()
}

#[tauri::command]
pub async fn list_installed_tts_voices(app: AppHandle) -> Result<Vec<String>> {
    let entries = catalog::entries()?;
    tauri::async_runtime::spawn_blocking(move || {
        scan_installed(entries, |entry| files::resolve_model_dir(&app, entry).ok())
    })
    .await
    .map_err(|e| HolziError::CrdtInit {
        reason: format!("scan installed speech models: {e}"),
    })
}

/// Downloads (or repairs) the files of catalog entry `voice_id`, reporting progress as
/// [`EVENT_DOWNLOAD_PROGRESS`]. Idempotent.
#[tauri::command]
pub async fn download_tts_voice(
    app: AppHandle,
    state: State<'_, AppState>,
    voice_id: String,
) -> Result<()> {
    let entry = catalog::get(&voice_id)?;
    state
        .gate()
        .run(ensure_installed(&app, &state, entry))
        .await?
}

/// Makes sure `entry` is installed, downloading what is missing. Shared with
/// `download_assistant_assets`.
pub(crate) async fn ensure_installed(
    app: &AppHandle,
    state: &AppState,
    entry: &'static TtsCatalogEntry,
) -> Result<()> {
    if !engine_enabled(entry.engine) {
        return Err(TtsError::Unavailable {
            reason: format!("{:?} is not enabled in this build", entry.engine),
        }
        .into());
    }
    let _publication_lock =
        crate::models::paths::acquire_model_publication_lock(app, &entry.id, &state.gate().token())
            .await?;
    let app_for_dir = app.clone();
    let dir =
        tauri::async_runtime::spawn_blocking(move || files::resolve_model_dir(&app_for_dir, entry))
            .await
            .map_err(|e| HolziError::CrdtInit {
                reason: format!("resolve speech model directory: {e}"),
            })??;
    files::ensure_model_files(&dir, entry, |received, total| {
        let _ = app.emit(
            EVENT_DOWNLOAD_PROGRESS,
            DownloadProgressWire {
                id: &entry.id,
                received,
                total,
            },
        );
    })
    .await?;
    Ok(())
}
