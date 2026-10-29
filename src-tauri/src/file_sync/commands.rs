//! Tauri commands of sync rules (contracts/tauri-commands.md): list, create, change, pause,
//! delete, run now, stop, the preview, the exclusion file, the deletions waiting for the user, and
//! the run log with its depth. The logic lives in the functions without Tauri below them; the
//! commands only find the vault, the device and the session.

use std::sync::Mutex;

use tauri::{AppHandle, Emitter, Manager};

use super::manage::{self, SyncRuleIgnore, SyncRulePlan};
use super::model::{SyncRule, SyncRuleDraft, SyncRuleOutcome};
use super::pending::{self, SyncRulePending, SyncRulePendingChoice};
use super::rules::{validate, SyncRuleError, SyncRuleErrorCode};
use super::run_log::{self, SyncRuleRun, SyncRuleRunItem, MAX_LOG_DEPTH};
use super::scan::ScanError;
use super::schedule::{Command, Shared};
use super::service::{
    device_name, open_place, open_place_for, FileSyncState, SyncRulesChanged, SYNC_RULES_CHANGED,
};
use super::status::{status_of, SyncRuleFileError, SyncRuleView};
use super::store;
use crate::files::access::Want;
use crate::files::local::OwnPlaces;
use crate::files::state::FilesState;
use crate::files::{FilesError, FilesErrorCode};
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

