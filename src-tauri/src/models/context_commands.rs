//! The three commands behind the context settings of a local model (spec 086): read the setting, keep it,
//! and say what it would mean on this machine. They are reached only through the settings actions, which are
//! guardrails (FR-011): no agent and no MCP client can call them.

use tauri::{AppHandle, State};

use super::context_setting::{self, ContextSetting};
use super::paths;
use crate::chat::session::ChatState;
use crate::error::{HolziError, Result};
use crate::llm::llamacpp::memory::ModelShape;
use crate::llm::llamacpp::model::{free_memory, read_shape, wanted_context};
use crate::llm::llamacpp::report::{report, ContextReport};
use crate::state::AppState;
use crate::state_utils::active_database;
use crate::storage::models as models_store;

/// What the header of an installed model says, and the context the catalog wants for it. The file must be
/// installed on this device: a setting is for a model that can run here.
async fn shape_of(
    app: &AppHandle,
    state: &State<'_, AppState>,
    model_id: &str,
) -> Result<(Option<ModelShape>, Option<u32>)> {
    let canonical =
        paths::canonical_model_file(app, model_id)?.ok_or_else(|| HolziError::ModelNotFound {
            id: model_id.to_string(),
        })?;
    let db = active_database(state)?;
    let id = model_id.to_string();
    let catalog = db
        .read(move |r| models_store::get_model(r, &id))
        .await
        .ok()
        .flatten()
        .and_then(|row| row.context_window)
        .and_then(|n| u32::try_from(n).ok());
    let path = canonical.absolute_path;
    let shape = tauri::async_runtime::spawn_blocking(move || read_shape(&path, None))
        .await
        .map_err(|e| HolziError::Io {
            reason: format!("reading the model header: {e}"),
        })?
        .map_err(|e| HolziError::Io {
            reason: e.to_string(),
        })?;
    Ok((shape, catalog))
}

/// The context setting of this device for `model_id`; Auto and the standard cache while there is none.
#[tauri::command]
pub async fn get_model_context(
    state: State<'_, AppState>,
    model_id: String,
) -> Result<ContextSetting> {
    let db = active_database(&state)?;
    let device = db.device_id();
    db.read(move |r| context_setting::read(r, device, &model_id))
        .await
}

/// Keeps a setting for `model_id` on this device. A length the rules of the choice forbid (below the floor,
/// beyond four times the training, a stretch the model's own rope scaling forbids) is refused with the
/// reason and changes nothing. A length that does not fit the memory now is kept: the memory changes and the
/// load decides (FR-007).
#[tauri::command]
pub async fn set_model_context(
    app: AppHandle,
    state: State<'_, AppState>,
    model_id: String,
    setting: ContextSetting,
) -> Result<ContextSetting> {
    let (shape, _) = shape_of(&app, &state, &model_id).await?;
    context_setting::validate(shape.as_ref(), &setting).map_err(|e| HolziError::InvalidInput {
        reason: e.to_string(),
    })?;
    let db = active_database(&state)?;
    let device = db.device_id();
    db.write(move |tx| context_setting::write(tx, device, &model_id, &setting))
        .await?;
    Ok(setting)
}

/// What `setting` would mean for `model_id` on this machine now: what the file says, what the device allows
/// with the chosen cache type, how the choice would be placed and, when the model is loaded, what it runs with.
#[tauri::command]
pub async fn model_context_report(
    app: AppHandle,
    state: State<'_, AppState>,
    chat: State<'_, ChatState>,
    model_id: String,
    setting: ContextSetting,
) -> Result<ContextReport> {
    let (shape, catalog) = shape_of(&app, &state, &model_id).await?;
    let free = tauri::async_runtime::spawn_blocking(free_memory)
        .await
        .map_err(|e| HolziError::Io {
            reason: format!("reading free memory: {e}"),
        })?;
    let loaded = chat
        .session_for_local_model(&model_id)
        .and_then(|session| session.adapter.placement_info());
    Ok(report(
        shape.as_ref(),
        free,
        wanted_context(catalog),
        setting.choice(),
        setting.cache,
        loaded,
    ))
}
