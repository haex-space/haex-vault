//! Tauri commands of sync rules (contracts/tauri-commands.md): list, create, change, pause,
//! delete, run now and stop. The logic lives in the functions without Tauri below them; the
//! commands only find the vault, the device and the session.

use std::sync::Mutex;

use tauri::{AppHandle, Manager};

use super::model::{SyncRule, SyncRuleDirection, SyncRuleDraft};
use super::rules::{validate, SyncRuleError, SyncRuleErrorCode};
use super::schedule::{Command, Shared};
use super::service::FileSyncState;
use super::status::{status_of, SyncRuleView};
use super::store;
use crate::files::local::OwnPlaces;
use crate::files::state::FilesState;
use crate::passwords::clock;
use crate::state::AppState;
use crate::vault_gate::VaultDb;

fn unavailable(error: impl std::fmt::Display) -> SyncRuleError {
    log::warn!("file_sync: the vault could not be used: {error}");
    SyncRuleError::new(
        SyncRuleErrorCode::Unavailable,
        "the vault cannot be used now",
    )
}

fn not_found() -> SyncRuleError {
    SyncRuleError::new(SyncRuleErrorCode::NotFound, "no such rule")
}

/// The rule `id` of `device`.
async fn rule_of(db: &VaultDb, device: &str, id: &str) -> Result<SyncRule, SyncRuleError> {
    let (device, id) = (device.to_owned(), id.to_owned());
    db.read(move |r| Ok(store::rule(r, &device, &id)))
        .await
        .map_err(unavailable)?
        .map_err(unavailable)?
        .ok_or_else(not_found)
}

/// The rules of `device`, each with its status.
pub async fn list(
    db: &VaultDb,
    device: &str,
    shared: Option<&Mutex<Shared>>,
) -> Result<Vec<SyncRuleView>, SyncRuleError> {
    let owned = device.to_owned();
    let rows = db
        .read(move |r| {
            let rules = store::rules(r, &owned)?;
            let mut out = Vec::new();
            for rule in rules {
                let pending = store::pending(r, &owned, &rule.id)?.len();
                out.push((rule, pending));
            }
            Ok(out)
        })
        .await
        .map_err(unavailable)?;
    Ok(rows
        .into_iter()
        .map(|(rule, pending)| view(rule, pending, shared))
        .collect())
}

fn view(rule: SyncRule, pending: usize, shared: Option<&Mutex<Shared>>) -> SyncRuleView {
    let guard = shared.map(|shared| shared.lock().unwrap_or_else(|e| e.into_inner()));
    let running = guard
        .as_ref()
        .is_some_and(|shared| shared.running.contains(&rule.id));
    let last = guard.as_ref().and_then(|shared| shared.last.get(&rule.id));
    let status = status_of(
        &rule,
        running,
        last,
        u32::try_from(pending).unwrap_or(u32::MAX),
    );
    SyncRuleView { rule, status }
}

async fn others(
    db: &VaultDb,
    device: &str,
    except: Option<&str>,
) -> Result<Vec<SyncRule>, SyncRuleError> {
    let owned = device.to_owned();
    let mut rules = db
        .read(move |r| Ok(store::rules(r, &owned)))
        .await
        .map_err(unavailable)?
        .map_err(unavailable)?;
    rules.retain(|rule| Some(rule.id.as_str()) != except);
    Ok(rules)
}

fn checked(
    draft: SyncRuleDraft,
    others: &[SyncRule],
    own: &OwnPlaces,
) -> Result<SyncRuleDraft, SyncRuleError> {
    if draft.direction == SyncRuleDirection::Both {
        return Err(SyncRuleError::new(
            SyncRuleErrorCode::Unsupported,
            "two-way rules come with a later version",
        ));
    }
    validate(draft, others, own)
}

/// Creates a rule of `device`.
pub async fn create(
    db: &VaultDb,
    device: &str,
    own: &OwnPlaces,
    draft: SyncRuleDraft,
) -> Result<SyncRule, SyncRuleError> {
    let draft = checked(draft, &others(db, device, None).await?, own)?;
    let id = uuid::Uuid::new_v4().to_string();
    let (owned, rule_id, now) = (device.to_owned(), id.clone(), clock::now());
    db.write(move |tx| {
        store::insert_rule(tx, &owned, &rule_id, &draft, &now)?;
        Ok(())
    })
    .await
    .map_err(unavailable)?;
    rule_of(db, device, &id).await
}

/// Changes a rule of `device`; a change of places or direction starts it over (data-model.md).
pub async fn update(
    db: &VaultDb,
    device: &str,
    own: &OwnPlaces,
    id: &str,
    draft: SyncRuleDraft,
) -> Result<SyncRule, SyncRuleError> {
    rule_of(db, device, id).await?;
    let draft = checked(draft, &others(db, device, Some(id)).await?, own)?;
    let (owned, rule_id, now) = (device.to_owned(), id.to_owned(), clock::now());
    let found = db
        .write(move |tx| Ok(store::update_rule(tx, &owned, &rule_id, &draft, &now)?))
        .await
        .map_err(unavailable)?;
    if !found {
        return Err(not_found());
    }
    rule_of(db, device, id).await
}