/// Creates a rule of `device`.
pub async fn create(
    db: &VaultDb,
    device: &str,
    own: &OwnPlaces,
    draft: SyncRuleDraft,
) -> Result<SyncRule, SyncRuleError> {
    let draft = validate(draft, &others(db, device, None).await?, own)?;
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
    let draft = validate(draft, &others(db, device, Some(id)).await?, own)?;
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

/// The deletions of the rule `id` of `device` waiting for the user.
pub async fn pending_list(
    db: &VaultDb,
    device: &str,
    id: &str,
) -> Result<Vec<SyncRulePending>, SyncRuleError> {
    rule_of(db, device, id).await?;
    pending::list(db, device, id).await.map_err(unavailable)
}

/// The logged runs of the rule `id` of `device`, the latest first.
pub async fn runs(db: &VaultDb, device: &str, id: &str) -> Result<Vec<SyncRuleRun>, SyncRuleError> {
    rule_of(db, device, id).await?;
    let (device, id) = (device.to_owned(), id.to_owned());
    db.read(move |r| Ok(run_log::runs(r, &device, &id)))
        .await
        .map_err(unavailable)?
        .map_err(unavailable)
}

/// Up to `limit` file actions of a logged run of `device`, from `offset` on.
pub async fn run_items(
    db: &VaultDb,
    device: &str,
    run_id: &str,
    offset: u32,
    limit: u32,
) -> Result<Vec<SyncRuleRunItem>, SyncRuleError> {
    let (device, run_id) = (device.to_owned(), run_id.to_owned());
    db.read(move |r| Ok(run_log::items(r, &device, &run_id, offset, limit)))
        .await
        .map_err(unavailable)?
        .map_err(unavailable)
}

/// The log depth of `device`.
pub async fn log_depth(db: &VaultDb, device: &str) -> Result<u32, SyncRuleError> {
    let device = device.to_owned();
    db.read(move |r| Ok(run_log::depth(r, &device)))
        .await
        .map_err(unavailable)?
        .map_err(unavailable)
}

/// Sets the log depth of `device`; the logs of its rules shrink at once (FR-044).
pub async fn set_log_depth(db: &VaultDb, device: &str, depth: u32) -> Result<(), SyncRuleError> {
    if depth > MAX_LOG_DEPTH {
        return Err(SyncRuleError::new(
            SyncRuleErrorCode::InvalidDepth,
            "the log depth lies between 0 and 100",
        ));
    }
    let device = device.to_owned();
    db.write(move |tx| Ok(run_log::set_depth(tx, &device, depth)?))
        .await
        .map_err(unavailable)
}

fn place_error(error: FilesError) -> SyncRuleError {
    match error.code {
        FilesErrorCode::Unreachable => {
            SyncRuleError::new(SyncRuleErrorCode::Unreachable, error.message)
        }
        FilesErrorCode::ReadOnly | FilesErrorCode::NoAccess => {
            SyncRuleError::new(SyncRuleErrorCode::ReadOnly, error.message)
        }
        FilesErrorCode::TooLarge => {
            SyncRuleError::new(SyncRuleErrorCode::InvalidIgnore, error.message)
        }
        _ => SyncRuleError::new(SyncRuleErrorCode::PlaceError, error.message),
    }
}

fn scan_error(error: ScanError) -> SyncRuleError {
    match error {
        ScanError::Unreachable { reason, .. } => {
            SyncRuleError::new(SyncRuleErrorCode::Unreachable, reason)
        }
        ScanError::Failed { reason, .. } => {
            SyncRuleError::new(SyncRuleErrorCode::PlaceError, reason)
        }
    }
}

/// The rule a preview is for: the stored rule `id`, a draft of it, or a new draft; and the state it
/// starts from. A draft that moves the places or turns the direction starts without state, as the
/// saved change would (data-model.md).
async fn preview_rule(
    db: &VaultDb,
    device: &str,
    own: &OwnPlaces,
    id: Option<&str>,
    draft: Option<SyncRuleDraft>,
) -> Result<(SyncRule, Vec<store::StateRow>), SyncRuleError> {
    let stored = match id {
        Some(id) => Some(rule_of(db, device, id).await?),
        None => None,
    };
    let rule = match (draft, &stored) {
        (Some(draft), _) => {
            let draft = validate(draft, &others(db, device, id).await?, own)?;
            SyncRule {
                id: id.unwrap_or("preview").to_owned(),
                name: draft.name,
                place_a: draft.place_a,
                place_b: draft.place_b,
                direction: draft.direction,
                delete_mode: draft.delete_mode,
                interval_secs: draft.interval_secs,
                enabled: draft.enabled,
                allow_decrypted: draft.allow_decrypted,
                created_at: String::new(),
                updated_at: String::new(),
                last_run_at: None,
                last_outcome: None::<SyncRuleOutcome>,
                last_error: None,
            }
        }
        (None, Some(stored)) => stored.clone(),
        (None, None) => return Err(not_found()),
    };
    let keeps_state = stored.as_ref().is_some_and(|stored| {
        stored.place_a == rule.place_a
            && stored.place_b == rule.place_b
            && stored.direction == rule.direction
    });
    if !keeps_state {
        return Ok((rule, Vec::new()));
    }
    let (owned, rule_id) = (device.to_owned(), rule.id.clone());
    let state = db
        .read(move |r| Ok(store::state(r, &owned, &rule_id)))
        .await
        .map_err(unavailable)?
        .map_err(unavailable)?;
    Ok((rule, state))
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
    let sync = app.state::<FileSyncState>();
    sync.cancel_and_wait(id.clone()).await;
    let result = update(&db, &device, &own, &id, draft).await;
    sync.send(Command::Reload);
    sync.release_cancel(id);
    let rule = result?;
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

#[tauri::command]
pub async fn sync_rules_pending_list(
    app: AppHandle,
    id: String,
) -> Result<Vec<SyncRulePending>, SyncRuleError> {
    let (db, device) = here(&app)?;
    pending_list(&db, &device, &id).await
}

/// Applies the user's choice at once; a run of the rule stops meanwhile and goes on after. Returns
/// the files whose step failed; they stay in the list.
#[tauri::command]
pub async fn sync_rules_pending_resolve(
    app: AppHandle,
    id: String,
    paths: Vec<String>,
    choice: SyncRulePendingChoice,
) -> Result<Vec<SyncRuleFileError>, SyncRuleError> {
    let (db, device) = here(&app)?;
    let rule = rule_of(&db, &device, &id).await?;
    let sync = app.state::<FileSyncState>();
    let was_running = sync.session().is_some_and(|session| {
        session
            .shared
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .running
            .contains(&id)
    });
    sync.cancel_and_wait(id.clone()).await;
    let result = async {
        let a = open_place(&app, &rule.place_a).await.map_err(place_error)?;
        let b = open_place(&app, &rule.place_b).await.map_err(place_error)?;
        let run_id = uuid::Uuid::new_v4().to_string();
        pending::resolve(
            &db,
            &device,
            &rule,
            &run_id,
            a.as_ref(),
            b.as_ref(),
            &paths,
            choice,
        )
        .await
        .map_err(place_error)
    }
    .await;
    sync.release_cancel(id.clone());
    if was_running {
        sync.send(Command::Run(id.clone()));
    }
    if let Err(error) = app.emit(SYNC_RULES_CHANGED, SyncRulesChanged { rule_id: Some(id) }) {
        log::warn!("file_sync: emit {SYNC_RULES_CHANGED} failed: {error}");
    }
    Ok(result?
        .failures
        .into_iter()
        .map(|(path, reason)| SyncRuleFileError { path, reason })
        .collect())
}

#[tauri::command]
pub async fn sync_rules_runs(
    app: AppHandle,
    id: String,
) -> Result<Vec<SyncRuleRun>, SyncRuleError> {
    let (db, device) = here(&app)?;
    runs(&db, &device, &id).await
}

#[tauri::command]
pub async fn sync_rules_run_items(
    app: AppHandle,
    run_id: String,
    offset: u32,
    limit: u32,
) -> Result<Vec<SyncRuleRunItem>, SyncRuleError> {
    let (db, device) = here(&app)?;
    run_items(&db, &device, &run_id, offset, limit).await
}

#[tauri::command]
pub async fn sync_rules_log_depth_get(app: AppHandle) -> Result<u32, SyncRuleError> {
    let (db, device) = here(&app)?;
    log_depth(&db, &device).await
}

#[tauri::command]
pub async fn sync_rules_log_depth_set(app: AppHandle, depth: u32) -> Result<(), SyncRuleError> {
    let (db, device) = here(&app)?;
    set_log_depth(&db, &device, depth).await
}

/// What a run would do now; changes nothing. `id` alone previews a stored rule, `draft` a new one,
/// both a change of a stored one.
#[tauri::command]
pub async fn sync_rules_preview(
    app: AppHandle,
    id: Option<String>,
    draft: Option<SyncRuleDraft>,
) -> Result<SyncRulePlan, SyncRuleError> {
    let (db, device) = here(&app)?;
    let own = app.state::<FilesState>().own.clone();
    let (rule, state) = preview_rule(&db, &device, &own, id.as_deref(), draft).await?;
    let a = open_place(&app, &rule.place_a).await.map_err(place_error)?;
    let b = open_place(&app, &rule.place_b).await.map_err(place_error)?;
    let name = device_name(&db, &device).await;
    let today: String = clock::now().chars().take(10).collect();
    manage::preview(&rule, a.as_ref(), b.as_ref(), &state, &name, &today)
        .await
        .map_err(scan_error)
}

/// Opens place A to write, or to read with the reason it cannot be written.
async fn open_a(
    app: &AppHandle,
    rule: &SyncRule,
) -> Result<(Box<dyn super::place::Place>, Option<String>), SyncRuleError> {
    match open_place(app, &rule.place_a).await {
        Ok(place) => Ok((place, None)),
        Err(error)
            if matches!(
                error.code,
                FilesErrorCode::ReadOnly | FilesErrorCode::NoAccess
            ) =>
        {
            let place = open_place_for(app, &rule.place_a, Want::Read)
                .await
                .map_err(place_error)?;
            Ok((place, Some(error.message)))
        }
        Err(error) => Err(place_error(error)),
    }
}

async fn ignore_of(app: &AppHandle, rule: &SyncRule) -> Result<SyncRuleIgnore, SyncRuleError> {
    let (a, read_only) = open_a(app, rule).await?;
    let b = open_place_for(app, &rule.place_b, Want::Read)
        .await
        .map_err(place_error)?;
    manage::ignore_get(a.as_ref(), b.as_ref(), read_only)
        .await
        .map_err(scan_error)
}

#[tauri::command]
pub async fn sync_rules_ignore_get(
    app: AppHandle,
    id: String,
) -> Result<SyncRuleIgnore, SyncRuleError> {
    let (db, device) = here(&app)?;
    let rule = rule_of(&db, &device, &id).await?;
    ignore_of(&app, &rule).await
}

/// Writes the exclusion file of place A; the rule runs with it after.
#[tauri::command]
pub async fn sync_rules_ignore_set(
    app: AppHandle,
    id: String,
    content: String,
) -> Result<SyncRuleIgnore, SyncRuleError> {
    let (db, device) = here(&app)?;
    let rule = rule_of(&db, &device, &id).await?;
    let (a, read_only) = open_a(&app, &rule).await?;
    if let Some(reason) = read_only {
        return Err(SyncRuleError::new(SyncRuleErrorCode::ReadOnly, reason));
    }
    manage::ignore_set(a.as_ref(), &content)
        .await
        .map_err(place_error)?;
    app.state::<FileSyncState>().send(Command::Run(id));
    ignore_of(&app, &rule).await
}

#[cfg(test)]
#[path = "commands_tests.rs"]
mod tests;
