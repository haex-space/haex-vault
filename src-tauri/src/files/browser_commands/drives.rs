//! The drives of the file browser (spec 064, contracts/commands.md): the live list, opening a
//! drive, which mounts it first when needed, and releasing it. None of this is an agent action or a peer request
//! (FR-021, FR-022). The backend blocks, so every call runs off the async executor.

use std::path::Path;

use serde::Serialize;
use tauri::ipc::Channel;
use tauri::{AppHandle, Manager, State};
use ts_rs::TS;

use super::blocking;
use crate::files::local::drives::backend::{backend, DriveBackend, DriveError, UnlockMode};
use crate::files::local::drives::monitor::{DriveMonitor, DrivesChanged};
use crate::files::local::drives::{DriveRelease, DriveState};
use crate::files::media::server::MediaServer;
use crate::files::search::SearchManager;
use crate::files::state::FilesState;
use crate::files::transfer::TransferManager;
use crate::files::FilesError;
use crate::state::AppState;

/// A password manager entry offered for a drive; never its password.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct DriveEntry {
    pub item_id: String,
    pub title: String,
    pub username: Option<String>,
    pub url: Option<String>,
}

/// Why the unlock dialog opens.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub enum DriveNeedsPassword {
    /// No saved password fits.
    None,
    /// More than one saved password fits; the dialog offers them.
    Several,
    /// The one saved password did not unlock the drive.
    SavedFailed,
}

/// Why a drive was opened but its password was not remembered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub enum DriveRememberFailed {
    /// The picked entry already has another address; the window asks before replacing it.
    UrlTaken,
    Failed,
}

/// What opening a drive led to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
#[ts(export, export_to = "../../src/types/bindings/")]
pub enum DriveOpened {
    /// Mounted, at `path`.
    Opened {
        path: String,
        remember_failed: Option<DriveRememberFailed>,
    },
    /// Locked, and the window has to ask for a password.
    NeedsPassword {
        reason: DriveNeedsPassword,
        message: Option<String>,
        candidates: Vec<DriveEntry>,
    },
    /// The system's own unlock dialog opened (FR-019).
    SystemDialog,
}

/// What releasing a drive led to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub enum DriveReleased {
    /// Unmounted; `ejected` when it was also switched off or ejected and may be pulled.
    Released { ejected: bool },
    /// Transfers on the drive still run; nothing happened (only without `cancelTransfers`).
    TransfersRunning { count: u32 },
}

/// Starts sending the drive list to `channel`, at once and after every change (FR-004).
#[tauri::command]
pub async fn files_drives_watch(
    monitor: State<'_, DriveMonitor>,
    channel: Channel<DrivesChanged>,
) -> Result<u64, FilesError> {
    let monitor = monitor.inner().clone();
    blocking(move || {
        Ok(monitor.subscribe(Box::new(move |message| {
            channel.send(message.clone()).is_ok()
        })))
    })
    .await
}

#[tauri::command]
pub fn files_drives_unwatch(monitor: State<'_, DriveMonitor>, id: u64) {
    monitor.unsubscribe(id);
}

/// Opens a drive from the sidebar or a tab's notice: mounts it when needed (FR-006).
#[tauri::command]
pub async fn files_drive_open(id: String) -> Result<DriveOpened, FilesError> {
    blocking(move || open(&*backend(), &id)).await
}

/// [`files_drive_open`] against any backend.
pub(crate) fn open(backend: &dyn DriveBackend, id: &str) -> Result<DriveOpened, FilesError> {
    let drive = backend
        .list()?
        .into_iter()
        .find(|drive| drive.id == id)
        .ok_or(DriveError::Gone)?;
    let path = match drive.state {
        DriveState::Mounted => drive.path.ok_or(DriveError::Gone)?,
        DriveState::Unmounted => backend.mount(id)?,
        DriveState::Locked => {
            return match backend.unlock_mode() {
                UnlockMode::SystemDialog => {
                    backend.open_system_unlock(id)?;
                    Ok(DriveOpened::SystemDialog)
                }
                // Unlocking with a password comes with spec 064 US3.
                UnlockMode::Passphrase | UnlockMode::None => Err(DriveError::Unsupported.into()),
            };
        }
    };
    Ok(DriveOpened::Opened {
        path,
        remember_failed: None,
    })
}

/// Unmounts a drive and, as the sidebar's button says, ejects it (FR-008 to FR-012). Running
/// transfers on it are only cancelled with `cancel_transfers`.
#[tauri::command]
pub async fn files_drive_release(
    app: AppHandle,
    id: String,
    cancel_transfers: bool,
) -> Result<DriveReleased, FilesError> {
    blocking(move || release(&*backend(), &AppHolders(app), &id, cancel_transfers)).await
}

/// What holds files of this device for the window and its extensions, released before a drive
/// goes (research R9).
pub(crate) trait Holders {
    /// How many running transfers read or write at or below `prefix`.
    fn transfers(&self, prefix: &Path) -> usize;
    /// Cancels those transfers and waits until they ended.
    fn cancel_transfers(&self, prefix: &Path);
    /// Ends the watches, media tokens, searches and extension watches at or below `prefix`.
    fn release_under(&self, prefix: &Path);
}

struct AppHolders(AppHandle);

impl Holders for AppHolders {
    fn transfers(&self, prefix: &Path) -> usize {
        self.0.state::<TransferManager>().touching(prefix).len()
    }

    fn cancel_transfers(&self, prefix: &Path) {
        let transfers = self.0.state::<TransferManager>().inner().clone();
        tokio::runtime::Handle::current().block_on(transfers.cancel_touching(prefix));
    }

    fn release_under(&self, prefix: &Path) {
        self.0.state::<FilesState>().unwatch_under(prefix);
        self.0.state::<MediaServer>().release_under(prefix);
        self.0.state::<SearchManager>().cancel_under(prefix);
        self.0
            .state::<AppState>()
            .extensions()
            .fs
            .watches
            .end_under(prefix);
    }
}

/// [`files_drive_release`] against any backend and holders, in the order of
/// contracts/commands.md.
pub(crate) fn release(
    backend: &dyn DriveBackend,
    holders: &dyn Holders,
    id: &str,
    cancel_transfers: bool,
) -> Result<DriveReleased, FilesError> {
    let drive = backend
        .list()?
        .into_iter()
        .find(|drive| drive.id == id)
        .ok_or(DriveError::Gone)?;
    if drive.release == DriveRelease::None {
        return Err(DriveError::Unsupported.into());
    }
    let path = drive.path.ok_or(DriveError::Gone)?;
    let prefix = Path::new(&path);
    let count = holders.transfers(prefix);
    if count > 0 && !cancel_transfers {
        return Ok(DriveReleased::TransfersRunning {
            count: u32::try_from(count).unwrap_or(u32::MAX),
        });
    }
    holders.cancel_transfers(prefix);
    holders.release_under(prefix);
    let ejected = backend.release(id)?;
    Ok(DriveReleased::Released { ejected })
}

#[cfg(test)]
#[path = "drives_tests.rs"]
mod tests;
