//! The models an extension may use, in its settings (spec 055 US2, T032): list them, grant one with
//! a ceiling, revoke it, and order the granted ones. Every change holds from the next start of a
//! job, which reads the grants anew; a running job sees it before its next step.

use serde::Deserialize;
use tauri::{AppHandle, State};
use ts_rs::TS;
use uuid::Uuid;

use crate::error::{HolziError, Result};
use crate::extensions::host::ExtensionHost;
use crate::extensions::permissions::order;
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
    /// The highest effort: `none`, `low`, `medium` or `high`.
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
pub struct ExtensionModelOrderArgs {
    pub extension_id: String,
    /// The granted models, first choice first.
    pub model_ids: Vec<String>,
}

/// The model and ceiling the user picked in a question about a model.
#[derive(Debug, Clone, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct ExtensionModelPick {
    pub model_id: String,
    /// `none`, `low`, `medium` or `high`.
    pub effort: String,
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

/// Remembers the answer to a question about a model: one row per model, and a new grant takes its
/// place in the rank (spec 055 R5).
pub(crate) fn remember_model(
    db: &VaultDb,
    question: &Question,
    status: PermissionStatus,
    now_ms: i64,
) -> Result<()> {
    let effort = Level::parse(&question.action).ok_or_else(|| invalid("unknown effort"))?;
    let (extension_id, model_id) = (question.extension_id, question.target.clone());
    db.write_blocking(move |tx| {
        grants::put(tx, extension_id, &model_id, status, effort, now_ms).map_err(Into::into)
    })
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

/// Every model of the vault for one extension: the ones it may use first, in its rank.
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
pub fn set(db: &VaultDb, args: ExtensionModelGrantArgs, now_ms: i64) -> Result<()> {
    let extension_id = parse_id(&args.extension_id)?;
    let effort = Level::parse(&args.effort).ok_or_else(|| HolziError::InvalidInput {
        reason: "unknown effort".into(),
    })?;
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
    })
}

/// Takes the model away again, remembered or held for the session: the next job asks.
pub fn revoke(db: &VaultDb, host: &ExtensionHost, args: ExtensionModelRevokeArgs) -> Result<()> {
    let extension_id = parse_id(&args.extension_id)?;
    let model_id = args.model_id.clone();
    db.write_blocking(move |tx| grants::revoke(tx, extension_id, &model_id).map_err(Into::into))?;
    grants::forget_held(host, extension_id, &args.model_id);
    Ok(())
}

/// Orders the granted models of an installed extension.
pub fn reorder(db: &VaultDb, args: ExtensionModelOrderArgs) -> Result<()> {
    let extension_id = parse_id(&args.extension_id)?;
    db.write_blocking(move |tx| {
        order::set_order(tx, extension_id, &args.model_ids).map_err(Into::into)
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
    tauri::async_runtime::spawn_blocking(move || {
        set(&db, args, unix_millis(std::time::SystemTime::now()))
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
pub async fn extension_model_grants_reorder(
    state: State<'_, AppState>,
    args: ExtensionModelOrderArgs,
) -> Result<()> {
    let db = active_database(&state)?;
    tauri::async_runtime::spawn_blocking(move || reorder(&db, args))
        .await
        .map_err(task_error)?
}

#[cfg(test)]
#[path = "model_grants_tests.rs"]
mod tests;
