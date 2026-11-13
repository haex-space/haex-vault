//! The models an extension may use, in its settings (spec 055 US2, T032): list them, grant one with
//! a ceiling, revoke it, and choose the default one among the granted. Every change holds from the next start of a
//! job, which reads the grants anew; a running job sees it before its next step.

use serde::Deserialize;
use tauri::{AppHandle, State};
use ts_rs::TS;
use uuid::Uuid;

use crate::error::{HolziError, Result};
use crate::extensions::host::ExtensionHost;
use crate::extensions::permissions::default_model;
use crate::extensions::permissions::prompts::Question;
use crate::extensions::permissions::{PermissionKind, PermissionStatus, Target};
use crate::model_jobs::effort::Level;
use crate::model_jobs::grants::{self, ExtensionModelEntry};
use crate::passwords::clock::unix_millis;
use crate::state::AppState;
use crate::state_utils::active_database;
use crate::storage::wm_session_commands::current_device_uuid;
use crate::vault_gate::VaultDb;

#[derive(Debug, Clone, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct ExtensionModelGrantArgs {
    pub extension_id: String,
    pub model_id: String,
    /// The effort the user chose: one the model offers (`none`, `low`, `medium`, `high`, `xhigh`, `max`).
    pub effort: String,
}

#[derive(Debug, Clone, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct ExtensionModelRevokeArgs {
    pub extension_id: String,
    pub model_id: String,
}

#[derive(Debug, Clone, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct ExtensionModelDefaultArgs {
    pub extension_id: String,
    /// The model that becomes the default; absent takes the default away.
    #[serde(default)]
    #[ts(optional)]
    pub model_id: Option<String>,
}

/// The model and effort the user picked in a question about a model.
#[derive(Debug, Clone, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct ExtensionModelPick {
    pub model_id: String,
    /// `none`, `low`, `medium`, `high`, `xhigh` or `max`.
    pub effort: String,
    /// The model also becomes the default model of the extension.
    #[serde(default)]
    pub make_default: bool,
}

/// The question as the user answered it: for a model, the one picked and its ceiling.
pub(crate) fn as_answered(
    question: &Question,
    pick: Option<ExtensionModelPick>,
) -> Result<Question> {
    let Some(pick) = pick.filter(|_| question.kind == PermissionKind::Model) else {
        return Ok(question.clone());
    };
    if !matches!(
        Target::parse(PermissionKind::Model, &pick.model_id),
        Some(Target::Model(_))
    ) {
        return Err(invalid("not a model id"));
    }
    let effort = Level::parse(&pick.effort).ok_or_else(|| invalid("unknown effort"))?;
    Ok(Question {
        action: effort.as_str().to_owned(),
        target: pick.model_id,
        ..question.clone()
    })
}

/// Remembers the answer to a question about a model: one row per model.
pub(crate) fn remember_model(
    db: &VaultDb,
    host: &ExtensionHost,
    question: &Question,
    status: PermissionStatus,
    now_ms: i64,
) -> Result<()> {
    let effort = Level::parse(&question.action).ok_or_else(|| invalid("unknown effort"))?;
    let (extension_id, model_id) = (question.extension_id, question.target.clone());
    db.write_blocking({
        let model_id = model_id.clone();
        move |tx| {
            grants::put(tx, extension_id, &model_id, status, effort, now_ms).map_err(Into::into)
        }
    })?;
    grants::forget_held(host, extension_id, &model_id);
    Ok(())
}

fn invalid(reason: &str) -> HolziError {
    HolziError::InvalidInput {
        reason: reason.to_owned(),
    }
}

fn parse_id(text: &str) -> Result<Uuid> {
    Uuid::parse_str(text).map_err(|_| HolziError::ExtensionNotFound)
}

fn task_error(error: tauri::Error) -> HolziError {
    HolziError::InvalidInput {
        reason: format!("model grants task: {error}"),
    }
}

/// Every model of the vault for one extension: the ones it may use first, its default before the others.
pub fn list(
    db: &VaultDb,
    host: &ExtensionHost,
    device: Uuid,
    extension_id: Uuid,
) -> Result<Vec<ExtensionModelEntry>> {
    let held = host
        .permissions
        .temporary(extension_id, PermissionKind::Model);
    db.read_blocking(move |q| grants::entries(q, extension_id, device, held).map_err(Into::into))
}

