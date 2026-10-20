//! The permission commands of holzi's window (US3, T067, contracts/tauri-commands.md
//! §Berechtigungen). Only holzi's own window reaches them; an extension cannot grant, resolve or
//! change a permission (FR-021).

use serde::Deserialize;
use serde_json::json;
use tauri::{AppHandle, State};
use ts_rs::TS;
use uuid::Uuid;

use super::frames::WindowEmitter;
use super::model_grants::{as_answered, remember_model, ExtensionModelPick};
pub use super::permission_list::{list, PermissionView};
use crate::error::{HolziError, Result};
use crate::extensions::bridge::dispatch::Emit;
use crate::extensions::bridge::events::emit_to_frames;
use crate::extensions::fs::end_revoked_watches;
use crate::extensions::host::ExtensionHost;
use crate::extensions::mail::watch::end_revoked as end_revoked_mail_watches;
use crate::extensions::permissions::prompts::{Asked, PermissionDecision, Question};
use crate::extensions::permissions::store::{self as permission_store, NewPermission};
use crate::extensions::permissions::{Permission, PermissionKind, PermissionStatus};
use crate::passwords::clock::unix_millis;
use crate::state::AppState;
use crate::state_utils::active_database;
use crate::storage::wm_session_commands::current_device_uuid;
use crate::vault_gate::VaultDb;

fn parse_id(text: &str) -> Result<Uuid> {
    Uuid::parse_str(text).map_err(|_| HolziError::ExtensionNotFound)
}

fn invalid(reason: &str) -> HolziError {
    HolziError::InvalidInput {
        reason: reason.to_owned(),
    }
}

#[derive(Debug, Clone, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct PermissionSetArgs {
    pub extension_id: String,
    pub kind: String,
    pub action: String,
    pub target: String,
    /// `granted`, `denied` or `ask`.
    pub status: String,
    /// The row whose state this setting changes, if any. It keeps its scope when the scope fits
    /// its kind (a shell row of another device stays on that device); a row from before
    /// Clarifications 2026-10-06 whose scope no longer fits is replaced by one in the kind's scope.
    #[serde(default)]
    pub replaces: Option<String>,
}

/// Writes a permission as the settings chose it; only forms holzi can read are accepted.
pub fn set(db: &VaultDb, device: Uuid, args: PermissionSetArgs, now_ms: i64) -> Result<()> {
    let extension_id = parse_id(&args.extension_id)?;
    let kind = PermissionKind::parse(&args.kind).ok_or_else(|| invalid("unknown kind"))?;
    let own_scope = kind.scope_on(device);
    if Permission::from_row(
        &args.kind,
        &args.action,
        &args.target,
        &args.status,
        own_scope,
    )
    .is_none()
    {
        return Err(invalid("permission not understood"));
    }
    let replaces = args.replaces.as_deref().map(parse_id).transpose()?;
    db.write_blocking(move |tx| {
        let rows = permission_store::rows_of(tx, extension_id)?;
        let scope = replaces
            .and_then(|old| rows.iter().find(|r| r.id == old))
            .filter(|r| r.is_about(&args.kind, &args.action, &args.target))
            .map(|r| r.vault_device_uuid)
            .filter(|&scope| kind.fits(scope))
            .unwrap_or(own_scope);
        let declared = rows
            .iter()
            .any(|r| r.declared && r.is_about(&args.kind, &args.action, &args.target));
        let id = permission_store::put(
            tx,
            extension_id,
            &NewPermission {
                kind: &args.kind,
                action: &args.action,
                target: &args.target,
                status: &args.status,
                declared,
                vault_device_uuid: scope,
            },
            now_ms,
        )?;
        // Only a row of this extension (as in `remove`).
        if let Some(old) = replaces.filter(|old| *old != id) {
            if rows.iter().any(|r| r.id == old) {
                permission_store::delete(tx, old)?;
            }
        }
        Ok(())
    })
}

#[derive(Debug, Clone, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct PermissionRemoveArgs {
    pub extension_id: String,
    /// A remembered row.
    #[serde(default)]
    pub permission_id: Option<String>,
    /// A decision held in memory: `kind|action|target`.
    #[serde(default)]
    pub temporary_key: Option<String>,
}

/// Revokes a remembered row or a decision held in memory. A row's removal reaches the running
/// watches through the vault's change report (`sql::changes`); a decision in memory has none, so
/// its watches are checked here.
pub fn remove(
    db: &VaultDb,
    host: &ExtensionHost,
    device: Uuid,
    args: PermissionRemoveArgs,
) -> Result<()> {
    let extension_id = parse_id(&args.extension_id)?;
    if let Some(key) = args.temporary_key {
        let mut parts = key.splitn(3, '|');
        let (Some(kind), Some(action), Some(target)) = (parts.next(), parts.next(), parts.next())
        else {
            return Err(invalid("temporaryKey is kind|action|target"));
        };
        let kind = PermissionKind::parse(kind).ok_or_else(|| invalid("unknown kind"))?;
        host.permissions.forget(extension_id, kind, action, target);
        end_revoked_watches(db, host, device);
        end_revoked_mail_watches(db, host, device);
        return Ok(());
    }
    let id = parse_id(args.permission_id.as_deref().unwrap_or_default())?;
    db.write_blocking(move |tx| {
        // Only a row of this extension.
        if permission_store::rows_of(tx, extension_id)?
            .iter()
            .any(|r| r.id == id)
        {
            permission_store::delete(tx, id)?;
        }
        Ok(())
    })
}

