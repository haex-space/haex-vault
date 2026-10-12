//! Tauri commands of the file browser's window (spec 044, contracts/tauri-commands.md). Every
//! command acts as [`Caller::User`]; paths are resolved and checked in Rust before anything is
//! read. The commands are split by what they do: [`navigate`] (sources, folders, watching),
//! [`view`] (viewer, thumbnails, system apps), [`change`] (create, rename, transfers) and
//! [`search`]; this module keeps what they share.

pub mod change;
pub mod encrypted;
pub mod navigate;
pub mod search;
pub mod view;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use tauri::{AppHandle, Manager, State};

use crate::error::HolziError;
use crate::files::access::{check, AgentGrants, Target, Verdict, Want};
use crate::files::encrypted::{Background, Scope, VaultSource};
use crate::files::local::{resolve, resolve_entry};
use crate::files::state::FilesState;
use crate::files::storage_source::StorageFiles;
use crate::files::transfer::remote::Side;
use crate::files::{FilesError, FilesErrorCode, SourceRef};
use crate::passwords::access::Caller;
use crate::state::AppState;

pub use navigate::{KnownPlace, StorageSource};

/// The files of the storage `storage_id` (spec 044 US5), or why not. Its credentials stay inside
/// `remote_storage`; no answer carries them (FR-038).
pub(crate) async fn storage_files(
    state: &AppState,
    storage_id: &str,
) -> Result<StorageFiles, FilesError> {
    let service = crate::remote_storage::commands::service(state)
        .map_err(|_| FilesError::new(FilesErrorCode::Unsupported, "no open vault"))?;
    let access = service
        .access_of(storage_id)
        .await
        .map_err(|error| match error {
            HolziError::StorageNotFound => {
                FilesError::new(FilesErrorCode::NotFound, "no such storage")
            }
            HolziError::StorageCredentialsUnavailable { .. } => FilesError::new(
                FilesErrorCode::Credentials,
                "the storage's credentials are not on this device",
            ),
            other => {
                log::warn!("files: a storage could not be opened: {other}");
                FilesError::new(FilesErrorCode::Unsupported, "the storage cannot be opened")
            }
        })?;
    Ok(StorageFiles::new(
        crate::remote_storage::commands::stores(),
        access,
    ))
}

/// [`storage_files`] for the window: with the [`Scope`] that lets its paths go through encrypted
/// folders by their decrypted names (spec 048). The opened folders live in `files` until the vault
/// closes.
pub(crate) async fn window_storage(
    app: &AppHandle,
    state: &AppState,
    files: &FilesState,
    storage_id: &str,
) -> Result<StorageFiles, FilesError> {
    files.forget_on_close(state.gate());
    let app = app.clone();
    let vault: VaultSource = Arc::new(move || {
        app.state::<AppState>()
            .database()
            .map_err(|_| FilesError::new(FilesErrorCode::Unsupported, "no open vault"))
    });
    let gate = state.gate().clone();
    let background: Background = Arc::new(move |work| {
        let token = gate.token();
        let _ = gate.spawn(async move {
            tokio::select! {
                () = token.cancelled() => {}
                () = work => {}
            }
        });
    });
    Ok(storage_files(state, storage_id).await?.with_scope(Scope {
        storage_id: storage_id.to_owned(),
        folders: Arc::clone(&files.encrypted),
        vault,
        background,
    }))
}

/// The storage id of `source`, if it is one.
pub(super) fn storage_of(source: &SourceRef) -> Option<&str> {
    match source {
        SourceRef::Storage { storage_id } => Some(storage_id),
        SourceRef::Device => None,
    }
}

/// One end of a transfer across sources.
pub(super) async fn side_of(
    app: &AppHandle,
    state: &State<'_, AppState>,
    files: &FilesState,
    source: &SourceRef,
) -> Result<Side, FilesError> {
    Ok(match storage_of(source) {
        Some(id) => Side::Storage(Arc::new(window_storage(app, state, files, id).await?)),
        None => Side::Device,
    })
}

pub(super) async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> Result<T, FilesError> + Send + 'static,
) -> Result<T, FilesError> {
    tauri::async_runtime::spawn_blocking(work)
        .await
        .unwrap_or_else(|error| {
            log::error!("files: a blocking task failed: {error}");
            Err(FilesError::new(
                FilesErrorCode::NotFound,
                "the operation failed",
            ))
        })
}

/// The resolved device path the user may `want`, or why not.
pub(super) fn device_path(
    files: &FilesState,
    source: &SourceRef,
    path: &str,
    want: Want,
) -> Result<PathBuf, FilesError> {
    checked(files, source, path, want, resolve)
}

/// Like [`device_path`] for an entry itself: a link stays the link ([`resolve_entry`]).
pub(super) fn device_entry(
    files: &FilesState,
    source: &SourceRef,
    path: &str,
    want: Want,
) -> Result<PathBuf, FilesError> {
    checked(files, source, path, want, resolve_entry)
}

fn checked(
    files: &FilesState,
    source: &SourceRef,
    path: &str,
    want: Want,
    resolver: fn(&Path) -> Result<PathBuf, FilesError>,
) -> Result<PathBuf, FilesError> {
    if !matches!(source, SourceRef::Device) {
        return Err(FilesError::new(
            FilesErrorCode::Unsupported,
            "storages come with a later version",
        ));
    }
    let real = resolver(Path::new(path))?;
    match check(
        &Caller::User,
        &Target::Device(real.clone()),
        want,
        &files.own,
        &AgentGrants::default(),
    ) {
        Verdict::Allowed | Verdict::ReadOnly => Ok(real),
        Verdict::Refused(error) => Err(error),
        Verdict::Ask => Err(FilesError::new(FilesErrorCode::NotGranted, "not granted")),
    }
}