/// Lets the extension use a model up to `effort`, for every own device.
pub fn set(
    db: &VaultDb,
    host: &ExtensionHost,
    args: ExtensionModelGrantArgs,
    now_ms: i64,
) -> Result<()> {
    let extension_id = parse_id(&args.extension_id)?;
    let effort = Level::parse(&args.effort).ok_or_else(|| HolziError::InvalidInput {
        reason: "unknown effort".into(),
    })?;
    let model_id = args.model_id.clone();
    db.write_blocking(move |tx| {
        grants::put(
            tx,
            extension_id,
            &args.model_id,
            PermissionStatus::Granted,
            effort,
            now_ms,
        )
        .map_err(Into::into)
    })?;
    grants::forget_held(host, extension_id, &model_id);
    Ok(())
}

/// Takes the model away again, remembered or held for the session: the next job asks.
pub fn revoke(db: &VaultDb, host: &ExtensionHost, args: ExtensionModelRevokeArgs) -> Result<()> {
    let extension_id = parse_id(&args.extension_id)?;
    let model_id = args.model_id.clone();
    db.write_blocking(move |tx| grants::revoke(tx, extension_id, &model_id).map_err(Into::into))?;
    grants::forget_held(host, extension_id, &args.model_id);
    Ok(())
}

/// Makes `model_id` the default model of an installed extension, or takes the default away. Only
/// a model the extension may use now (remembered, or held for the session) can be the default: a
/// list that was loaded before a revocation must not bring a revoked model back as the default.
pub fn set_default(
    db: &VaultDb,
    host: &ExtensionHost,
    device: Uuid,
    args: ExtensionModelDefaultArgs,
) -> Result<()> {
    let extension_id = parse_id(&args.extension_id)?;
    if let Some(model_id) = &args.model_id {
        let allowed = list(db, host, device, extension_id)?
            .iter()
            .any(|entry| entry.id == *model_id && entry.ceiling.is_some());
        if !allowed {
            return Err(invalid("the extension may not use this model"));
        }
    }
    db.write_blocking(move |tx| match &args.model_id {
        Some(model_id) => grants::make_default(tx, extension_id, model_id).map_err(Into::into),
        None => default_model::clear(tx, extension_id).map_err(Into::into),
    })
}

#[tauri::command]
pub async fn extension_model_grants_list(
    app: AppHandle,
    state: State<'_, AppState>,
    extension_id: String,
) -> Result<Vec<ExtensionModelEntry>> {
    let id = parse_id(&extension_id)?;
    let db = active_database(&state)?;
    let device = current_device_uuid(&app, &db)?;
    let host = state.extensions();
    tauri::async_runtime::spawn_blocking(move || list(&db, &host, device, id))
        .await
        .map_err(task_error)?
}

#[tauri::command]
pub async fn extension_model_grants_set(
    state: State<'_, AppState>,
    args: ExtensionModelGrantArgs,
) -> Result<()> {
    let db = active_database(&state)?;
    let host = state.extensions();
    tauri::async_runtime::spawn_blocking(move || {
        set(&db, &host, args, unix_millis(std::time::SystemTime::now()))
    })
    .await
    .map_err(task_error)?
}

#[tauri::command]
pub async fn extension_model_grants_revoke(
    state: State<'_, AppState>,
    args: ExtensionModelRevokeArgs,
) -> Result<()> {
    let db = active_database(&state)?;
    let host = state.extensions();
    tauri::async_runtime::spawn_blocking(move || revoke(&db, &host, args))
        .await
        .map_err(task_error)?
}

#[tauri::command]
pub async fn extension_model_default_set(
    app: AppHandle,
    state: State<'_, AppState>,
    args: ExtensionModelDefaultArgs,
) -> Result<()> {
    let db = active_database(&state)?;
    let device = current_device_uuid(&app, &db)?;
    let host = state.extensions();
    tauri::async_runtime::spawn_blocking(move || set_default(&db, &host, device, args))
        .await
        .map_err(task_error)?
}

#[cfg(test)]
#[path = "model_grants_tests.rs"]
mod tests;
