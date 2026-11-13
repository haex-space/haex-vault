//! Model resolution: the session-start resolver chain (`resolve_default_model`; there is no
//! default model, only the last model picked) and a read-only snapshot of the current load state.
//! A vault open loads no model: a local model goes into memory when a session or a job needs it.
//!
//! Split out of `chat/commands.rs` (2026-09-15 review) — fifth and last of
//! five steps. See `chat/commands.rs`'s own history for the rest of the
//! split plan.

use serde::Serialize;
use tauri::{AppHandle, State};
use uuid::Uuid;

use crate::error::{HolziError, Result};
use crate::models::paths;
use crate::state::AppState;
use crate::state_utils::active_database;
use crate::storage::{chat_threads as thread_store, preferences, preferences::PrefScope};

use super::commands::PREF_LAST_ACTIVE_MODEL;
use super::local_model::PREF_ACTIVE_LOCAL_MODEL;
use super::model_loading::{load_api_key_model, load_model};
use super::session::{ChatState, ModelLoadStatus};

/// Which fallback branch the session resolver picked. Wire payload for
/// `resolve_default_model`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ResolveSource {
    /// The model the thread used last: a continued session keeps its model.
    Session,
    LastActive,
    /// Nothing loadable was picked: the person chooses.
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
/// There is no default model: without a loadable last pick the result is
/// `None` and the person chooses.
#[tauri::command]
pub async fn resolve_default_model(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<ResolveDefaultModelResult> {
    resolve_model(&app, &state, None).await
}

/// The model a session uses: the one `thread_id` used last, while it can still load; otherwise,
/// and for a new session, the chain of [`resolve_default_model`] (the last model picked in the
/// model menu first). A local model counts while it is the active one and its file is there; it
/// need not be in memory yet, the load follows the pick. Whether the agent is driven by text or
/// voice makes no difference.
pub(crate) async fn resolve_model(
    app: &AppHandle,
    state: &State<'_, AppState>,
    thread_id: Option<Uuid>,
) -> Result<ResolveDefaultModelResult> {
    let db = active_database(state)?;
    let this_device = db.device_id();
    let (session, last_active, active_local) = db
        .read(move |r| {
            // 0. the model the continued session used last.
            let session = match thread_id {
                Some(id) => thread_store::get_thread(r, id)?.and_then(|t| t.last_model_id),
                None => None,
            };
            // 1. last_active on this device.
            let last_active =
                preferences::get(r, PrefScope::Device(this_device), PREF_LAST_ACTIVE_MODEL)?;
            let active_local =
                preferences::get(r, PrefScope::Device(this_device), PREF_ACTIVE_LOCAL_MODEL)?;
            let candidate_pref = |value: Option<String>| value.filter(|s| !s.is_empty());
            Ok::<_, haex_crdt::Error>((
                candidate_pref(session),
                candidate_pref(last_active),
                active_local,
            ))
        })
        .await?;

    // Only a model somebody chose can be the answer, so only these are checked for loadability.
    // An API-key candidate goes through the same provider-row, model-row, credential,
    // legacy-repair, and adapter checks used by load_model; a local one must be the active model
    // with its file on this device.
    let mut loadable_local: Vec<String> = Vec::new();
    let mut loadable_api_key_ids = Vec::new();
    for composite_id in [&session, &last_active].into_iter().flatten() {
        if let Some((provider_id_str, _)) = composite_id.split_once(':') {
            if load_api_key_model(state, composite_id, provider_id_str, None)
                .await
                .is_ok()
            {
                loadable_api_key_ids.push(composite_id.clone());
            }
        } else if local_pick_is_usable(
            active_local.as_deref(),
            composite_id,
            matches!(paths::canonical_model_file(app, composite_id), Ok(Some(_))),
        ) {
            loadable_local.push(composite_id.clone());
        }
    }
    Ok(choose_model(
        [session, last_active],
        loadable_local,
        loadable_api_key_ids,
    ))
}

/// Whether a picked local model can be the chat's: it is the active local model of this device
/// and its file is there. Whether it is in memory does not matter, the chat loads it on use.
pub(crate) fn local_pick_is_usable(active: Option<&str>, picked: &str, file_present: bool) -> bool {
    file_present && active == Some(picked)
}

/// The first loadable of the preferred models, in the order session, last picked; otherwise
/// none: nothing is chosen on the person's behalf.
pub(crate) fn choose_model(
    preferred: [Option<String>; 2],
    loadable_local: Vec<String>,
    loadable_api: Vec<String>,
) -> ResolveDefaultModelResult {
    const SOURCES: [ResolveSource; 2] = [ResolveSource::Session, ResolveSource::LastActive];
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
    ResolveDefaultModelResult {
        model_id: None,
        source: ResolveSource::None,
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

/// Read-only snapshot of the active Vault's model-load state.
#[tauri::command]
pub async fn model_load_status(chat: State<'_, ChatState>) -> Result<ModelLoadStatus> {
    Ok(chat.model_load_status())
}

#[cfg(test)]
#[path = "default_model_tests.rs"]
mod tests;
