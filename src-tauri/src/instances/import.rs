//! `import_instance` — take over a vault file the person chose (spec 043 FR-002a, contract
//! `import-instance.md`, research R8): copy it into the instances folder under a free name, unlock
//! the copy and publish it as the active vault. The chosen file is only read. Any failure after the
//! marker takes every file of the copy back (`cleanup`).

use std::path::Path;

use serde::Deserialize;
use tauri::{AppHandle, Runtime, State};
use ts_rs::TS;

use crate::chat::default_model::start_default_model_preload;
use crate::chat::events::emit_model_load_status;
use crate::chat::session::ChatState;
use crate::error::{HolziError, Result};
use crate::files::picked::{self, Opener, PickedFile};
use crate::identity::installation_id_path;
use crate::state::AppState;
use crate::voice::VoiceState;

use super::cleanup::remove_vault_files;
use super::create::{publish_active, CreateInstanceResult};
use super::events::emit_instance_list_changed;
use super::open::open_existing_database;
use super::passphrase::Passphrase;
use super::paths::{
    get_app_local_data, get_instances_directory, get_pending_marker_path, INSTANCE_EXTENSION,
};
use super::vault_id;

/// Names tried before giving up (`name`, `name-2`, …).
const MAX_NAME_TRIES: usize = 100;
/// The longest instance name (`paths::validate_instance_name`).
const MAX_NAME_LEN: usize = 64;

#[derive(Debug, Deserialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
#[serde(rename_all = "camelCase")]
pub struct ImportInstanceArgs {
    /// The choice of the open dialog.
    pub file: PickedFile,
    #[ts(type = "string")]
    pub passphrase: Passphrase,
}

/// The instance name for a shown file name (data-model.md): the name without its extension,
/// characters outside `[A-Za-z0-9_-]` become `-`, it starts with a letter or digit, at most 64
/// characters; `vault` when nothing is left.
pub fn name_for(file_name: &str) -> String {
    let stem = Path::new(file_name)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or(file_name);
    let mapped: String = stem
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '-'
            }
        })
        .collect();
    let trimmed: String = mapped
        .trim_start_matches(|c: char| !c.is_ascii_alphanumeric())
        .chars()
        .take(MAX_NAME_LEN)
        .collect();
    if trimmed.is_empty() {
        "vault".to_string()
    } else {
        trimmed
    }
}

/// `base`, then `base-2`, `base-3`, …, each at most 64 characters.
fn candidate(base: &str, index: usize) -> String {
    if index == 1 {
        return base.to_string();
    }
    let suffix = format!("-{index}");
    let keep = MAX_NAME_LEN - suffix.len();
    format!("{}{suffix}", &base[..base.len().min(keep)])
}

/// Picks a free name and reserves it with its pending marker (`create_new`, so a parallel create
/// or import takes the next one). Returns the name and the path of its database file.
fn reserve_name(dir: &Path, base: &str) -> Result<(String, std::path::PathBuf)> {
    for index in 1..=MAX_NAME_TRIES {
        let name = candidate(base, index);
        let db_path = dir.join(format!("{name}.{INSTANCE_EXTENSION}"));
        if db_path.exists() {
            continue;
        }
        match std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(get_pending_marker_path(&db_path))
        {
            Ok(_) => return Ok((name, db_path)),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error.into()),
        }
    }
    Err(HolziError::NameConflict {
        name: base.to_string(),
    })
}

/// A database that opened but is no vault of holzi: another app's SQLCipher file, or a holzi
/// file from a version this one cannot read.
fn not_a_vault(error: HolziError) -> HolziError {
    match error {
        HolziError::WrongPassphrase
        | HolziError::NotEnoughSpace
        | HolziError::Unreadable
        | HolziError::AlreadyOnThisDevice { .. }
        | HolziError::VaultAlreadyOpenElsewhere => error,
        other => {
            log::warn!("import_instance: the chosen file is no vault: {other}");
            HolziError::NotAValidInstance {
                reason: "not a vault of holzi".to_string(),
            }
        }
    }
}

