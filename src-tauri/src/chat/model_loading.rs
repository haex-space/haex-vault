//! Model-lifecycle Tauri commands: loading a remote or local model into
//! the active session (with the mandatory local integrity check),
//! unloading it, and reporting which one is active.
//!
//! Split out of `chat/commands.rs` (2026-09-15 review) — second of five
//! steps. See `chat/commands.rs`'s own history for the rest of the split
//! plan.
//!
//! Maintainability exception (spaex 500-LoC rule): at ~730 lines this is
//! well over the limit on its own. `resolve_local_model_metadata` and
//! `verify_local_model_integrity` (roughly 170 of these lines) are a
//! self-contained pair — the mandatory pre-load SHA-256 check and its
//! `integrity_override` bypass — that only needs `AppState`/`paths`/
//! `models_store`, not `ChatState` or the events layer the rest of this
//! file shares with `load_model_inner`.
//!
//! Concrete split plan, if this grows further: move
//! `resolve_local_model_metadata` and `verify_local_model_integrity` to a
//! new `chat/model_integrity.rs` — that alone gets this file to roughly
//! 560 lines, still over the limit, so a second extraction (e.g. the
//! display-name/provider-name resolution helpers) would be needed to
//! actually reach 500.

use std::sync::Arc;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::error::{HolziError, Result};
#[cfg(feature = "llm-llamacpp")]
use crate::models::paths;
use crate::providers::build_adapter;
use crate::state::AppState;
use crate::state_utils::active_database;
use crate::storage::preferences::{self, PrefScope};
use crate::storage::{models as models_store, providers as providers_store};

use super::commands::PREF_LAST_ACTIVE_MODEL;
use super::events::{emit_load_error, emit_load_progress};
use super::session::{ActiveSession, ChatState};
use super::tools::selftest::spawn_tool_use_check;

/// Semantic phase of a model-load. See spec 002 §FR-015b + contracts.
/// Backend never emits localised strings; frontend translates via
/// `chat.loading.<phase>` (FR-020 i18n boundary).
// `CudaJitWarmup` is only constructed on CUDA builds —
// `dead_code` fires on default-feature builds. The variant is part of
// the wire contract (spec 002 §FR-015b), so silence the lint rather
// than hide the enum behind a cfg.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum LoadPhase {
    Connecting,
    Loading,
    CudaJitWarmup,
    Ready,
}

/// Payload for `active_model_info` and `load_model`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LoadedModelInfo {
    pub model_id: String,
    pub name: String,
    pub tokenizer_repo: String,
    pub context_window: Option<i64>,
    /// The local engine that runs the model (spec 070); absent for a remote model.
    pub engine: Option<&'static str>,
    /// Where a local model went and what its context is made of (spec 086); absent for a remote model.
    pub placement: Option<crate::adapters::PlacementInfo>,
}

/// What a load is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LoadMode {
    /// The model of the chat session: a provider's model, or the active local model, which the
    /// chat picks up when it is in memory and loads on first use when it is not. Another local
    /// model is refused: the settings activate it.
    Chat,
    /// The active local model: loaded into memory, apart from the chat's session. The settings
    /// do this; a chat load of the active model that is not in memory yet does it on the way.
    Activate,
}

/// How a load runs: what it is for, and whether the failed integrity check of a local model is
/// bypassed (only the explicit "load anyway" of the integrity dialog).
#[derive(Debug, Clone, Copy)]
pub(crate) struct LoadOptions {
    pub(crate) mode: LoadMode,
    pub(crate) integrity_override: bool,
}

