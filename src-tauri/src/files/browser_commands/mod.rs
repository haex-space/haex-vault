//! Tauri commands of the file browser's window (spec 044, contracts/tauri-commands.md). Every
//! command acts as [`Caller::User`]; paths are resolved and checked in Rust before anything is
//! read. The commands are split by what they do: [`navigate`] (sources, folders, watching),
//! [`view`] (viewer, thumbnails, system apps), [`change`] (create, rename, transfers) and
//! [`search`]; this module keeps what they share.

pub mod change;
pub mod encrypted;
pub mod navigate;
pub mod peers;
pub mod search;
pub mod view;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use tauri::{AppHandle, Manager};

use crate::error::HolziError;
use crate::files::access::{check, AgentGrants, Target, Verdict, Want};
use crate::files::encrypted::device::{has_raw_part, route_device, DeviceRoot};
use crate::files::encrypted::{Background, Scope, VaultSource};
use crate::files::local::{resolve, resolve_entry};
use crate::files::peer::connect::Connections;
use crate::files::peer::store::{peer_access, PeerStore};
use crate::files::peer::wire::Caller as PeerCaller;
use crate::files::share::PEER_PREFIX;
use crate::files::state::FilesState;
use crate::files::storage_source::StorageFiles;
use crate::files::transfer::remote::Side;
use crate::files::{Entry, FilesError, FilesErrorCode, SourceRef};
use crate::passwords::access::Caller;
use crate::state::AppState;
use crate::sync::registry::SyncRegistry;

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

/// [`storage_files`] for the window and for agents: with the [`Scope`] that lets its paths go
/// through encrypted folders by their decrypted names (spec 048); an agent's executor decides which
/// folders it may enter. The opened folders live in `files` until the vault closes.
pub(crate) async fn window_storage<R: tauri::Runtime>(
    app: &AppHandle<R>,
    state: &AppState,
    files: &FilesState,
    storage_id: &str,
) -> Result<StorageFiles, FilesError> {
    if storage_id.starts_with(PEER_PREFIX) {
        return peer_storage(app, storage_id, PeerCaller::User);
    }
    let scope = window_scope(app, state, files, storage_id);
    Ok(storage_files(state, storage_id).await?.with_scope(scope))
}

/// Another own device as a storage (spec 056 US1): its files over `haex-vault-files/1`, asked as
/// `caller`. The serving device opens its encrypted folders itself, so no scope goes with it; an
/// unknown device or this one answers `NotFound` on the first call.
pub(crate) fn peer_storage<R: tauri::Runtime>(
    app: &AppHandle<R>,
    storage_id: &str,
    caller: PeerCaller,
) -> Result<StorageFiles, FilesError> {
    Ok(StorageFiles::new(
        Arc::new(PeerStore::new(peer_links(app)?, caller)),
        peer_access(storage_id, None),
    ))
}

/// The session's links to other devices; none while sync does not run.
pub(crate) fn peer_links<R: tauri::Runtime>(
    app: &AppHandle<R>,
) -> Result<Arc<Connections>, FilesError> {
    app.try_state::<Arc<SyncRegistry>>()
        .and_then(|registry| registry.get())
        .and_then(|runtime| runtime.files.clone())
        .ok_or_else(|| {
            FilesError::new(
                FilesErrorCode::Unreachable,
                "other devices are not reachable now",
            )
        })
}

/// The device as a storage for its encrypted folders (spec 048 US7, research R16), with the same
/// scope as [`window_storage`]; it needs an open vault for the folder keys and the cache.
pub(crate) fn device_storage<R: tauri::Runtime>(
    app: &AppHandle<R>,
    state: &AppState,
    files: &FilesState,
    root: &DeviceRoot,
) -> Result<StorageFiles, FilesError> {
    state
        .database()
        .map_err(|_| FilesError::new(FilesErrorCode::Unsupported, "no open vault"))?;
    let scope = window_scope(app, state, files, &root.storage_id);
    Ok(root.files(crate::remote_storage::commands::stores(), scope))
}

fn window_scope<R: tauri::Runtime>(
    app: &AppHandle<R>,
    state: &AppState,
    files: &FilesState,
    storage_id: &str,
) -> Scope {
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
    Scope {
        storage_id: storage_id.to_owned(),
        folders: Arc::clone(&files.encrypted),
        vault,
        background,
    }
}

/// The storage id of `source`, if it is one.
pub(super) fn storage_of(source: &SourceRef) -> Option<&str> {
    match source {
        SourceRef::Storage { storage_id } => Some(storage_id),
        SourceRef::Device => None,
    }
}

