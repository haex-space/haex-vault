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
use uuid::Uuid;

use crate::error::{HolziError, Result};
use crate::models::paths;
use crate::state::AppState;
use crate::state_utils::active_database;
use crate::storage::{
    chat_threads as thread_store, models as models_store, preferences, preferences::PrefScope,
};
use crate::vault_gate::VaultGate;

use super::commands::PREF_LAST_ACTIVE_MODEL;
use super::events::emit_load_error;
use super::model_loading::{
    load_api_key_model, load_model, load_model_inner, resolve_display_name, LoadIdentity,
    LoadOutcome,
};
use super::session::{ChatState, ModelLoadStatus};

pub(crate) const PREF_DEFAULT_MODEL: &str = "chat.default_model_id";

/// Which fallback branch the session resolver picked. Wire payload for
/// `resolve_default_model`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ResolveSource {
    /// The model the thread used last: a continued session keeps its model.
    Session,
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
    resolve_model(&app, &state, None).await
}

/// The model a session uses: the one `thread_id` used last, while it can still load; otherwise,
/// and for a new session, the chain of [`resolve_default_model`] (the last model picked in the
/// model menu first). Whether the agent is driven by text or voice makes no difference.
pub(crate) async fn resolve_model(
    app: &AppHandle,
    state: &State<'_, AppState>,
    thread_id: Option<Uuid>,
) -> Result<ResolveDefaultModelResult> {
    let db = active_database(state)?;
    let this_device = db.device_id();
    let (result, installed_local_ids, api_key_ids) = db
        .read(move |r| {
            // 0. the model the continued session used last.
            let session = match thread_id {
                Some(id) => thread_store::get_thread(r, id)?.and_then(|t| t.last_model_id),
                None => None,
            };
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
                    candidate_pref(session),
                    candidate_pref(last_active),
                    candidate_pref(default_device),
                    candidate_pref(default_vault),
                ),
                installed_local,
                api_key_ids,
            ))
        })
        .await?;
    let (session, last_active, default_device, default_vault) = result;

    // Validate API-key candidates through the same provider-row, model-row,
    // credential, legacy-repair, and adapter checks used by load_model.
    let mut loadable_api_key_ids = Vec::new();
    for composite_id in api_key_ids {
        let Some((provider_id_str, _)) = composite_id.split_once(':') else {
            continue;
        };
        if load_api_key_model(state, &composite_id, provider_id_str, None)
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
        if let Ok(Some(_)) = paths::canonical_model_file(app, &slug) {
            loadable_local.push(slug);
        }
    }
    Ok(choose_model(
        [session, last_active, default_device, default_vault],
        loadable_local,
        loadable_api_key_ids,
    ))
}

/// The first loadable of the preferred models, in the order session, last picked, device default,
/// vault default; otherwise the first loadable local model, then the first loadable API model.
pub(crate) fn choose_model(
    preferred: [Option<String>; 4],
    loadable_local: Vec<String>,
    loadable_api: Vec<String>,
) -> ResolveDefaultModelResult {
    const SOURCES: [ResolveSource; 4] = [
        ResolveSource::Session,
        ResolveSource::LastActive,
        ResolveSource::DefaultDevice,
        ResolveSource::DefaultVault,
    ];
    let is_loadable =
        |id: &str| loadable_api.iter().any(|s| s == id) || loadable_local.iter().any(|s| s == id);
    for (candidate, source) in preferred.into_iter().zip(SOURCES) {
        if let Some(id) = candidate.filter(|id| is_loadable(id)) {
            return ResolveDefaultModelResult {
                model_id: Some(id),
                source,
            };
        }
    }
    // First-available: prefer a local model (no network), else any
    // api_key row. Deterministic within each bucket (models are
    // ordered by name; loadable_local mirrors that order).
    let first = loadable_local
        .into_iter()
        .next()
        .or_else(|| loadable_api.into_iter().next());
    ResolveDefaultModelResult {
        source: if first.is_some() {
            ResolveSource::FirstAvailable
        } else {
            ResolveSource::None
        },
        model_id: first,
    }
}

/// `use_session_model`: loads the model of the session `thread_id` (`None`: a new session) when
/// another one is loaded, so a continued session answers with the model it used before and a new
/// one with the last model picked (spec 006, spec 002 FR-009 as amended 2026-11-02). Called when
/// the chat opens a session or starts a new one, and by the voice assistant before a round. A
/// model that can no longer load is skipped by [`resolve_model`], so opening an old session never
/// shows a load error for it.
#[tauri::command]
pub async fn use_session_model(
    app: AppHandle,
    state: State<'_, AppState>,
    chat: State<'_, ChatState>,
    thread_id: Option<Uuid>,
) -> Result<ResolveDefaultModelResult> {
    let resolved = resolve_model(&app, &state, thread_id).await?;
    let Some(wanted) = resolved.model_id.clone() else {
        return Ok(resolved);
    };
    let loaded = chat
        .session
        .lock()
        .map_err(|e| HolziError::CrdtInit {
            reason: format!("chat.session mutex poisoned: {e}"),
        })?
        .as_ref()
        .map(|session| session.model_id.clone());
    if loaded.as_deref() != Some(wanted.as_str()) {
        load_model(app.clone(), state, chat, wanted).await?;
    }
    Ok(resolved)
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

#[cfg(test)]
#[path = "default_model_tests.rs"]
mod tests;
