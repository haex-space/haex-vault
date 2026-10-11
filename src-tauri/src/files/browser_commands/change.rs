//! Creating, renaming and transfers (spec 044 US3, US5).

use tauri::ipc::Channel;
use tauri::{AppHandle, State};
use ts_rs::TS;

use super::{blocking, device_entry, device_path, side_of, storage_of, window_storage};
use crate::files::access::Want;
use crate::files::local::edit;
use crate::files::state::FilesState;
use crate::files::transfer::local::{prepare, touches_own, Job};
use crate::files::transfer::remote::{prepare_remote, RemoteJob};
use crate::files::transfer::{
    platform_removal, same_device, ConflictChoice, TransferEvent, TransferManager, TransferOp,
};
use crate::files::{Entry, FilesError, FilesErrorCode, SourceRef};
use crate::state::AppState;

/// Creates a folder (FR-017).
#[tauri::command]
pub async fn files_create_folder(
    app: AppHandle,
    state: State<'_, AppState>,
    files: State<'_, FilesState>,
    source: SourceRef,
    path: String,
    name: String,
) -> Result<Entry, FilesError> {
    if let Some(id) = storage_of(&source) {
        return window_storage(&app, &state, &files, id)
            .await?
            .create_folder(&path, &name)
            .await;
    }
    let parent = device_path(&files, &source, &path, Want::Write)?;
    let own = files.own.clone();
    blocking(move || edit::create_folder(&parent, &name, &own)).await
}

/// Renames an entry in its folder (FR-017).
#[tauri::command]
pub async fn files_rename(
    app: AppHandle,
    state: State<'_, AppState>,
    files: State<'_, FilesState>,
    source: SourceRef,
    path: String,
    new_name: String,
) -> Result<Entry, FilesError> {
    if let Some(id) = storage_of(&source) {
        return window_storage(&app, &state, &files, id)
            .await?
            .rename(&path, &new_name)
            .await;
    }
    let entry = device_entry(&files, &source, &path, Want::Write)?;
    let own = files.own.clone();
    blocking(move || edit::rename(&entry, &new_name, &own)).await
}

/// Where a copy or move goes.
#[derive(Debug, Clone, serde::Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct TransferTarget {
    pub source: SourceRef,
    pub path: String,
}

/// Copies or moves between sources (US5): the device paths are checked as for a local transfer
/// (FR-037), then the job is listed and checked (into itself, space) before it starts.
#[allow(clippy::too_many_arguments)]
async fn start_remote(
    app: &AppHandle,
    state: &State<'_, AppState>,
    files: &FilesState,
    transfers: &TransferManager,
    op: TransferOp,
    from: &SourceRef,
    paths: &[String],
    to: TransferTarget,
    channel: Channel<TransferEvent>,
) -> Result<String, FilesError> {
    let want = if op == TransferOp::Copy {
        Want::Read
    } else {
        Want::Write
    };
    let sources = match storage_of(from) {
        Some(_) => paths.to_vec(),
        None => paths
            .iter()
            .map(|path| {
                device_entry(files, from, path, want)
                    .map(|real| real.to_string_lossy().into_owned())
            })
            .collect::<Result<_, _>>()?,
    };
    let target = match storage_of(&to.source) {
        Some(_) => to.path.clone(),
        None => device_path(files, &to.source, &to.path, Want::Write)?
            .to_string_lossy()
            .into_owned(),
    };
    let job = RemoteJob {
        op,
        from: side_of(app, state, files, from).await?,
        sources,
        to: side_of(app, state, files, &to.source).await?,
        target,
    };
    let (job, plan) = blocking(move || {
        let handle = tokio::runtime::Handle::current();
        let plan = prepare_remote(&job, &handle, |path| {
            crate::files::local::drives::space(path).map(|(_, free)| free)
        })?;
        Ok((job, plan))
    })
    .await?;
    transfers.start_remote(state.gate(), job, plan, channel)
}