/// Where a window path of a source leads (spec 048 research R17). It lives for one command, so the
/// storage is not boxed.
#[allow(clippy::large_enum_variant)]
pub(super) enum Place {
    /// A storage, or an encrypted folder on this device (`device`) seen as the device's storage;
    /// `path` is the window path in that storage.
    Storage {
        files: StorageFiles,
        path: String,
        device: Option<DeviceRoot>,
    },
    /// An ordinary path of this device.
    Device,
}

impl Place {
    /// `entry` of the storage as the window's source names it.
    pub(super) fn entry(device: Option<&DeviceRoot>, entry: Entry) -> Entry {
        match device {
            Some(root) => root.entry(entry),
            None => entry,
        }
    }
}

/// Where `path` of `source` leads. A device path into an encrypted folder is checked like the
/// device folder that holds the folder: no change in holzi's own places (FR-037).
pub(super) async fn place_of(
    app: &AppHandle,
    state: &AppState,
    files: &FilesState,
    source: &SourceRef,
    path: &str,
    want: Want,
) -> Result<Place, FilesError> {
    if let Some(id) = storage_of(source) {
        return Ok(Place::Storage {
            files: window_storage(app, state, files, id).await?,
            path: path.to_owned(),
            device: None,
        });
    }
    let device = Path::new(path);
    let Some(root) = DeviceRoot::of(device) else {
        return Ok(Place::Device);
    };
    // An ordinary path that exists needs neither the vault nor a listing.
    if !has_raw_part(device) && device.symlink_metadata().is_ok() {
        return Ok(Place::Device);
    }
    let Ok(storage) = device_storage(app, state, files, &root) else {
        return Ok(Place::Device);
    };
    let Some(view) = route_device(&storage, &root, device).await? else {
        return Ok(Place::Device);
    };
    let holder = device
        .ancestors()
        .find(|dir| dir.symlink_metadata().is_ok())
        .unwrap_or(&root.dir);
    checked(files, source, &holder.to_string_lossy(), want, resolve)?;
    Ok(Place::Storage {
        files: storage,
        path: view,
        device: Some(root),
    })
}

/// One end of a transfer: the storage the paths are on (also the device's storage when one of
/// them is in an encrypted folder on this device), or the device, with the paths on it.
pub(super) struct Ends {
    pub storage: Option<StorageFiles>,
    pub paths: Vec<String>,
}

impl Ends {
    /// The side of a transfer.
    pub fn side(self) -> (Side, Vec<String>) {
        match self.storage {
            Some(storage) => (Side::Storage(Arc::new(storage)), self.paths),
            None => (Side::Device, self.paths),
        }
    }
}

/// The end of `paths` of `source` for `want`. When one of them leads into an encrypted folder on
/// this device, all go through the device's storage; the ordinary ones are checked as before.
pub(super) async fn ends_of(
    app: &AppHandle,
    state: &AppState,
    files: &FilesState,
    source: &SourceRef,
    paths: &[String],
    want: Want,
) -> Result<Ends, FilesError> {
    if let Some(id) = storage_of(source) {
        return Ok(Ends {
            storage: Some(window_storage(app, state, files, id).await?),
            paths: paths.to_vec(),
        });
    }
    let mut device = None;
    for path in paths {
        if let Place::Storage {
            files: storage,
            device: Some(root),
            ..
        } = place_of(app, state, files, source, path, want).await?
        {
            device = Some((storage, root));
            break;
        }
    }
    let Some((storage, root)) = device else {
        let paths = paths
            .iter()
            .map(|path| {
                device_entry(files, source, path, want)
                    .map(|real| real.to_string_lossy().into_owned())
            })
            .collect::<Result<_, _>>()?;
        return Ok(Ends {
            storage: None,
            paths,
        });
    };
    let mut views = Vec::with_capacity(paths.len());
    for path in paths {
        if Path::new(path).symlink_metadata().is_ok() {
            device_entry(files, source, path, want)?;
        } else {
            place_of(app, state, files, source, path, want).await?;
        }
        views.push(root.view_of(Path::new(path)));
    }
    Ok(Ends {
        storage: Some(storage),
        paths: views,
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
    // The raw directory of an encrypted folder is no path of the window (spec 048 research R17).
    if has_raw_part(Path::new(path)) {
        return Err(FilesError::new(FilesErrorCode::NotFound, "not found"));
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
