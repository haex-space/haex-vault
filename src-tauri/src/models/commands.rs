//! Tauri commands for downloading, importing and managing local GGUFs.
//!
//! Every download command emits a `model-download-progress` event with
//! `{ modelId, bytesDownloaded, bytesTotal }` while the transfer runs
//! and a `model-download-complete` event on success. Errors surface
//! through the command return value; there is no "error" event.
//!
//! Maintainability exception (spaex 500-LoC rule): this module currently
//! keeps the existing local-model commands beside the shared HF publication
//! path so the feature can reuse one registration boundary. A follow-up
//! split should extract the HF command façade and the local listing/delete
//! façade once the journaled publication work is stabilized; doing that
//! during this implementation would create a second registration boundary
//! and make failure-atomic behavior harder to review.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};
use uuid::Uuid;

use crate::catalog;
use crate::chat::session::ChatState;
use crate::error::{HolziError, Result};
use crate::hardware::{self, classify, Fit, ModelFitInputs};
use crate::model_capabilities::ModelCapabilities;
use crate::providers::local::ensure_local_provider;
use crate::state::AppState;
use crate::state_utils::active_database;
use crate::storage::models::{self as models_store, IntegrityStatus, ModelRow, SourceKind};
use crate::vault_gate::VaultDb;

use super::{download, hash, huggingface, import, paths};

pub(crate) const EVENT_PROGRESS: &str = "model-download-progress";
const EVENT_COMPLETE: &str = "model-download-complete";

/// Payload for the frontend: joins the model row and the local file
/// info when present.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledModelPayload {
    pub id: String,
    pub name: String,
    pub provider_id: String,
    pub context_window: Option<i64>,
    pub relative_path: String,
    pub size_bytes: i64,
    pub source_kind: SourceKind,
    pub hf_repo: Option<String>,
    pub hf_filename: Option<String>,
    pub hf_revision: Option<String>,
    pub hf_revision_ref: Option<String>,
    pub file_sha256: Option<String>,
    pub integrity_status: IntegrityStatus,
    /// What this model supports (spec 012); `None` means not determined.
    pub capabilities: Option<ModelCapabilities>,
}