impl LoadOptions {
    #[cfg(feature = "llm-llamacpp")]
    pub(crate) fn new(mode: LoadMode) -> Self {
        Self {
            mode,
            integrity_override: false,
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) struct LoadIdentity {
    pub(crate) vault_generation: u64,
    pub(crate) load_id: u64,
}

pub(crate) enum LoadOutcome {
    Loaded(LoadedModelInfo),
    Cancelled,
}

impl LoadPhase {
    fn as_str(self) -> &'static str {
        match self {
            Self::Connecting => "connecting",
            Self::Loading => "loading",
            Self::CudaJitWarmup => "cuda-jit-warmup",
            Self::Ready => "ready",
        }
    }
}

/// Whether a newer load or a closed vault has replaced this one. A chat load that starts while a
/// local model is being activated does not replace the activation: the two are apart, and the
/// active model stays loaded whatever the chat chooses; only a closed vault ends an activation.
fn superseded(chat: &ChatState, identity: LoadIdentity, mode: LoadMode) -> bool {
    match mode {
        LoadMode::Chat => !chat.is_current_load(identity.vault_generation, identity.load_id),
        LoadMode::Activate => chat.vault_generation() != identity.vault_generation,
    }
}

fn cancelled(cancel: Option<&CancellationToken>) -> bool {
    cancel.is_some_and(CancellationToken::is_cancelled)
}

fn publish_load_phase(
    app: &AppHandle,
    chat: &ChatState,
    identity: LoadIdentity,
    model_id: &str,
    model_name: &str,
    phase: LoadPhase,
    provider_name: Option<String>,
) -> bool {
    if !chat.set_model_loading(
        identity.vault_generation,
        identity.load_id,
        model_id.to_string(),
        model_name.to_string(),
        phase.as_str().to_string(),
        provider_name.clone(),
    ) {
        return false;
    }
    emit_load_progress(
        app,
        identity.vault_generation,
        identity.load_id,
        model_id,
        model_name,
        phase,
        provider_name,
    );
    true
}

/// Shared implementation for the chat's model selection and the activation of a local model.
/// The operation mutex is deliberately owned by the caller.
pub(crate) async fn load_model_inner(
    app: &AppHandle,
    state: &State<'_, AppState>,
    chat: &ChatState,
    model_id: &str,
    identity: LoadIdentity,
    cancel: Option<&CancellationToken>,
    options: LoadOptions,
) -> Result<LoadOutcome> {
    let LoadOptions {
        mode,
        integrity_override,
    } = options;
    #[cfg(not(feature = "llm-llamacpp"))]
    let _ = integrity_override;

    if mode == LoadMode::Activate && model_id.contains(':') {
        return Err(HolziError::InvalidInput {
            reason: "only a local model can be activated".into(),
        });
    }
    if cancelled(cancel) || superseded(chat, identity, mode) {
        return Ok(LoadOutcome::Cancelled);
    }
    let name = resolve_display_name(state, model_id)
        .await
        .unwrap_or_else(|| model_id.to_string());

    // The chat takes the engine of the active local model when it is in memory; when it is not,
    // the chat's own load puts it there (and keeps it there, like an activation does).
    let held = if mode == LoadMode::Chat && !model_id.contains(':') {
        chat.session_for_local_model(model_id)
    } else {
        None
    };
    let loads_local = !model_id.contains(':') && held.is_none();

    let session = if let Some((provider_id_str, _remote_id)) = model_id.split_once(':') {
        let provider_name = resolve_provider_name(state, provider_id_str).await;
        if !publish_load_phase(
            app,
            chat,
            LoadIdentity { ..identity },
            model_id,
            &name,
            LoadPhase::Connecting,
            provider_name,
        ) || cancelled(cancel)
        {
            return Ok(LoadOutcome::Cancelled);
        }
        // This path only performs bounded database/adapter setup. Await it
        // to completion so cancellation cannot drop a spawn_blocking DB read
        // while a Vault close is waiting to release its Arc.
        let app_for_emit = app.clone();
        let emit: crate::adapters::cli_delegate::EventEmitter = Arc::new(move |event, payload| {
            let _ = app_for_emit.emit(event, payload);
        });
        let delegate_chat_ctx = Some(crate::adapters::cli_delegate::DelegateChatContext {
            pending_tool_approvals: Arc::clone(&chat.pending_tool_approvals),
            emit,
            database: Some(active_database(state)?),
            children: chat.children().clone(),
            // Spec 021 US6: a registered delegate gets holzi's own MCP server.
            holzi: app
                .try_state::<crate::mcp_server::McpServer>()
                .map(|server| server.delegate_access()),
            deny_rules: Vec::new(),
        });
        load_api_key_model(state, model_id, provider_id_str, delegate_chat_ctx).await?
    } else if let Some(held) = held {
        held
    } else {
        if mode == LoadMode::Chat
            && crate::chat::local_model::chosen_local_model(state)
                .await?
                .as_deref()
                != Some(model_id)
        {
            return Err(HolziError::InvalidInput {
                reason: "the local model is not active".into(),
            });
        }
        #[cfg(feature = "llm-llamacpp")]
        {
            let phase = local_load_phase();
            // A chat load that took over the status meanwhile is no reason to stop the activation.
            publish_load_phase(
                app,
                chat,
                LoadIdentity { ..identity },
                model_id,
                &name,
                phase,
                None,
            );
            if cancelled(cancel) {
                return Ok(LoadOutcome::Cancelled);
            }
            // Resolve all database metadata before entering the cancellable
            // model load. This guarantees that close never returns while a
            // detached spawn_blocking task still retains the database Arc.
            let metadata =
                resolve_local_model_metadata(app, state, model_id, integrity_override).await?;
            let future = load_local_model_from_metadata(model_id, metadata);
            match cancel {
                Some(cancel) => tokio::select! {
                    _ = cancel.cancelled() => return Ok(LoadOutcome::Cancelled),
                    result = future => result?,
                },
                None => future.await?,
            }
        }
        #[cfg(not(feature = "llm-llamacpp"))]
        {
            return Err(HolziError::InvalidInput {
                reason: "local inference is not enabled in this build".into(),
            });
        }
    };

    if cancelled(cancel) || superseded(chat, identity, mode) {
        return Ok(LoadOutcome::Cancelled);
    }

    // A local model nobody knows anything about is checked in the background (spec 032 FR-018b),
    // once, when it is loaded.
    let local_adapter = loads_local.then(|| Arc::clone(&session.adapter));

    let info = LoadedModelInfo {
        model_id: session.model_id.clone(),
        name,
        tokenizer_repo: session.tokenizer_repo.clone(),
        context_window: session.context_window,
        engine: session.engine,
        placement: session.adapter.placement_info(),
    };
    match mode {
        LoadMode::Chat => {
            if loads_local {
                // The model is in memory now: the active local model is this one, as after an
                // activation. Before the chat's session is set, so the session survives it.
                chat.cancel_tool_check();
                chat.replace_local_model(Some(session.clone()));
            }
            *chat.session.lock().map_err(|e| HolziError::CrdtInit {
                reason: format!("chat.session mutex poisoned: {e}"),
            })? = Some(session);
        }
        LoadMode::Activate => {
            // Another local model becomes the active one: the check of the last one is moot. Only
            // now, so a failed load leaves the check of the retained model running.
            chat.cancel_tool_check();
            chat.replace_local_model(Some(session));
        }
    }
    // An activation goes on when a chat load takes over the status meanwhile (`superseded`); its
    // `Ready` would then report the chat load as done while that one still runs.
    if chat.set_model_ready(
        identity.vault_generation,
        identity.load_id,
        info.model_id.clone(),
        info.name.clone(),
    ) {
        emit_load_progress(
            app,
            identity.vault_generation,
            identity.load_id,
            model_id,
            &info.name,
            LoadPhase::Ready,
            None,
        );
    }
    if let Some(adapter) = local_adapter {
        let token = chat.begin_tool_check();
        spawn_tool_use_check(app, info.model_id.clone(), adapter, token);
    }
    Ok(LoadOutcome::Loaded(info))
}

/// Loads the model of the chat session: a provider's model (`<provider_id>:<remote_id>`), or the
/// active local model, which the chat picks up when it is in memory and loads when it is not (the
/// first use of the model). A local model that is not active is refused: the settings activate it
/// ([`super::local_model::activate_local_model`]). If another model was the session's, it is
/// dropped first.
#[tauri::command]
pub async fn load_model(
    app: AppHandle,
    state: State<'_, AppState>,
    chat: State<'_, ChatState>,
    model_id: String,
) -> Result<LoadedModelInfo> {
    load_model_command(app, state, chat, model_id, false, LoadMode::Chat).await
}

/// `select_model`: the person picks a model in the model menu. Loads it like [`load_model`] and,
/// once it is loaded, remembers it as the last picked model (`chat.last_active_model_id`), which a
/// new session starts with (spec 002 FR-009 as amended 2026-11-02). Only this pick counts: a
/// session that is continued with its own model, a retry, a download or the load of the active
/// local model do not change it. With `thread_id` (the session open in the window) the pick becomes that
/// session's model too; otherwise its older model would win again before the next message, typed
/// or spoken (operator 2026-11-06).
#[tauri::command]
pub async fn select_model(
    app: AppHandle,
    state: State<'_, AppState>,
    chat: State<'_, ChatState>,
    model_id: String,
    thread_id: Option<Uuid>,
) -> Result<LoadedModelInfo> {
    let info = load_model_command(
        app,
        state.clone(),
        chat,
        model_id.clone(),
        false,
        LoadMode::Chat,
    )
    .await?;
    remember_picked_model(&state, model_id.clone()).await?;
    if let Some(thread_id) = thread_id {
        set_session_model(&state, thread_id, model_id).await?;
    }
    Ok(info)
}

/// Makes `model_id` the model of session `thread_id`; a provider model (`provider:model`) also
/// sets the session's provider.
async fn set_session_model(
    state: &State<'_, AppState>,
    thread_id: Uuid,
    model_id: String,
) -> Result<()> {
    let provider_id = model_id
        .split_once(':')
        .and_then(|(provider, _)| Uuid::parse_str(provider).ok());
    let db = active_database(state)?;
    db.write(move |tx| {
        crate::storage::chat_threads::set_thread_model(tx, thread_id, provider_id, &model_id)
            .and_then(|changed| {
                if changed == 1 {
                    Ok(())
                } else {
                    Err(haex_crdt::Error::consumer(
                        "the chat session is no longer available",
                    ))
                }
            })
    })
    .await?;
    Ok(())
}

/// Writes `model_id` as the last picked model of this device.
pub(crate) async fn remember_picked_model(
    state: &State<'_, AppState>,
    model_id: String,
) -> Result<()> {
    let db = active_database(state)?;
    let device = db.device_id();
    db.write(move |tx| {
        preferences::insert_or_update(
            tx,
            PrefScope::Device(device),
            PREF_LAST_ACTIVE_MODEL,
            &model_id,
        )
        .map(|_| ())
    })
    .await?;
    Ok(())
}

pub(super) async fn load_model_command(
    app: AppHandle,
    state: State<'_, AppState>,
    chat: State<'_, ChatState>,
    model_id: String,
    integrity_override: bool,
    mode: LoadMode,
) -> Result<LoadedModelInfo> {
    let _operation = chat.acquire_operation()?;
    let (vault_generation, load_id) = chat.begin_model_load();
    let identity = LoadIdentity {
        vault_generation,
        load_id,
    };
    let load = load_model_inner(
        &app,
        &state,
        &chat,
        &model_id,
        identity,
        None,
        LoadOptions {
            mode,
            integrity_override,
        },
    );
    match state.gate().run(load).await? {
        Ok(LoadOutcome::Loaded(info)) => Ok(info),
        Ok(LoadOutcome::Cancelled) => Err(HolziError::InvalidInput {
            reason: "model load was cancelled".into(),
        }),
        Err(error) => {
            chat.set_model_error(
                vault_generation,
                load_id,
                Some(model_id.clone()),
                None,
                "model_load_failed".into(),
            );
            emit_load_error(
                &app,
                vault_generation,
                load_id,
                Some(model_id),
                None,
                "model_load_failed",
            );
            Err(error)
        }
    }
}

/// Returns the provider's display name for a `<provider_uuid>:...`
/// composite id. Best-effort — `None` when the row is missing.
async fn resolve_provider_name(
    state: &State<'_, AppState>,
    provider_id_str: &str,
) -> Option<String> {
    let provider_id = Uuid::parse_str(provider_id_str).ok()?;
    let db = active_database(state).ok()?;
    db.read(move |r| Ok(providers_store::get_provider(r, provider_id)?.map(|p| p.name)))
        .await
        .ok()?
}

/// Picks the load-phase for a local model.
///
/// Heuristic: on a CUDA build without an existing NV compute cache we
/// emit `cuda-jit-warmup` (the ~30 s kernel-JIT window per Etappe-0
/// finding #4). On CPU/Metal builds we always emit `loading` — no
/// comparable warmup exists there (spec 002 §FR-015c).
#[cfg(feature = "llm-llamacpp")]
fn local_load_phase() -> LoadPhase {
    #[cfg(feature = "llm-llamacpp-cuda")]
    {
        if let Some(home) = std::env::var_os("HOME") {
            let cache = std::path::PathBuf::from(home).join(".nv/ComputeCache");
            let has_cache = std::fs::read_dir(&cache)
                .map(|mut it| it.next().is_some())
                .unwrap_or(false);
            if !has_cache {
                return LoadPhase::CudaJitWarmup;
            }
        } else {
            // No HOME → we cannot tell. Prefer the warmup label so the
            // user is not surprised by a long first-load without an
            // explanation.
            return LoadPhase::CudaJitWarmup;
        }
    }
    LoadPhase::Loading
}

/// Resolves a provider-qualified model id and builds its remote adapter session.
///
/// `delegate_chat_ctx` is only actually needed for a `cli_delegate`
/// provider — it becomes that adapter's live approval-bridge handles
/// (spec 007-cli-delegate data-model.md "Signatur-Änderung"); an
/// `api_key` provider's `build_adapter` branch ignores it. Pass `None`
/// from a call site that only checks whether a model *would* load
/// (`default_model.rs`'s loadability scan never calls `stream_chat`).
pub(crate) async fn load_api_key_model(
    state: &State<'_, AppState>,
    composite_id: &str,
    provider_id_str: &str,
    delegate_chat_ctx: Option<crate::adapters::cli_delegate::DelegateChatContext>,
) -> Result<ActiveSession> {
    let provider_id = Uuid::parse_str(provider_id_str).map_err(|_| HolziError::InvalidInput {
        reason: format!("bad composite model id: {composite_id}"),
    })?;
    let db = active_database(state)?;
    let id_owned = composite_id.to_string();
    let (provider, row) = db
        .read(move |r| {
            let provider = providers_store::get_provider(r, provider_id)?.ok_or_else(|| {
                haex_crdt::Error::from(haex_crdt::rusqlite::Error::QueryReturnedNoRows)
            })?;
            let row = models_store::get_model(r, &id_owned)?.ok_or_else(|| {
                haex_crdt::Error::from(haex_crdt::rusqlite::Error::QueryReturnedNoRows)
            })?;
            Ok((provider, row))
        })
        .await
        .map_err(|_| HolziError::ModelNotFound {
            id: composite_id.to_string(),
        })?;

    let provider = crate::providers::repair_legacy_adapter(&db, &provider).await?;
    let provider_kind = provider.kind;
    let adapter = build_adapter(&provider, delegate_chat_ctx)?;
    Ok(ActiveSession {
        model_id: composite_id.to_string(),
        provider_id: Some(provider_id),
        provider_kind,
        adapter: Arc::from(adapter),
        tokenizer_repo: String::new(),
        context_window: row.context_window,
        engine: None,
    })
}

/// Loads an installed local model and wraps it in an adapter-backed session.
///
/// Resolves the on-disk file via [`paths::canonical_model_file`] rather
/// than a persisted `relative_path` — the filesystem is authoritative
/// for what is installed (spec 002 §"Installed model"). Metadata
/// (`context_window`, `tokenizer_repo`) still comes from the
/// synchronised `models` catalog row.
///
/// `integrity_override` is `true` only for the explicit
/// `activate_local_model_with_integrity_override` path (contracts/tauri-commands.md
/// §"load_model und lokale Integritätsprüfung"): it skips the blocking
/// hash comparison and marks the row `untrusted` instead of `verified`,
/// but never skips the file-existence check and never rewrites
/// `file_sha256` — only a successful download/update/import may do that
/// (research.md Entscheidung 6).
#[cfg(feature = "llm-llamacpp")]
async fn resolve_local_model_metadata(
    app: &AppHandle,
    state: &State<'_, AppState>,
    model_id: &str,
    integrity_override: bool,
) -> Result<LocalModelMetadata> {
    let canonical =
        paths::canonical_model_file(app, model_id)?.ok_or_else(|| HolziError::ModelNotFound {
            id: model_id.to_string(),
        })?;

    let db = active_database(state)?;
    let id_owned = model_id.to_string();
    let row = db
        .read(move |r| {
            let row = models_store::get_model(r, &id_owned)?.ok_or_else(|| {
                haex_crdt::Error::from(haex_crdt::rusqlite::Error::QueryReturnedNoRows)
            })?;
            Ok(row)
        })
        .await
        .map_err(|_| HolziError::ModelNotFound {
            id: model_id.to_string(),
        })?;

    if integrity_override {
        // Persist the explicit safety decision before loading. If this
        // write fails, continuing would create a successful load that is
        // no longer visibly marked as untrusted.
        let db = active_database(state)?;
        let id_for_update = model_id.to_string();
        let status_update = db
            .write(move |tx| {
                models_store::set_integrity_status(
                    tx,
                    &id_for_update,
                    models_store::IntegrityStatus::Untrusted,
                )
            })
            .await
            .map_err(|e| HolziError::ModelIntegrityError {
                model_id: model_id.to_string(),
                reason: format!("persisting untrusted status: {e}"),
            })?;
        if status_update == 0 {
            return Err(HolziError::ModelIntegrityError {
                model_id: model_id.to_string(),
                reason: "model row disappeared while marking it untrusted".into(),
            });
        }
    } else {
        verify_local_model_integrity(
            state,
            model_id,
            &canonical.absolute_path,
            row.file_sha256.as_deref(),
        )
        .await?;
    }

    // Prefer the persisted `tokenizer_repo` (migration 0009 onwards).
    // The catalog fallback covers pre-0009 rows the lazy backfill in
    // `list_installed_models` has not yet touched.
    let tokenizer_repo = row
        .tokenizer_repo
        .or_else(|| crate::catalog::get(model_id).map(|e| e.tokenizer_repo.clone()))
        .ok_or_else(|| HolziError::InvalidInput {
            reason: format!("tokenizer_repo missing for model {model_id}"),
        })?;

    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    let drafter = crate::models::drafter::for_load(paths::models_root(app)?, model_id).await;
    #[cfg(any(target_os = "android", target_os = "ios"))]
    let drafter = None;
    // What this device's person chose for the model's context (spec 086); Auto while nothing is.
    let device = db.device_id();
    let setting_id = model_id.to_string();
    let setting = db
        .read(move |r| crate::models::context_setting::read(r, device, &setting_id))
        .await?;
    Ok((
        canonical.absolute_path,
        row.context_window,
        tokenizer_repo,
        drafter,
        setting,
    ))
}

/// Model file, context window, tokenizer repository, installed drafter and the context setting of this
/// device of a local model.
#[cfg(feature = "llm-llamacpp")]
type LocalModelMetadata = (
    std::path::PathBuf,
    Option<i64>,
    String,
    Option<std::path::PathBuf>,
    crate::models::context_setting::ContextSetting,
);

/// Mandatory pre-load integrity gate for local models (contracts/
/// tauri-commands.md §"load_model und lokale Integritätsprüfung",
/// research.md Entscheidung 6). Recomputes the full SHA-256 of the
/// canonical file in a blocking task and compares it against
/// `models.file_sha256`. Only an exact match lets the normal load
/// proceed; every other outcome returns a structured error the frontend
/// renders as the integrity decision dialog (`load_untrusted` /
/// `repair_source` / `choose_other`).
#[cfg(feature = "llm-llamacpp")]
async fn verify_local_model_integrity(
    state: &State<'_, AppState>,
    model_id: &str,
    absolute_path: &std::path::Path,
    expected_sha256: Option<&str>,
) -> Result<()> {
    let Some(expected) = expected_sha256.filter(|s| crate::models::hash::is_valid_sha256_hex(s))
    else {
        return Err(HolziError::ModelIntegrityUnknown {
            model_id: model_id.to_string(),
            expected_sha256: expected_sha256.map(str::to_string),
        });
    };

    let path_owned = absolute_path.to_path_buf();
    let hash_result =
        tauri::async_runtime::spawn_blocking(move || crate::models::hash::sha256_file(&path_owned))
            .await
            .map_err(|e| HolziError::ModelIntegrityError {
                model_id: model_id.to_string(),
                reason: format!("hashing task join: {e}"),
            })?;

    let actual = hash_result.map_err(|e| HolziError::ModelIntegrityError {
        model_id: model_id.to_string(),
        reason: e.to_string(),
    })?;

    if actual != expected {
        // Keep the persisted status aligned with the failed check. This is
        // best-effort because the mismatch itself must remain the reported
        // error and the expected file hash must never be rewritten here.
        if let Ok(db) = active_database(state) {
            let id_for_update = model_id.to_string();
            let _ = db
                .write(move |tx| {
                    models_store::set_integrity_status(
                        tx,
                        &id_for_update,
                        models_store::IntegrityStatus::Untrusted,
                    )
                })
                .await;
        }
        return Err(HolziError::ModelIntegrityMismatch {
            model_id: model_id.to_string(),
            expected_sha256: expected.to_string(),
            actual_sha256: actual,
        });
    }

    // Match — record the confirmation so the model list reflects the
    // latest check. Never touches `file_sha256` itself (Entscheidung 6).
    // Best-effort: a failure here must not turn a successful integrity
    // check into a blocked load.
    if let Ok(db) = active_database(state) {
        let id_for_update = model_id.to_string();
        let _ = db
            .write(move |tx| {
                models_store::set_integrity_status(
                    tx,
                    &id_for_update,
                    models_store::IntegrityStatus::Verified,
                )
            })
            .await;
    }
    Ok(())
}

#[cfg(feature = "llm-llamacpp")]
async fn load_local_model_from_metadata(
    model_id: &str,
    (path, context_window, tokenizer_repo, drafter, setting): LocalModelMetadata,
) -> Result<ActiveSession> {
    // The engine follows from the file's own architecture (spec 070); a model no compiled engine
    // loads fails here with a message, not inside the engine.
    let (adapter, engine) = crate::llm::engine::load_local(
        &path,
        crate::llm::engine::LoadRequest {
            context_window: context_window.and_then(|n| u32::try_from(n).ok()),
            drafter: drafter.as_deref(),
            choice: setting.choice(),
            cache: setting.cache,
        },
    )
    .await
    .map_err(|e| HolziError::ModelDownload {
        reason: format!("local model load: {e}"),
    })?;

    Ok(ActiveSession {
        model_id: model_id.to_string(),
        provider_id: None,
        provider_kind: crate::storage::providers::ProviderKind::Local,
        adapter,
        tokenizer_repo,
        context_window,
        engine: Some(engine.name()),
    })
}

/// Loads an installed local model into a session of its own, apart from the chat's: the model of a
/// job that does not take the chat's place (spec 055 FR-033). Runs the same integrity check as a
/// load in the chat.
#[cfg(feature = "llm-llamacpp")]
pub(crate) async fn load_local_session(
    app: &AppHandle,
    state: &State<'_, AppState>,
    model_id: &str,
) -> Result<ActiveSession> {
    let metadata = resolve_local_model_metadata(app, state, model_id, false).await?;
    load_local_model_from_metadata(model_id, metadata).await
}

/// Returns the cached display name for a model when its row can be read.
pub(crate) async fn resolve_display_name(
    state: &State<'_, AppState>,
    model_id: &str,
) -> Option<String> {
    let db = match active_database(state) {
        Ok(db) => db,
        Err(e) => {
            log::warn!("resolve_display_name: no active database for {model_id}: {e}");
            return None;
        }
    };
    let id_owned = model_id.to_string();
    let query = db
        .read(move |r| Ok(models_store::get_model(r, &id_owned)?.map(|r| r.name)))
        .await;
    // Both branches fall back to `None` (the caller then shows the raw
    // model id), but a failed read logs first: it is a real bug worth
    // noticing, not the same "this model just has no cached row" case a
    // plain lookup miss is.
    match query {
        Ok(name) => name,
        Err(e) => {
            log::warn!("resolve_display_name: db read failed for {model_id}: {e}");
            None
        }
    }
}

/// Introspection — `None` when no model is loaded. `name` is resolved the
/// same way `load_model_inner` resolves it (`resolve_display_name`), not
/// read off `ActiveSession` directly — that struct carries no display-name
/// field of its own, only `model_id`, so a naive read used to surface the
/// raw id (a `<provider-uuid>:<remote-id>` composite for `cli_delegate`
/// models) as the "name" instead.
///
/// Read-only, so it deliberately does not take the operation slot
/// (`ChatState::acquire_operation`): that slot is an exclusive try-lock for
/// loads, vault transitions and whole turns, so a reader holding it made two
/// overlapping reads — the chat page issues one from the load-status handler
/// and one from `initialize()` — reject each other, and made every read fail
/// while a turn was running.
#[tauri::command]
pub async fn active_model_info(
    state: State<'_, AppState>,
    chat: State<'_, ChatState>,
) -> Result<Option<LoadedModelInfo>> {
    let session = {
        let guard = chat.session.lock().map_err(|e| HolziError::CrdtInit {
            reason: format!("chat.session mutex poisoned: {e}"),
        })?;
        guard.clone()
    };
    let Some(session) = session else {
        return Ok(None);
    };
    Ok(Some(describe(&state, session).await))
}

/// What the frontend shows of a loaded model; `name` is resolved like [`load_model_inner`] does.
pub(crate) async fn describe(
    state: &State<'_, AppState>,
    session: ActiveSession,
) -> LoadedModelInfo {
    let name = resolve_display_name(state, &session.model_id)
        .await
        .unwrap_or_else(|| session.model_id.clone());
    let placement = session.adapter.placement_info();
    LoadedModelInfo {
        model_id: session.model_id,
        name,
        tokenizer_repo: session.tokenizer_repo,
        context_window: session.context_window,
        engine: session.engine,
        placement,
    }
}
