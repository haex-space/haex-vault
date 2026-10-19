//! Default-model resolution: the session-start resolver chain
//! (`resolve_default_model`), the Vault-open background preload it feeds
//! into, and a read-only snapshot of the current load state.
//!
//! Split out of `chat/commands.rs` (2026-09-15 review) — fifth and last of
//! five steps. See `chat/commands.rs`'s own history for the rest of the
//! split plan.

use serde::Serialize;
use tauri::{AppHandle, Manager, State};
use tokio_util::sync::CancellationToken;

use crate::error::Result;
use crate::models::paths;
use crate::state::AppState;
use crate::state_utils::active_database;
use crate::storage::{models as models_store, preferences, preferences::PrefScope};
use crate::vault_gate::VaultGate;

use super::commands::PREF_LAST_ACTIVE_MODEL;
use super::events::emit_load_error;
use super::model_loading::{
    load_api_key_model, load_model_inner, resolve_display_name, LoadIdentity, LoadOutcome,
};
use super::session::{ChatState, ModelLoadStatus};

pub(crate) const PREF_DEFAULT_MODEL: &str = "chat.default_model_id";

/// Which fallback branch the session resolver picked. Wire payload for
/// `resolve_default_model`.
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ResolveSource {
    LastActive,
    DefaultDevice,
    DefaultVault,
    FirstAvailable,
    None,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolveDefaultModelResult {
    pub model_id: Option<String>,
    pub source: ResolveSource,
}

/// Runs the session-start resolver chain (spec 002 §FR-014) and
/// returns which model the caller should load, plus the branch that
/// picked it. Pure read — no preferences are written along the chain.
/// If the returned `source` is `FirstAvailable`, the caller MUST NOT
/// echo the id back into `chat.last_active_model_id`; the passive
/// selection is a "temporary choice for this session" (FR-015).
#[tauri::command]
pub async fn resolve_default_model(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<ResolveDefaultModelResult> {
    let db = active_database(&state)?;
    let this_device = db.device_id();
    let (result, installed_local_ids, api_key_ids) = db
        .read(move |r| {
            // 1. last_active on this device.
            let last_active =
                preferences::get(r, PrefScope::Device(this_device), PREF_LAST_ACTIVE_MODEL)?;
            // 2. default on this device.
            let default_device =
                preferences::get(r, PrefScope::Device(this_device), PREF_DEFAULT_MODEL)?;
            // 3. default vault-wide.
            let default_vault = preferences::get(r, PrefScope::Vault, PREF_DEFAULT_MODEL)?;
            // 4. Everything on this device that could load.
            let all_models = models_store::list_all_models(r)?;
            let mut installed_local = Vec::new();
            let mut api_key_ids = Vec::new();
            for row in &all_models {
                if row.id.contains(':') {
                    api_key_ids.push(row.id.clone());
                } else {
                    // For local models the loadability check is
                    // "canonical file exists". We do that outside
                    // the DB call so `paths::canonical_model_file`
                    // can use the AppHandle.
                    installed_local.push(row.id.clone());
                }
            }
            let candidate_pref = |value: Option<String>| value.filter(|s| !s.is_empty());
            Ok::<_, haex_crdt::Error>((
                (
                    candidate_pref(last_active),
                    candidate_pref(default_device),
                    candidate_pref(default_vault),
                ),
                installed_local,
                api_key_ids,
            ))
        })
        .await?;
    let (last_active, default_device, default_vault) = result;

    // Validate API-key candidates through the same provider-row, model-row,
    // credential, legacy-repair, and adapter checks used by load_model.
    let mut loadable_api_key_ids = Vec::new();
    for composite_id in api_key_ids {
        let Some((provider_id_str, _)) = composite_id.split_once(':') else {
            continue;
        };
        if load_api_key_model(&state, &composite_id, provider_id_str, None)
            .await
            .is_ok()
        {
            loadable_api_key_ids.push(composite_id);
        }
    }

    // Filter installed_local down to those that actually have a
    // canonical file on disk right now.
    let mut loadable_local: Vec<String> = Vec::new();
    for slug in installed_local_ids {
        if let Ok(Some(_)) = paths::canonical_model_file(&app, &slug) {
            loadable_local.push(slug);
        }
    }
    let is_loadable = |id: &str| -> bool {
        loadable_api_key_ids.iter().any(|s| s == id) || loadable_local.iter().any(|s| s == id)
    };

    if let Some(id) = last_active.filter(|id| is_loadable(id)) {
        return Ok(ResolveDefaultModelResult {
            model_id: Some(id),
            source: ResolveSource::LastActive,
        });
    }
    if let Some(id) = default_device.filter(|id| is_loadable(id)) {
        return Ok(ResolveDefaultModelResult {
            model_id: Some(id),
            source: ResolveSource::DefaultDevice,
        });
    }
    if let Some(id) = default_vault.filter(|id| is_loadable(id)) {
        return Ok(ResolveDefaultModelResult {
            model_id: Some(id),
            source: ResolveSource::DefaultVault,
        });
    }
    // First-available: prefer a local model (no network), else any
    // api_key row. Deterministic within each bucket (models are
    // ordered by name; loadable_local mirrors that order).
    if let Some(id) = loadable_local.into_iter().next() {
        return Ok(ResolveDefaultModelResult {
            model_id: Some(id),
            source: ResolveSource::FirstAvailable,
        });
    }
    if let Some(id) = loadable_api_key_ids.into_iter().next() {
        return Ok(ResolveDefaultModelResult {
            model_id: Some(id),
            source: ResolveSource::FirstAvailable,
        });
    }
    Ok(ResolveDefaultModelResult {
        model_id: None,
        source: ResolveSource::None,
    })
}

/// Resolves only local candidates for the background Vault-open preload.
/// Provider preferences are skipped and the next fallback tier is checked.
async fn resolve_default_local_model(
    app: &AppHandle,
    state: &State<'_, AppState>,
) -> Result<Option<String>> {
    let db = active_database(state)?;
    let this_device = db.device_id();
    let (preferences_to_try, installed_local_ids) = db
        .read(move |r| {
            let last_active =
                preferences::get(r, PrefScope::Device(this_device), PREF_LAST_ACTIVE_MODEL)?;
            let default_device =
                preferences::get(r, PrefScope::Device(this_device), PREF_DEFAULT_MODEL)?;
            let default_vault = preferences::get(r, PrefScope::Vault, PREF_DEFAULT_MODEL)?;
            let local_ids = models_store::list_all_models(r)?
                .into_iter()
                .filter(|row| !row.id.contains(':'))
                .map(|row| row.id)
                .collect::<Vec<_>>();
            Ok::<_, haex_crdt::Error>(([last_active, default_device, default_vault], local_ids))
        })
        .await?;

    let loadable_local = installed_local_ids
        .into_iter()
        .filter(|id| {
            paths::canonical_model_file(app, id)
                .ok()
                .flatten()
                .is_some()
        })
        .collect::<Vec<_>>();
    let is_loadable = |id: &str| loadable_local.iter().any(|candidate| candidate == id);

    for preference in preferences_to_try.into_iter().flatten() {
        if !preference.contains(':') && is_loadable(&preference) {
            return Ok(Some(preference));
        }
    }
    Ok(loadable_local.into_iter().next())
}

/// Starts the single background preload for the currently-published Vault.
/// The task owns no operation mutex, so a Vault switch can cancel it promptly.
pub fn start_default_model_preload(app: AppHandle, chat: ChatState) {
    let cancel = CancellationToken::new();
    let task_cancel = cancel.clone();
    let task_chat = chat.clone();
    let task_app = app.clone();
    // Tracked by the gate, so the close waits for it and aborts it; it also keeps its own token
    // and join handle, which a vault switch used before and the close still cancels through.
    let spawned = app.state::<VaultGate>().spawn(async move {
        let state = task_app.state::<AppState>();
        let model_id = match resolve_default_local_model(&task_app, &state).await {
            Ok(Some(model_id)) => model_id,
            Ok(None) | Err(_) => return,
        };
        if task_cancel.is_cancelled() {
            return;
        }

        let (vault_generation, load_id) = task_chat.begin_model_load();
        let identity = LoadIdentity {
            vault_generation,
            load_id,
        };
        match load_model_inner(
            &task_app,
            &state,
            &task_chat,
            &model_id,
            identity,
            Some(&task_cancel),
            false,
        )
        .await
        {
            Ok(LoadOutcome::Loaded(_)) | Ok(LoadOutcome::Cancelled) => {}
            Err(error) if !task_cancel.is_cancelled() => {
                let model_name = resolve_display_name(&state, &model_id).await;
                if task_chat.set_model_error(
                    vault_generation,
                    load_id,
                    Some(model_id.clone()),
                    model_name.clone(),
                    "model_load_failed".into(),
                ) {
                    emit_load_error(
                        &task_app,
                        vault_generation,
                        load_id,
                        Some(model_id),
                        model_name,
                        "model_load_failed",
                    );
                }
                log::warn!("background model preload failed: {error}");
            }
            Err(_) => {}
        }
    });
    match spawned {
        Ok(join) => chat.install_preload_handle(cancel, join),
        Err(_) => log::info!("no preload: the vault is closing"),
    }
}

/// Read-only snapshot of the active Vault's model-load state.
#[tauri::command]
pub async fn model_load_status(chat: State<'_, ChatState>) -> Result<ModelLoadStatus> {
    Ok(chat.model_load_status())
}
