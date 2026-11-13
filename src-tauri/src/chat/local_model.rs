//! The active local model: the one local model of this device the person chose on the settings
//! page "Lokale Modelle", remembered per device (`ai.active_local_model_id`). It goes into memory
//! when something needs it (a chat session on it, a model job), never when a vault opens, and
//! stays there until the person unloads it or the vault closes. The chat offers it among the other
//! models and loads it on use, but never unloads one itself.

use tauri::{AppHandle, State};

use crate::error::Result;
use crate::state::AppState;
use crate::state_utils::active_database;
use crate::storage::preferences::{self, PrefScope};

use crate::storage::models as models_store;

use super::model_loading::{describe, load_model_command, LoadMode, LoadedModelInfo};
use super::session::ChatState;

pub(crate) const PREF_ACTIVE_LOCAL_MODEL: &str = "ai.active_local_model_id";

/// Loads the installed local model `model_id` into memory and makes it the active one; the model
/// that was active is unloaded. Runs the mandatory integrity check of a local model: a failed
/// check is a structured `ModelIntegrity*` error, which the frontend shows as a decision dialog;
/// "load anyway" there calls [`activate_local_model_with_integrity_override`]. Refused while a turn
/// or another load holds the chat.
#[tauri::command]
pub async fn activate_local_model(
    app: AppHandle,
    state: State<'_, AppState>,
    chat: State<'_, ChatState>,
    model_id: String,
) -> Result<LoadedModelInfo> {
    activate(app, state, chat, model_id, false).await
}

/// Explicit, confirmation-gated bypass of the integrity check (the `load_untrusted` option of the
/// integrity dialog). Loads whatever file is on disk and marks `integrityStatus = 'untrusted'`;
/// `file_sha256` is left untouched, so the next normal activation asks again.
#[tauri::command]
pub async fn activate_local_model_with_integrity_override(
    app: AppHandle,
    state: State<'_, AppState>,
    chat: State<'_, ChatState>,
    model_id: String,
) -> Result<LoadedModelInfo> {
    activate(app, state, chat, model_id, true).await
}

async fn activate(
    app: AppHandle,
    state: State<'_, AppState>,
    chat: State<'_, ChatState>,
    model_id: String,
    integrity_override: bool,
) -> Result<LoadedModelInfo> {
    let info = load_model_command(
        app,
        state.clone(),
        chat,
        model_id.clone(),
        integrity_override,
        LoadMode::Activate,
    )
    .await?;
    remember_active_local_model(&state, Some(model_id)).await?;
    Ok(info)
}

/// Unloads the active local model and frees its memory; a chat session that ran it ends. Idempotent.
/// Refused while a turn or another load holds the chat.
#[tauri::command]
pub async fn deactivate_local_model(
    state: State<'_, AppState>,
    chat: State<'_, ChatState>,
) -> Result<()> {
    deactivate(&state, &chat).await
}

/// [`deactivate_local_model`] for the callers inside the app, such as the deletion of the model.
pub(crate) async fn deactivate(state: &State<'_, AppState>, chat: &ChatState) -> Result<()> {
    // The guard and the stored choice come first: a refused call, or a failed write, leaves the
    // tool check and the loaded model as they were.
    let _operation = chat.acquire_operation()?;
    remember_active_local_model(state, None).await?;
    chat.cancel_tool_check();
    chat.replace_local_model(None);
    Ok(())
}

/// The active local model, `None` when none is chosen. While it is not in memory yet, only what
/// the model's catalog row says is known: no engine and no placement.
#[tauri::command]
pub async fn active_local_model_info(
    state: State<'_, AppState>,
    chat: State<'_, ChatState>,
) -> Result<Option<LoadedModelInfo>> {
    if let Some(model) = chat.local_model() {
        return Ok(Some(describe(&state, model).await));
    }
    let Some(model_id) = chosen_local_model(&state).await? else {
        return Ok(None);
    };
    let db = active_database(&state)?;
    let row = db
        .read(move |r| models_store::get_model(r, &model_id))
        .await?;
    Ok(row.map(|row| LoadedModelInfo {
        model_id: row.id,
        name: row.name,
        tokenizer_repo: row.tokenizer_repo.unwrap_or_default(),
        context_window: row.context_window,
        engine: None,
        placement: None,
    }))
}

/// The active local model of this device as chosen, whether or not it is in memory.
pub(crate) async fn chosen_local_model(state: &State<'_, AppState>) -> Result<Option<String>> {
    let db = active_database(state)?;
    let device = db.device_id();
    Ok(db
        .read(move |r| preferences::get(r, PrefScope::Device(device), PREF_ACTIVE_LOCAL_MODEL))
        .await?
        .filter(|id| !id.is_empty()))
}

/// Writes the active local model of this device, or clears it.
pub(crate) async fn remember_active_local_model(
    state: &State<'_, AppState>,
    model_id: Option<String>,
) -> Result<()> {
    let db = active_database(state)?;
    let device = db.device_id();
    db.write(move |tx| match model_id {
        Some(id) => preferences::insert_or_update(
            tx,
            PrefScope::Device(device),
            PREF_ACTIVE_LOCAL_MODEL,
            &id,
        )
        .map(|_| ()),
        None => {
            preferences::delete(tx, PrefScope::Device(device), PREF_ACTIVE_LOCAL_MODEL).map(|_| ())
        }
    })
    .await?;
    Ok(())
}