/// Everything `import_instance` does up to and including publishing the vault, generic over the
/// runtime and the opener of the chosen file so a test can run it without a window.
pub async fn import_instance_core<R: Runtime>(
    app: &AppHandle<R>,
    state: &AppState,
    chat: &ChatState,
    opener: impl Opener + Send + 'static,
    file: PickedFile,
    passphrase: Passphrase,
) -> Result<CreateInstanceResult> {
    state.gate().ensure_can_open()?;
    let _operation = chat.acquire_operation()?;

    let dir = get_instances_directory(app)?;
    let installation_id_file = installation_id_path(&get_app_local_data(app)?);
    // Fails early on a file that is not there, before any name is taken.
    drop(picked::open_read(&opener, &file)?);
    let base = name_for(&picked::display_name(&opener, &file));
    let (name, db_path) = reserve_name(&dir, &base)?;

    let copy_path = db_path.clone();
    let open_dir = dir.clone();
    let own_name = name.clone();
    let opened = tauri::async_runtime::spawn_blocking(move || {
        picked::copy_into(&opener, &file, &copy_path)?;
        let db = open_existing_database(passphrase.as_str(), &copy_path, &installation_id_file)
            .map_err(not_a_vault)?;
        let fingerprint =
            vault_id::fingerprint(&db).ok_or_else(|| HolziError::NotAValidInstance {
                reason: "no vault identity".to_string(),
            })?;
        if let Some(other) = vault_id::owner_of(&open_dir, &fingerprint, &own_name) {
            return Err(HolziError::AlreadyOnThisDevice { name: other });
        }
        vault_id::write(&copy_path, &db)?;
        Ok(db)
    })
    .await
    .map_err(|error| HolziError::CrdtInit {
        reason: format!("import task failed: {error}"),
    })
    .and_then(|result| result);

    let db = match opened {
        Ok(db) => db,
        Err(error) => {
            remove_vault_files(&db_path);
            return Err(error);
        }
    };
    match publish_active(state, &name, &db, &get_pending_marker_path(&db_path)) {
        Ok(result) => Ok(result),
        Err(error) => {
            drop(db);
            remove_vault_files(&db_path);
            Err(error)
        }
    }
}

/// Takes over a chosen vault file and opens it as the active vault.
#[tauri::command]
pub async fn import_instance(
    app: AppHandle,
    state: State<'_, AppState>,
    chat: State<'_, ChatState>,
    voice: State<'_, VoiceState>,
    args: ImportInstanceArgs,
) -> Result<CreateInstanceResult> {
    let result =
        import_instance_core(&app, &state, &chat, app.clone(), args.file, args.passphrase).await?;
    // Spec 043 FR-011a: the screen capture protection of this device, before anything shows.
    if let Ok(db) = state.database() {
        crate::privacy::screen_capture::apply_for_vault(&app, &db).await;
    }
    // As after an open (`open.rs`): the session services of the vault.
    crate::vault_events::start_for_active_instance(&app, &state);
    crate::extensions::registry::lifecycle::start_for_active_instance(&app, &state);
    // Spec 057: channels end when their permission or the other vault's standing goes.
    crate::extensions::peers::watch::start_for_active_instance(&app, &state);
    // Spec 018: register the confirmed extension tools for an imported vault as well.
    crate::extensions::agent_tools::registration::start_for_active_instance(&app, &state, &chat);
    crate::extensions::sql::changes::start_for_active_instance(&app, &state);
    crate::sync::start_for_active_instance(&app, &state).await;
    // Spec 061: sync rules run on this device while the vault is open; other own devices are
    // reached through the sync just started.
    crate::file_sync::service::start_for_active_instance(&app, &state);
    // Spec 049: the External Bridge, when this device turned it on.
    crate::browser_bridge::start_for_active_instance(&app, &state);
    // Spec 021: the MCP access for external agents, when this device turned it on.
    crate::mcp_server::start_for_active_instance(&app, &state);
    // Spec 055 FR-037: the models of every provider are read again.
    crate::providers::refresh_all::start_after_open(&state);
    crate::passwords::maintenance::start_after_open(&state);
    // Spec 051: the address book tidies its photos once per open.
    crate::contacts::maintenance::start_after_open(&state);
    // Spec 051 (FR-020): the sync with the phone's address book catches up.
    crate::contacts::phone::trigger::start(&app);
    crate::contacts::phone::trigger::watch(&app);
    // Spec 060 (research R9): links other holzi processes got for this vault.
    crate::deep_link::start_for_active_instance(&app);
    crate::extensions::registry::blobs::start_after_open(&state);

    chat.bump_vault_generation();
    voice.invalidate_whisper_cache().await;
    emit_model_load_status(&app, &chat);
    emit_instance_list_changed(&app, "imported", Some(result.info.name.clone()));
    start_default_model_preload(app.clone(), chat.inner().clone());
    Ok(result)
}

#[cfg(test)]
#[path = "import_tests.rs"]
mod tests;