impl InstalledModelPayload {
    /// Projects the vault row and file metadata into the shared frontend payload.
    fn from_row(row: ModelRow, relative_path: String, size_bytes: i64) -> Self {
        Self {
            id: row.id,
            name: row.name,
            provider_id: row.provider_id.to_string(),
            context_window: row.context_window,
            relative_path,
            size_bytes,
            source_kind: row.source_kind,
            hf_repo: row.hf_repo,
            hf_filename: row.hf_filename,
            hf_revision: row.hf_revision,
            hf_revision_ref: row.hf_revision_ref,
            file_sha256: row.file_sha256,
            integrity_status: row.integrity_status,
            capabilities: row.capabilities,
        }
    }
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProgressEvent {
    pub(crate) model_id: String,
    pub(crate) bytes_downloaded: u64,
    pub(crate) bytes_total: Option<u64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadFromHfArgs {
    /// Stable id under which this model is stored. For the free HF flow
    /// this must equal `huggingface::derive_model_id(hf_repo, hf_filename)`
    /// — see [`download_model_from_hf`]. Catalog installs keep their
    /// curated id instead (data-model.md).
    pub id: String,
    pub name: String,
    pub hf_repo: String,
    pub hf_filename: String,
    /// Commit SHA resolved before this call — never a mutable ref.
    pub hf_revision: String,
    /// Optional tracked branch/tag for later update checks.
    pub hf_revision_ref: Option<String>,
    pub tokenizer_repo: String,
    pub context_window: Option<i64>,
    /// Required `true` to proceed when the file classifies as `TooBig`.
    pub force_too_big: Option<bool>,
    /// Forces a fresh source download for an explicit integrity repair,
    /// even when repository, filename and revision match the installed row.
    pub force_repair: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportModelArgs {
    pub id: String,
    pub name: String,
    /// The choice of the open dialog (spec 043: a path or a provider address).
    pub source_path: crate::files::PickedFile,
    pub tokenizer_repo: String,
    pub context_window: Option<i64>,
    /// The GGUF filename inside the managed slug dir. Defaults to the
    /// source file's basename if not given.
    pub filename: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewHuggingFaceInstallArgs {
    pub repo_id: String,
    pub filename: String,
    pub revision: Option<String>,
    pub tokenizer_repo: Option<String>,
    pub context_window: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HuggingFaceUpdateStatusPayload {
    pub model_id: String,
    pub repo_id: String,
    pub revision_ref: String,
    pub installed_revision: String,
    pub latest_revision: Option<String>,
    pub update_available: bool,
    pub checked_at: String,
    pub error_code: Option<String>,
}

/// `search_huggingface_models` (contracts/tauri-commands.md). `page` is
/// accepted for contract-shape compatibility but not yet wired to a real
/// Hub pagination cursor — research.md Entscheidung 10 scopes the MVP to a
/// single capped page of results. An omitted query returns the ten most
/// downloaded public GGUF repositories as the default discovery view.
#[tauri::command]
pub async fn search_huggingface_models(
    query: Option<String>,
    page: Option<u32>,
    limit: Option<u32>,
) -> Result<Vec<huggingface::HuggingFaceModelResult>> {
    if page.is_some_and(|p| p == 0) {
        return Err(HolziError::InvalidInput {
            reason: "page must be positive".into(),
        });
    }
    let hf = huggingface::HfClient::production()?;
    huggingface::search_models(&hf, query.as_deref(), limit.map(|l| l as usize)).await
}

/// `get_huggingface_model_details` (contracts/tauri-commands.md).
#[tauri::command]
pub async fn get_huggingface_model_details(
    repo_id: String,
    revision: Option<String>,
) -> Result<huggingface::HuggingFaceModelResult> {
    let hf = huggingface::HfClient::production()?;
    let hw = hardware::probe_async().await;
    huggingface::get_model_details(&hf, &hw, &repo_id, revision.as_deref()).await
}

/// `preview_huggingface_install` (contracts/tauri-commands.md). Read-only —
/// resolves the revision and classifies hardware fit, but never downloads
/// or persists anything.
#[tauri::command]
pub async fn preview_huggingface_install(
    args: PreviewHuggingFaceInstallArgs,
) -> Result<huggingface::InstallPreview> {
    let hf = huggingface::HfClient::production()?;
    let hw = hardware::probe_async().await;
    huggingface::preview_install(
        &hf,
        &hw,
        &args.repo_id,
        &args.filename,
        args.revision.as_deref(),
        args.tokenizer_repo.as_deref(),
        args.context_window.map(|c| c as u64),
    )
    .await
}

/// Downloads a model from the built-in catalog. Reuses the shared HF
/// download/registration path so catalog and free installs cannot drift —
/// still resolves `main` to a concrete commit SHA first (spec 005 review:
/// every local GGUF gets a pinned commit, not a mutable ref). Hardware fit
/// is informational only for curated entries (plan §"Lokale Inferenz":
/// "Hardware-Info ist Information, kein Gate"), so the `TooBig`
/// confirmation gate does not apply here.
#[tauri::command]
pub async fn download_model_from_catalog(
    app: AppHandle,
    state: State<'_, AppState>,
    chat: State<'_, ChatState>,
    catalog_id: String,
) -> Result<InstalledModelPayload> {
    let entry =
        catalog::get(&catalog_id)
            .cloned()
            .ok_or_else(|| HolziError::CatalogEntryNotFound {
                id: catalog_id.clone(),
            })?;
    let hf = huggingface::HfClient::production()?;
    let resolved = huggingface::resolve_revision(&hf, &entry.hf_repo, Some("main")).await?;
    let args = DownloadFromHfArgs {
        id: entry.id.clone(),
        name: entry.name.clone(),
        hf_repo: entry.hf_repo,
        hf_filename: entry.hf_filename,
        hf_revision: resolved.sha,
        hf_revision_ref: resolved.revision_ref,
        tokenizer_repo: entry.tokenizer_repo,
        context_window: Some(entry.context_window as i64),
        force_too_big: Some(true),
        force_repair: None,
    };
    download_from_hf_inner(app, state, chat, args, SourceKind::Catalog, false).await
}

/// Downloads a model from an arbitrary HuggingFace repo/filename pair
/// (the free discovery flow). `args.id` must be the deterministic id
/// derived from `(hfRepo, hfFilename)` — the same value
/// `preview_huggingface_install` returned — and `args.hfRevision` must
/// already be a resolved commit SHA. This is the trust boundary contract
/// point 1 in `contracts/tauri-commands.md`.
#[tauri::command]
pub async fn download_model_from_hf(
    app: AppHandle,
    state: State<'_, AppState>,
    chat: State<'_, ChatState>,
    args: DownloadFromHfArgs,
) -> Result<InstalledModelPayload> {
    huggingface::validate_repo_id(&args.hf_repo)?;
    huggingface::validate_gguf_filename(&args.hf_filename)?;
    huggingface::validate_repo_id(&args.tokenizer_repo)?;
    if let Some(revision_ref) = args.hf_revision_ref.as_deref() {
        huggingface::validate_revision_ref(revision_ref)?;
    }
    if !huggingface::is_commit_sha(&args.hf_revision) {
        return Err(HolziError::InvalidInput {
            reason: "hfRevision must already be a resolved commit SHA".into(),
        });
    }
    let expected_id = huggingface::derive_model_id(&args.hf_repo, &args.hf_filename);
    if args.id != expected_id {
        return Err(HolziError::InvalidInput {
            reason: "id does not match the id derived from hfRepo/hfFilename".into(),
        });
    }
    download_from_hf_inner(app, state, chat, args, SourceKind::Huggingface, true).await
}

/// Installs the latest checked update for an already-installed free HF
/// model, reusing its stored repo/filename/tokenizer and the same atomic
/// download path under the *same* model id (data-model.md: "Ein Update
/// ersetzt die Datei unter derselben Modell-ID"). The hardware
/// confirmation gate does not re-apply — the operator already accepted
/// this model's size when it was first installed.
#[tauri::command]
pub async fn install_huggingface_update(
    app: AppHandle,
    state: State<'_, AppState>,
    chat: State<'_, ChatState>,
    model_id: String,
) -> Result<InstalledModelPayload> {
    let db = active_database(&state)?;
    let id_for_lookup = model_id.clone();
    let row = db
        .read(move |r| models_store::get_model(r, &id_for_lookup))
        .await?
        .ok_or_else(|| HolziError::ModelNotFound {
            id: model_id.clone(),
        })?;

    if row.source_kind != SourceKind::Huggingface {
        return Err(HolziError::InvalidInput {
            reason: format!("{model_id} is not a free HuggingFace install"),
        });
    }
    let (Some(repo), Some(filename), Some(revision_ref), Some(tokenizer_repo)) = (
        row.hf_repo.clone(),
        row.hf_filename.clone(),
        row.hf_revision_ref.clone(),
        row.tokenizer_repo.clone(),
    ) else {
        return Err(HolziError::InvalidInput {
            reason: format!("{model_id} has no trackable HuggingFace revision ref"),
        });
    };

    let hf = huggingface::HfClient::production()?;
    let resolved = huggingface::resolve_revision(&hf, &repo, Some(&revision_ref)).await?;
    let args = DownloadFromHfArgs {
        id: model_id,
        name: row.name,
        hf_repo: repo,
        hf_filename: filename,
        hf_revision: resolved.sha,
        hf_revision_ref: resolved.revision_ref,
        tokenizer_repo,
        context_window: row.context_window,
        force_too_big: Some(true),
        force_repair: None,
    };
    download_from_hf_inner(app, state, chat, args, SourceKind::Huggingface, false).await
}

/// `check_huggingface_model_updates` (contracts/tauri-commands.md). Only
/// installed HF models with a stored `hfRevisionRef` are checked — a
/// direct SHA pin has nothing to track. Per-model HTTP failures are
/// reported inline via `errorCode` and never touch local state.
#[tauri::command]
pub async fn check_huggingface_model_updates(
    state: State<'_, AppState>,
) -> Result<Vec<HuggingFaceUpdateStatusPayload>> {
    let db = active_database(&state)?;
    let rows = db.read(move |r| models_store::list_all_models(r)).await?;

    let trackable: Vec<ModelRow> = rows
        .into_iter()
        .filter(|r| {
            r.source_kind == SourceKind::Huggingface
                && r.hf_repo.is_some()
                && r.hf_revision_ref.is_some()
                && r.hf_revision.is_some()
        })
        .collect();

    let hf = huggingface::HfClient::production()?;
    let mut out = Vec::with_capacity(trackable.len());
    for row in trackable {
        let repo_id = row.hf_repo.expect("filtered above");
        let revision_ref = row.hf_revision_ref.expect("filtered above");
        let installed_revision = row.hf_revision.expect("filtered above");
        let checked_at = iso8601_now();
        match huggingface::check_update(&hf, &repo_id, &revision_ref).await {
            Ok(latest) => {
                let update_available = latest != installed_revision;
                out.push(HuggingFaceUpdateStatusPayload {
                    model_id: row.id,
                    repo_id,
                    revision_ref,
                    installed_revision,
                    latest_revision: Some(latest),
                    update_available,
                    checked_at,
                    error_code: None,
                });
            }
            Err(e) => out.push(HuggingFaceUpdateStatusPayload {
                model_id: row.id,
                repo_id,
                revision_ref,
                installed_revision,
                latest_revision: None,
                update_available: false,
                checked_at,
                error_code: Some(error_kind_str(&e)),
            }),
        }
    }
    Ok(out)
}

/// Shared implementation behind `download_model_from_catalog`,
/// `download_model_from_hf` and `install_huggingface_update`. Callers have
/// already validated `args`; this owns the publication lock, the
/// idempotent-source and slug-conflict checks, the optional `TooBig` gate,
/// the transfer, and registration.
async fn download_from_hf_inner(
    app: AppHandle,
    state: State<'_, AppState>,
    chat: State<'_, ChatState>,
    args: DownloadFromHfArgs,
    source_kind: SourceKind,
    enforce_too_big: bool,
) -> Result<InstalledModelPayload> {
    let _operation = chat.acquire_operation()?;
    let db = active_database(&state)?;
    let _publication_lock =
        paths::acquire_model_publication_lock(&app, &args.id, &state.gate().token()).await?;

    if let Some(existing) = paths::canonical_model_file(&app, &args.id)? {
        if existing.filename != args.hf_filename {
            return Err(HolziError::InvalidInput {
                reason: format!(
                    "model slug '{}' already has finalized GGUF '{}'; cannot add '{}'",
                    args.id, existing.filename, args.hf_filename
                ),
            });
        }
        // Idempotent short-circuit (contract point 7): the exact same
        // source (repo + filename + revision) is already installed under
        // this id — skip the network round-trip entirely.
        let id_for_lookup = args.id.clone();
        let existing_row = db
            .read(move |r| models_store::get_model(r, &id_for_lookup))
            .await?;
        if let Some(row) = existing_row {
            let same_source = row.hf_repo.as_deref() == Some(args.hf_repo.as_str())
                && row.hf_filename.as_deref() == Some(args.hf_filename.as_str())
                && row.hf_revision.as_deref() == Some(args.hf_revision.as_str());
            if same_source && !args.force_repair.unwrap_or(false) {
                return Ok(InstalledModelPayload::from_row(
                    row,
                    existing.relative_path,
                    existing.size_bytes as i64,
                ));
            }
        }
    }

    if enforce_too_big && !args.force_too_big.unwrap_or(false) {
        let hf = huggingface::HfClient::production()?;
        if let Ok(Some(size_bytes)) =
            huggingface::lookup_file_size(&hf, &args.hf_repo, &args.hf_revision, &args.hf_filename)
                .await
        {
            let hw = hardware::probe_async().await;
            let fit = classify(
                &hw,
                ModelFitInputs {
                    file_size_bytes: size_bytes,
                    context_window: args.context_window.map(|c| c as u64),
                },
            );
            if fit == Fit::TooBig {
                return Err(HolziError::HardwareConfirmationRequired {
                    fit: "too_big".into(),
                });
            }
        }
    }

    let destination = paths::model_file_path(&app, &args.id, &args.hf_filename)?;
    let staging = staging_path(&destination);
    let relative = paths::relative_path(&args.id, &args.hf_filename)?;
    let url = huggingface::resolve_download_url(
        huggingface::DEFAULT_BASE_URL,
        &args.hf_repo,
        &args.hf_revision,
        &args.hf_filename,
    )?;

    let app_for_progress = app.clone();
    let model_id_for_progress = args.id.clone();

    let transfer = download::download_to_file(&url, staging, move |p| {
        let _ = app_for_progress.emit(
            EVENT_PROGRESS,
            ProgressEvent {
                model_id: model_id_for_progress.clone(),
                bytes_downloaded: p.bytes_downloaded,
                bytes_total: p.bytes_total,
            },
        );
    });
    let downloaded = state.gate().run(transfer).await??;

    let payload = register_downloaded(RegisterDownloadedArgs {
        db,
        id: args.id,
        name: args.name,
        relative,
        staged_path: downloaded.absolute_path,
        destination,
        size_bytes: downloaded.bytes_written as i64,
        context_window: args.context_window,
        tokenizer_repo: Some(args.tokenizer_repo),
        source_kind,
        hf_repo: Some(args.hf_repo),
        hf_filename: Some(args.hf_filename),
        hf_revision: Some(args.hf_revision),
        hf_revision_ref: args.hf_revision_ref,
    })
    .await?;

    let _ = app.emit(EVENT_COMPLETE, &payload);
    Ok(payload)
}

/// Imports a GGUF the operator chose in the file dialog.
#[tauri::command]
pub async fn import_model_from_file(
    app: AppHandle,
    state: State<'_, AppState>,
    chat: State<'_, ChatState>,
    args: ImportModelArgs,
) -> Result<InstalledModelPayload> {
    let _operation = chat.acquire_operation()?;
    let db = active_database(&state)?;
    let filename = args
        .filename
        .or_else(|| Some(crate::files::picked::display_name(&app, &args.source_path)))
        .filter(|name| !name.is_empty())
        .ok_or_else(|| HolziError::InvalidInput {
            reason: "source path has no filename".into(),
        })?;
    let _publication_lock =
        paths::acquire_model_publication_lock(&app, &args.id, &state.gate().token()).await?;
    if let Some(existing) = paths::canonical_model_file(&app, &args.id)? {
        if existing.filename != filename {
            return Err(HolziError::InvalidInput {
                reason: format!(
                    "model slug '{}' already has finalized GGUF '{}'; cannot add '{}'",
                    args.id, existing.filename, filename
                ),
            });
        }
    }
    let destination = paths::model_file_path(&app, &args.id, &filename)?;
    let staging = staging_path(&destination);
    let relative = paths::relative_path(&args.id, &filename)?;
    let copy = import::copy_into_managed(app.clone(), args.source_path, staging.clone());
    let bytes = state.gate().run(copy).await??;
    register_downloaded(RegisterDownloadedArgs {
        db,
        id: args.id,
        name: args.name,
        relative,
        staged_path: staging,
        destination,
        size_bytes: bytes as i64,
        context_window: args.context_window,
        tokenizer_repo: Some(args.tokenizer_repo),
        source_kind: SourceKind::Imported,
        hf_repo: None,
        hf_filename: None,
        hf_revision: None,
        hf_revision_ref: None,
    })
    .await
}

/// Lists all locally installed models on this device.
///
/// The filesystem under `<AppLocalData>/models/<slug>/<file.gguf>` is
/// authoritative for "installed here" (spec 002 §"Installed model" and
/// ADR-0001). Discovery scans that root, resolves each slug's canonical
/// file via [`paths::canonical_model_file`], and joins it with the
/// synchronised `models` catalog row. Slugs without a matching row —
/// and slugs whose canonical-file resolution errors (e.g. multiple
/// finalised GGUFs in one slug directory) — are skipped rather than
/// failing the whole call, mirroring the tolerance in
/// `resolve_default_model` so one broken slug cannot make the entire
/// installed-model list unavailable.
///
/// Lists installed models and joins their on-disk files with the vault
/// catalog. Metadata backfills run during vault-open maintenance, so this
/// read command does not commit or wake the sync service.
#[tauri::command]
pub async fn list_installed_models(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Vec<InstalledModelPayload>> {
    let db = active_database(&state)?;

    // Resolve the models root and canonical files on the async runtime's
    // blocking pool so all filesystem syscalls stay off the executor.
    // Per-slug tolerance: a slug
    // whose resolver errors (e.g. `canonical_model_file` ambiguity from
    // two finalised GGUFs under one slug — see `paths::canonical_model_file`)
    // is skipped so a single broken slug does not make the whole
    // installed-model list unusable. Same shape as `resolve_default_model`
    // which already skips such slugs via `if let Ok(Some(_))`.
    let app_for_scan = app.clone();
    let canonical_files = tauri::async_runtime::spawn_blocking(move || {
        let models_root = paths::models_root(&app_for_scan)?;
        let slug_dirs = scan_installed_slug_dirs(&models_root)?;
        let mut resolved: Vec<(String, paths::CanonicalModelFile)> = Vec::new();
        for slug in slug_dirs {
            if let Ok(Some(cf)) = paths::canonical_model_file(&app_for_scan, &slug) {
                resolved.push((slug, cf));
            }
        }
        Ok::<_, HolziError>(resolved)
    })
    .await
    .map_err(|e| HolziError::CrdtInit {
        reason: format!("list_installed_models scan join: {e}"),
    })??;

    let payload = db
        .read(move |r| {
            let mut out = Vec::with_capacity(canonical_files.len());
            for (slug, cf) in canonical_files {
                let Some(row) = models_store::get_model(r, &slug)? else {
                    continue;
                };
                out.push(InstalledModelPayload::from_row(
                    row,
                    cf.relative_path,
                    cf.size_bytes as i64,
                ));
            }
            Ok(out)
        })
        .await?;
    Ok(payload)
}

/// The subdirectories of `models_root` that name an installed model slug, sorted for a stable
/// listing. A missing root is "nothing installed", not an error. Skips `.locks/` (spec 013 T071):
/// a dot-directory is never a model slug — `validate_slug` already rejects a leading dot for
/// anything created through us.
fn scan_installed_slug_dirs(models_root: &Path) -> Result<Vec<String>> {
    let mut slug_dirs: Vec<String> = Vec::new();
    match std::fs::read_dir(models_root) {
        Ok(entries) => {
            for entry in entries {
                let entry = entry.map_err(HolziError::from)?;
                if !entry.file_type().map_err(HolziError::from)?.is_dir() {
                    continue;
                }
                if let Some(name) = entry.file_name().to_str() {
                    if !name.starts_with('.') {
                        slug_dirs.push(name.to_string());
                    }
                }
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(HolziError::from(e)),
    }
    // Deterministic ordering — the frontend can rely on stable listings
    // between calls without sorting itself.
    slug_dirs.sort();
    Ok(slug_dirs)
}

/// Removes the on-disk GGUF file for a model slug. The `models` row
/// stays in place because it is synced catalog/source metadata; deleting
/// the file only removes it from this device's installed list (spec 002
/// §"Installed model", data-model.md "Löschen der lokalen Datei entfernt
/// nicht die synchronisierte Metadatenzeile"). Idempotent when the file
/// is already absent.
#[tauri::command]
pub async fn delete_installed_model(
    app: AppHandle,
    _state: State<'_, AppState>,
    id: String,
) -> Result<()> {
    let Some(canonical) = paths::canonical_model_file(&app, &id)? else {
        return Ok(());
    };
    tokio::fs::remove_file(&canonical.absolute_path)
        .await
        .map_err(|e| HolziError::Io {
            reason: format!("remove {}: {e}", canonical.absolute_path.display()),
        })?;
    Ok(())
}

struct RegisterDownloadedArgs {
    db: VaultDb,
    id: String,
    name: String,
    relative: String,
    /// Absolute path of the fully written staging file. It is hashed before
    /// publication and is never visible to the local-model resolver.
    staged_path: PathBuf,
    /// Canonical destination published only after hashing succeeds.
    destination: PathBuf,
    size_bytes: i64,
    context_window: Option<i64>,
    tokenizer_repo: Option<String>,
    source_kind: SourceKind,
    hf_repo: Option<String>,
    hf_filename: Option<String>,
    hf_revision: Option<String>,
    hf_revision_ref: Option<String>,
}

/// Shared post-download / post-import work: hash the finalized file, then
/// `models` upsert + ensure local provider, in one vault write. The on-disk file is authoritative for "installed here" (spec 002
/// §"Installed model"); the caller has already written the finalised
/// before invoking this helper — `download::download_to_file` and
/// `import::copy_into_managed` both fully write the staging file first. The
/// staged file is hashed, then published with a rollback backup, and only
/// then registered. A database failure restores the previous file so a
/// failed update cannot leave new bytes paired with the old SHA.
async fn register_downloaded(args: RegisterDownloadedArgs) -> Result<InstalledModelPayload> {
    let RegisterDownloadedArgs {
        db,
        id,
        name,
        relative,
        staged_path,
        destination,
        size_bytes,
        context_window,
        tokenizer_repo,
        source_kind,
        hf_repo,
        hf_filename,
        hf_revision,
        hf_revision_ref,
    } = args;

    let hash_path = staged_path.clone();
    let hash_result = tauri::async_runtime::spawn_blocking(move || -> Result<String> {
        // Hashing runs on this blocking-pool thread alongside the DB
        // write it gates — never on the async executor (models::hash
        // doc comment).
        let file_sha256 =
            hash::sha256_file(&hash_path).map_err(|e| HolziError::ModelRegistrationFailed {
                reason: format!("hash {}: {e}", hash_path.display()),
            })?;
        Ok(file_sha256)
    })
    .await
    .map_err(|e| HolziError::CrdtInit {
        reason: format!("register_downloaded hash join: {e}"),
    })?;

    let file_sha256 = match hash_result {
        Ok(hash) => hash,
        Err(error) => {
            let _ = tokio::fs::remove_file(&staged_path).await;
            return Err(error);
        }
    };

    let backup = match publish_staged_file(&staged_path, &destination).await {
        Ok(backup) => backup,
        Err(error) => {
            let _ = tokio::fs::remove_file(&staged_path).await;
            return Err(error);
        }
    };
    // A join failure comes back as an error here too, so it rolls the file back like any
    // other failed registration.
    let db_result = db
        .write(move |tx| {
            let provider_id = ensure_local_provider(tx)?;
            let m = ModelRow {
                id: id.clone(),
                provider_id,
                name: name.clone(),
                context_window,
                fetched_at: Some(now_ms()),
                tokenizer_repo,
                hf_repo: hf_repo.clone(),
                hf_filename: hf_filename.clone(),
                hf_revision: hf_revision.clone(),
                hf_revision_ref: hf_revision_ref.clone(),
                file_sha256: Some(file_sha256.clone()),
                integrity_status: IntegrityStatus::Verified,
                source_kind,
                // The one creation site every download/import path shares:
                // a local model has no provider to ask, so its record is
                // derived here (spec 012).
                capabilities: Some(ModelCapabilities::local(&id)),
            };
            models_store::upsert_model(tx, &m)?;
            Ok(InstalledModelPayload::from_row(m, relative, size_bytes))
        })
        .await;

    match db_result {
        Ok(payload) => {
            if let Some(backup) = backup {
                let _ = tokio::fs::remove_file(backup).await;
            }
            Ok(payload)
        }
        Err(error) => {
            rollback_published_file(&destination, backup).await?;
            Err(error)
        }
    }
}

fn staging_path(destination: &std::path::Path) -> PathBuf {
    let filename = destination
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("model.gguf");
    destination.with_file_name(format!(".{filename}.{}.staging", Uuid::new_v4()))
}

async fn publish_staged_file(staged: &PathBuf, destination: &PathBuf) -> Result<Option<PathBuf>> {
    let backup = if tokio::fs::metadata(destination).await.is_ok() {
        let filename = destination
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("model.gguf");
        let backup = destination.with_file_name(format!(".{filename}.{}.backup", Uuid::new_v4()));
        tokio::fs::rename(destination, &backup).await.map_err(|e| {
            HolziError::ModelRegistrationFailed {
                reason: format!("backup {}: {e}", destination.display()),
            }
        })?;
        Some(backup)
    } else {
        None
    };

    if let Err(error) = tokio::fs::rename(staged, destination).await {
        if let Some(backup_path) = backup.as_ref() {
            let _ = tokio::fs::rename(backup_path, destination).await;
        }
        return Err(HolziError::ModelRegistrationFailed {
            reason: format!("publish {}: {error}", destination.display()),
        });
    }
    Ok(backup)
}

async fn rollback_published_file(destination: &PathBuf, backup: Option<PathBuf>) -> Result<()> {
    let _ = tokio::fs::remove_file(destination).await;
    if let Some(backup) = backup {
        tokio::fs::rename(&backup, destination).await.map_err(|e| {
            HolziError::ModelRegistrationFailed {
                reason: format!("restore {}: {e}", destination.display()),
            }
        })?;
    }
    Ok(())
}

/// Extracts the `HolziError` serde tag (`"kind"`) as a machine-readable
/// error code for `HuggingFaceUpdateStatusPayload.errorCode`, reusing the
/// existing `#[serde(tag = "kind")]` encoding instead of a parallel match.
fn error_kind_str(e: &HolziError) -> String {
    serde_json::to_value(e)
        .ok()
        .and_then(|v| v.get("kind").and_then(|k| k.as_str()).map(str::to_string))
        .unwrap_or_else(|| "Unknown".to_string())
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Formats the current time as `YYYY-MM-DDTHH:MM:SS.mmmZ` (UTC) without
/// pulling in a date/time crate — `checkedAt` is display-only and this
/// crate has no other ISO-8601 need yet.
fn iso8601_now() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    format_iso8601_utc(now.as_secs(), now.subsec_millis())
}

fn format_iso8601_utc(epoch_secs: u64, millis: u32) -> String {
    let days = (epoch_secs / 86400) as i64;
    let secs_of_day = epoch_secs % 86400;
    let (y, m, d) = civil_from_days(days);
    let hh = secs_of_day / 3600;
    let mm = (secs_of_day % 3600) / 60;
    let ss = secs_of_day % 60;
    format!("{y:04}-{m:02}-{d:02}T{hh:02}:{mm:02}:{ss:02}.{millis:03}Z")
}

/// Howard Hinnant's `civil_from_days`: days since the Unix epoch ->
/// proleptic Gregorian `(year, month, day)`. Chosen over pulling in a
/// date/time crate for one display-only formatter.
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}

#[cfg(test)]
#[path = "commands_tests.rs"]
mod tests;