#[derive(Debug, Clone, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct PermissionResolveArgs {
    pub request_id: String,
    pub decision: PermissionDecision,
    /// Store the decision; else it holds until the vault closes.
    pub remember: bool,
    /// For a question about a model (spec 055): the user's pick instead of the proposed one.
    #[serde(default)]
    #[ts(optional)]
    pub model: Option<ExtensionModelPick>,
}

/// Keeps the user's decision on `question`: as a row with "remember", else held in memory (for an
/// encrypted folder until a frame of the extension ends, spec 048 FR-036).
pub fn record(
    db: &VaultDb,
    host: &ExtensionHost,
    device: Uuid,
    question: &Question,
    status: PermissionStatus,
    remember: bool,
    now_ms: i64,
) -> Result<()> {
    if !remember {
        host.permissions.hold(question, status);
        // No row was written, so no change report reaches the running watches (`remove`).
        end_revoked_watches(db, host, device);
        end_revoked_mail_watches(db, host, device);
        return Ok(());
    }
    if question.kind == PermissionKind::Model {
        return remember_model(db, host, question, status, now_ms);
    }
    let scope = question.kind.scope_on(device);
    let Question {
        extension_id,
        kind,
        action,
        target,
    } = question.clone();
    db.write_blocking(move |tx| {
        let declared = permission_store::rows_of(tx, extension_id)?
            .iter()
            .any(|r| r.declared && r.is_about(kind.as_str(), &action, &target));
        permission_store::put(
            tx,
            extension_id,
            &NewPermission {
                kind: kind.as_str(),
                action: &action,
                target: &target,
                status: status.as_str(),
                declared,
                vault_device_uuid: scope,
            },
            now_ms,
        )
        .map(drop)
        .map_err(Into::into)
    })
}

/// Answers an open question: remembered as a row or held in memory, then every frame of the
/// extension hears `extension:permission-resolved` and the SDK repeats its call. A question that
/// is gone (its frames closed) is ignored.
pub fn resolve(
    db: &VaultDb,
    host: &ExtensionHost,
    emitter: &dyn Emit,
    device: Uuid,
    args: PermissionResolveArgs,
    now_ms: i64,
) -> Result<()> {
    let Some(Asked { question, told }) = host.permissions.take(&args.request_id) else {
        return Ok(());
    };
    let status = match args.decision {
        PermissionDecision::Allow => PermissionStatus::Granted,
        PermissionDecision::Deny => PermissionStatus::Denied,
    };
    let answered = as_answered(&question, args.model)?;
    record(db, host, device, &answered, status, args.remember, now_ms)?;
    // The SDK waits for the target its 1004 named.
    for target in told {
        emit_to_frames(
            emitter,
            host,
            question.extension_id,
            "extension:permission-resolved",
            &json!({
                "resourceType": question.kind.as_str(),
                "action": question.action,
                "target": target,
                "decision": if status == PermissionStatus::Granted { "granted" } else { "denied" },
            }),
        );
    }
    Ok(())
}

/// The user closed the question without answering: it is no longer open, so the next identical
/// call asks again. Nothing is decided (closing cancels, it never denies).
pub fn cancel(host: &ExtensionHost, request_id: &str) {
    host.permissions.take(request_id);
}

#[tauri::command]
pub async fn extension_permissions_list(
    state: State<'_, AppState>,
    extension_id: String,
) -> Result<Vec<PermissionView>> {
    let id = parse_id(&extension_id)?;
    let db = active_database(&state)?;
    let host = state.extensions();
    tauri::async_runtime::spawn_blocking(move || list(&db, &host, id))
        .await
        .map_err(|e| invalid(&format!("permissions task: {e}")))?
}

#[tauri::command]
pub async fn extension_permission_set(
    app: AppHandle,
    state: State<'_, AppState>,
    args: PermissionSetArgs,
) -> Result<()> {
    let db = active_database(&state)?;
    let device = current_device_uuid(&app, &db)?;
    tauri::async_runtime::spawn_blocking(move || {
        set(&db, device, args, unix_millis(std::time::SystemTime::now()))
    })
    .await
    .map_err(|e| invalid(&format!("permissions task: {e}")))?
}

#[tauri::command]
pub async fn extension_permission_remove(
    app: AppHandle,
    state: State<'_, AppState>,
    args: PermissionRemoveArgs,
) -> Result<()> {
    let db = active_database(&state)?;
    let device = current_device_uuid(&app, &db)?;
    let host = state.extensions();
    tauri::async_runtime::spawn_blocking(move || remove(&db, &host, device, args))
        .await
        .map_err(|e| invalid(&format!("permissions task: {e}")))?
}

#[tauri::command]
pub async fn extension_permission_resolve(
    app: AppHandle,
    state: State<'_, AppState>,
    args: PermissionResolveArgs,
) -> Result<()> {
    let db = active_database(&state)?;
    let device = current_device_uuid(&app, &db)?;
    let host = state.extensions();
    let emitter = WindowEmitter(app.clone());
    tauri::async_runtime::spawn_blocking(move || {
        resolve(
            &db,
            &host,
            &emitter,
            device,
            args,
            unix_millis(std::time::SystemTime::now()),
        )
    })
    .await
    .map_err(|e| invalid(&format!("permissions task: {e}")))?
}

#[tauri::command]
pub fn extension_permission_cancel(state: State<'_, AppState>, request_id: String) {
    cancel(&state.extensions(), &request_id);
}

#[cfg(test)]
#[path = "permissions_tests.rs"]
mod tests;
