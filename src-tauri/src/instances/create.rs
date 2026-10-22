//! `create_instance` — Genesis flow with `.pending` marker + rollback.
//!
//! Contract: `tauri-commands.md` §`create_instance`. Marker precedes the
//! DB so a crash between marker creation and marker removal is detectable
//! by `startup::cleanup_orphans_on_startup`.

use std::path::Path;
use std::sync::Arc;

use haex_crdt::Database;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Runtime, State};
use ts_rs::TS;

use crate::chat::default_model::start_default_model_preload;
use crate::chat::events::emit_model_load_status;
use crate::chat::session::ChatState;
use crate::error::{HolziError, Result};
use crate::identity::installation_id_path;
use crate::state::{ActiveInstanceHandle, AppState};
use crate::voice::VoiceState;

use super::cleanup::remove_vault_files;
use super::events::emit_instance_list_changed;
use super::info::InstanceInfo;
use super::passphrase::Passphrase;
use super::paths::{
    get_app_local_data, get_instance_path, get_pending_marker_path, validate_instance_name,
};
use super::vault_config::vault_config;
use super::vault_id;

#[derive(Debug, Deserialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
#[serde(rename_all = "camelCase")]
pub struct CreateInstanceArgs {
    pub name: String,
    #[ts(type = "string")]
    pub passphrase: Passphrase,
}

#[derive(Debug, Serialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
#[serde(rename_all = "camelCase")]
pub struct CreateInstanceResult {
    pub info: InstanceInfo,
}

/// Everything `create_instance` does up to and including publishing the new vault as the active
/// instance — generic over `R: Runtime`, not the concrete `AppHandle`, so a test can call it with
/// `tauri::test::MockRuntime`'s handle instead of a real, windowed one (spec 013 T053).
///
/// A refusal from [`crate::vault_gate::VaultGate::ensure_can_open`] is the first thing checked,
/// before any path is even computed (spec 013 FR-010, FR-022).
pub async fn create_instance_core<R: Runtime>(
    app: &AppHandle<R>,
    state: &AppState,
    chat: &ChatState,
    name: &str,
    passphrase: Passphrase,
) -> Result<CreateInstanceResult> {
    state.gate().ensure_can_open()?;
    let _operation = chat.acquire_operation()?;
    validate_instance_name(name)?;
    passphrase.validate_new()?;

    let db_path = get_instance_path(app, name)?;
    let pending_marker = get_pending_marker_path(&db_path);
    let app_local_data = get_app_local_data(app)?;
    let installation_id_file = installation_id_path(&app_local_data);

    if db_path.exists() {
        return Err(HolziError::NameConflict {
            name: name.to_string(),
        });
    }

    // Marker is atomic (`create_new`) — a parallel create in flight
    // fails with AlreadyExists here, before any Database::open work.
    std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&pending_marker)
        .map_err(|e| match e.kind() {
            std::io::ErrorKind::AlreadyExists => HolziError::NameConflict {
                name: name.to_string(),
            },
            _ => HolziError::from(e),
        })?;

    // From here on, any early return MUST take back every file of the vault (`cleanup`).
    // Moved into the blocking task: the last owner erases it when the task ends.
    let open_path = db_path.clone();
    let open_installation_id_file = installation_id_file.clone();
    let open_result = match tauri::async_runtime::spawn_blocking(move || {
        open_new_database(passphrase.as_str(), &open_path, &open_installation_id_file)
    })
    .await
    {
        Ok(result) => result,
        Err(e) => {
            remove_vault_files(&db_path);
            return Err(HolziError::CrdtInit {
                reason: format!("database open task failed: {e}"),
            });
        }
    };

    match open_result {
        Ok(db_arc) => {
            if let Err(error) = vault_id::write(&db_path, &db_arc) {
                drop(db_arc);
                remove_vault_files(&db_path);
                return Err(error.into());
            }
            match publish_active(state, name, &db_arc, &pending_marker) {
                Ok(result) => Ok(result),
                Err(e) => {
                    drop(db_arc);
                    remove_vault_files(&db_path);
                    Err(e)
                }
            }
        }
        Err(e) => {
            remove_vault_files(&db_path);
            Err(e)
        }
    }
}

/// Creates, initializes, and publishes a new encrypted instance.
#[tauri::command]
pub async fn create_instance(
    app: AppHandle,
    state: State<'_, AppState>,
    chat: State<'_, ChatState>,
    voice: State<'_, VoiceState>,
    args: CreateInstanceArgs,
) -> Result<CreateInstanceResult> {
    let result = create_instance_core(&app, &state, &chat, &args.name, args.passphrase).await?;
    // Spec 043 FR-011a: the screen capture protection of this device, before anything shows.
    if let Ok(db) = state.database() {
        crate::privacy::screen_capture::apply_for_vault(&app, &db).await;
    }
    // Spec 024: the sync service runs as tracked session work and ends with the close.
    crate::vault_events::start_for_active_instance(&app, &state);
    crate::extensions::registry::lifecycle::start_for_active_instance(&app, &state);
    // Spec 057: channels end when their permission or the other vault's standing goes.
    crate::extensions::peers::watch::start_for_active_instance(&app, &state);
    crate::extensions::sql::changes::start_for_active_instance(&app, &state);
    // Spec 051 (FR-021): changes of contacts reach the phone's address book.
    crate::contacts::phone::trigger::watch(&app);
    crate::sync::start_for_active_instance(&app, &state).await;
    // Spec 049: the External Bridge, when this device turned it on.
    crate::browser_bridge::start_for_active_instance(&app, &state);

    *chat.session.lock().unwrap_or_else(|e| e.into_inner()) = None;
    voice.invalidate_whisper_cache().await;
    emit_instance_list_changed(&app, "created", Some(args.name.clone()));
    chat.bump_vault_generation();
    emit_model_load_status(&app, &chat);
    start_default_model_preload(app.clone(), chat.inner().clone());
    Ok(result)
}

/// Publishes a newly opened database as the active instance.
pub(super) fn publish_active(
    state: &AppState,
    name: &str,
    db_arc: &Arc<Database>,
    pending_marker: &Path,
) -> Result<CreateInstanceResult> {
    state.install(
        ActiveInstanceHandle {
            name: name.to_string(),
            database: Arc::clone(db_arc),
        },
        // Publication commits Genesis: a leftover marker would cause startup
        // cleanup to delete this vault, so removal must succeed first.
        || Ok(std::fs::remove_file(pending_marker)?),
    )?;

    let info = InstanceInfo {
        name: name.to_string(),
        alias: None,
        last_access: now_millis(),
    };
    Ok(CreateInstanceResult { info })
}

/// Opens a new database with the lifecycle command's bootstrap configuration.
fn open_new_database(
    passphrase: &str,
    db_path: &Path,
    installation_id_file: &Path,
) -> Result<Arc<Database>> {
    let config = vault_config(passphrase, db_path, installation_id_file, true);
    let db = Database::open(config)?;
    // Spec 022: secure_delete from the start; the VACUUM queued by migration 0020 is cheap on an
    // empty vault.
    crate::storage::maintenance::run_after_open(&db);
    // Spec 024: vault identity, device keys, first device list and content key.
    crate::sync::genesis::run_after_open(&db, installation_id_file, true);
    Ok(Arc::new(db))
}

/// Returns the current UNIX timestamp in milliseconds.
fn now_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
#[path = "create_tests.rs"]
mod tests;