/// Checks a transfer of `paths` and starts it (FR-018 to FR-023); refusals before the start
/// (`intoItself`, `noSpace`, `holziOwned`, …) come back as the error.
#[allow(clippy::too_many_arguments)]
async fn start_transfer(
    state: &AppState,
    files: &FilesState,
    transfers: &TransferManager,
    op: TransferOp,
    from: &SourceRef,
    paths: &[String],
    to: Option<TransferTarget>,
    channel: Channel<TransferEvent>,
) -> Result<String, FilesError> {
    // Copying reads the sources; moving and deleting change their folders.
    let want = if op == TransferOp::Copy {
        Want::Read
    } else {
        Want::Write
    };
    let sources = paths
        .iter()
        .map(|path| device_entry(files, from, path, want))
        .collect::<Result<Vec<_>, _>>()?;
    let target = match (op, to) {
        (TransferOp::Delete, _) => None,
        (_, Some(to)) => Some(device_path(files, &to.source, &to.path, Want::Write)?),
        (_, None) => return Err(FilesError::invalid_path("no target folder")),
    };
    let job = Job {
        op,
        sources,
        target,
    };
    if touches_own(&job, &files.own) {
        return Err(FilesError::new(
            FilesErrorCode::HolziOwned,
            "holzi's own data is read-only",
        ));
    }
    let (job, totals) = blocking(move || {
        let totals = prepare(
            &job,
            |path| crate::files::local::drives::space(path).map(|(_, free)| free),
            same_device,
        )?;
        Ok((job, totals))
    })
    .await?;
    transfers.start(state.gate(), job, totals, platform_removal(), channel)
}

/// Copies, moves or deletes a selection; the progress comes through `channel`.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn files_transfer_start(
    app: AppHandle,
    state: State<'_, AppState>,
    files: State<'_, FilesState>,
    transfers: State<'_, TransferManager>,
    op: TransferOp,
    from: SourceRef,
    paths: Vec<String>,
    to: Option<TransferTarget>,
    channel: Channel<TransferEvent>,
) -> Result<String, FilesError> {
    if let (Some(id), TransferOp::Delete) = (storage_of(&from), op) {
        // A storage has no trash: the window asked before (FR-023).
        let storage = window_storage(&app, &state, &files, id).await?;
        return transfers.start_storage_delete(state.gate(), storage, paths, channel);
    }
    let crosses = storage_of(&from).is_some()
        || to
            .as_ref()
            .is_some_and(|target| storage_of(&target.source).is_some());
    if crosses {
        let to = to.ok_or_else(|| FilesError::invalid_path("no target folder"))?;
        return start_remote(
            &app, &state, &files, &transfers, op, &from, &paths, to, channel,
        )
        .await;
    }
    start_transfer(&state, &files, &transfers, op, &from, &paths, to, channel).await
}

/// Copies files and folders dropped from the system into a folder (FR-024).
#[tauri::command]
pub async fn files_import_dropped(
    app: AppHandle,
    state: State<'_, AppState>,
    files: State<'_, FilesState>,
    transfers: State<'_, TransferManager>,
    paths: Vec<String>,
    to: TransferTarget,
    channel: Channel<TransferEvent>,
) -> Result<String, FilesError> {
    if storage_of(&to.source).is_some() {
        return start_remote(
            &app,
            &state,
            &files,
            &transfers,
            TransferOp::Copy,
            &SourceRef::Device,
            &paths,
            to,
            channel,
        )
        .await;
    }
    start_transfer(
        &state,
        &files,
        &transfers,
        TransferOp::Copy,
        &SourceRef::Device,
        &paths,
        Some(to),
        channel,
    )
    .await
}

/// Answers the name conflict a transfer waits on.
#[tauri::command]
pub fn files_transfer_answer(
    transfers: State<'_, TransferManager>,
    transfer_id: String,
    choice: ConflictChoice,
    for_all: bool,
) {
    transfers.answer(&transfer_id, choice, for_all);
}

/// Cancels a transfer; nothing half-written stays behind (FR-020).
#[tauri::command]
pub fn files_transfer_cancel(transfers: State<'_, TransferManager>, transfer_id: String) {
    transfers.cancel(&transfer_id);
}

/// Starts a failed transfer again.
#[tauri::command]
pub fn files_transfer_retry(
    state: State<'_, AppState>,
    transfers: State<'_, TransferManager>,
    transfer_id: String,
) -> Result<(), FilesError> {
    transfers.retry(state.gate(), &transfer_id)
}