/// Pauses or resumes a rule of `device`.
pub async fn set_enabled(
    db: &VaultDb,
    device: &str,
    id: &str,
    enabled: bool,
) -> Result<SyncRule, SyncRuleError> {
    let (owned, rule_id, now) = (device.to_owned(), id.to_owned(), clock::now());
    let found = db
        .write(move |tx| Ok(store::set_enabled(tx, &owned, &rule_id, enabled, &now)?))
        .await
        .map_err(unavailable)?;
    if !found {
        return Err(not_found());
    }
    rule_of(db, device, id).await
}

/// Deletes a rule of `device`; the files of both places stay (FR-006).
pub async fn delete(db: &VaultDb, device: &str, id: &str) -> Result<(), SyncRuleError> {
    let (owned, rule_id) = (device.to_owned(), id.to_owned());
    let found = db
        .write(move |tx| Ok(store::delete_rule(tx, &owned, &rule_id)?))
        .await
        .map_err(unavailable)?;
    if found {
        Ok(())
    } else {
        Err(not_found())
    }
}

/// The open vault and this device.
fn here(app: &AppHandle) -> Result<(VaultDb, String), SyncRuleError> {
    let state = app.state::<AppState>();
    let db = state.database().map_err(unavailable)?;
    if let Some(session) = app.state::<FileSyncState>().session() {
        return Ok((db, session.device.clone()));
    }
    let device = crate::storage::wm_session_commands::current_device_uuid(app, &db)
        .map_err(unavailable)?
        .to_string();
    Ok((db, device))
}

fn view_one(app: &AppHandle, rule: SyncRule) -> SyncRuleView {
    let session = app.state::<FileSyncState>().session();
    view(rule, 0, session.as_ref().map(|session| &*session.shared))
}

#[tauri::command]
pub async fn sync_rules_list(app: AppHandle) -> Result<Vec<SyncRuleView>, SyncRuleError> {
    let (db, device) = here(&app)?;
    let session = app.state::<FileSyncState>().session();
    list(
        &db,
        &device,
        session.as_ref().map(|session| &*session.shared),
    )
    .await
}

#[tauri::command]
pub async fn sync_rules_create(
    app: AppHandle,
    draft: SyncRuleDraft,
) -> Result<SyncRuleView, SyncRuleError> {
    let (db, device) = here(&app)?;
    let own = app.state::<FilesState>().own.clone();
    let rule = create(&db, &device, &own, draft).await?;
    app.state::<FileSyncState>().send(Command::Reload);
    Ok(view_one(&app, rule))
}

#[tauri::command]
pub async fn sync_rules_update(
    app: AppHandle,
    id: String,
    draft: SyncRuleDraft,
) -> Result<SyncRuleView, SyncRuleError> {
    let (db, device) = here(&app)?;
    let own = app.state::<FilesState>().own.clone();
    let rule = update(&db, &device, &own, &id, draft).await?;
    app.state::<FileSyncState>().send(Command::Reload);
    Ok(view_one(&app, rule))
}

#[tauri::command]
pub async fn sync_rules_set_enabled(
    app: AppHandle,
    id: String,
    enabled: bool,
) -> Result<SyncRuleView, SyncRuleError> {
    let (db, device) = here(&app)?;
    let sync = app.state::<FileSyncState>();
    if !enabled {
        sync.cancel_and_wait(id.clone()).await;
    }
    let result = set_enabled(&db, &device, &id, enabled).await;
    sync.send(Command::Reload);
    if !enabled {
        sync.release_cancel(id.clone());
    }
    let rule = result?;
    Ok(view_one(&app, rule))
}

#[tauri::command]
pub async fn sync_rules_delete(app: AppHandle, id: String) -> Result<(), SyncRuleError> {
    let (db, device) = here(&app)?;
    let sync = app.state::<FileSyncState>();
    sync.cancel_and_wait(id.clone()).await;
    let result = delete(&db, &device, &id).await;
    sync.send(Command::Reload);
    sync.release_cancel(id);
    result?;
    Ok(())
}

#[tauri::command]
pub async fn sync_rules_run_now(app: AppHandle, id: String) -> Result<(), SyncRuleError> {
    let (db, device) = here(&app)?;
    rule_of(&db, &device, &id).await?;
    app.state::<FileSyncState>().send(Command::Run(id));
    Ok(())
}

#[tauri::command]
pub async fn sync_rules_cancel(app: AppHandle, id: String) -> Result<(), SyncRuleError> {
    let sync = app.state::<FileSyncState>();
    sync.cancel_and_wait(id.clone()).await;
    sync.release_cancel(id);
    Ok(())
}

#[cfg(test)]
#[path = "commands_tests.rs"]
mod tests;
