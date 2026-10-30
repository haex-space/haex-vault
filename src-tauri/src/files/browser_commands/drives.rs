//! The drives of the file browser (spec 064, contracts/commands.md): the live list, opening a
//! drive, which mounts or unlocks it first when needed, and releasing it. None of this is an agent
//! action or a peer request (FR-021, FR-022). The backend blocks, so every call runs off the async
//! executor; the password manager is asked from there too.

use std::future::Future;
use std::path::Path;

use serde::{Deserialize, Serialize};
use tauri::ipc::Channel;
use tauri::{AppHandle, Manager, State};
use ts_rs::TS;

use super::blocking;
use crate::files::local::drives::backend::{
    backend, DriveBackend, DriveError, DriveSecret, UnlockMode,
};
use crate::files::local::drives::monitor::{DriveMonitor, DrivesChanged};
use crate::files::local::drives::unlock::{self, DriveEntry};
use crate::files::local::drives::{Drive, DriveRelease, DriveState};
use crate::files::media::server::MediaServer;
use crate::files::search::SearchManager;
use crate::files::state::FilesState;
use crate::files::transfer::TransferManager;
use crate::files::FilesError;
use crate::passwords::service::PasswordsService;
use crate::state::AppState;

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

/// The password the unlock dialog sends: typed, or the one of an entry the user picked. It has no
/// `Debug`, so a typed password cannot end up in a log line (FR-017).
#[derive(Deserialize, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
#[ts(export, export_to = "../../src/types/bindings/")]
pub enum DriveSecretInput {
    Typed { password: String },
    Entry { item_id: String },
}

/// Whether the dialog's "remember" box was ticked: a typed password becomes a new entry, a picked
/// entry gets the drive's address.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub enum DriveRemember {
    No,
    Yes,
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

/// Opens a drive from the sidebar or a tab's notice: mounts it when needed (FR-006), and unlocks it
/// with the one saved password that fits (FR-013).
#[tauri::command]
pub async fn files_drive_open(app: AppHandle, id: String) -> Result<DriveOpened, FilesError> {
    blocking(move || open(&*backend(), passwords(&app).as_ref(), &id)).await
}

/// Unlocks a drive with the password from the dialog and remembers it when asked, but only once the
/// drive is open (FR-014 to FR-016). `entry_title` names a new entry in the window's language.
#[tauri::command]
pub async fn files_drive_unlock(
    app: AppHandle,
    id: String,
    secret: DriveSecretInput,
    remember: DriveRemember,
    entry_title: String,
) -> Result<DriveOpened, FilesError> {
    blocking(move || {
        unlock_drive(
            &*backend(),
            passwords(&app).as_ref(),
            &id,
            secret,
            remember,
            &entry_title,
        )
    })
    .await
}

/// Gives the entry `item_id` the address of the drive with `key` after the user agreed to replace
/// its old one (`rememberFailed: urlTaken`). The drive is named by its key, as unlocking it gave it
/// a new id.
#[tauri::command]
pub async fn files_drive_link_entry(
    app: AppHandle,
    key: String,
    item_id: String,
) -> Result<(), FilesError> {
    blocking(move || link(&*backend(), passwords(&app).as_ref(), &key, &item_id)).await
}

/// The entries the unlock dialog offers, without their passwords.
#[tauri::command]
pub async fn files_drive_unlock_candidates(
    app: AppHandle,
    query: String,
) -> Result<Vec<DriveEntry>, FilesError> {
    let passwords = passwords(&app).ok_or(DriveError::NoVault)?;
    Ok(unlock::candidates(&passwords, &query).await?)
}

/// The password manager of the open vault, if one is open (FR-018).
fn passwords(app: &AppHandle) -> Option<PasswordsService> {
    let state = app.state::<AppState>();
    let db = state.database().ok()?;
    Some(PasswordsService::with_usage(db, state.usage()))
}

/// Waits for a password manager call on the blocking thread a drive command runs on.
fn wait<T>(work: impl Future<Output = T>) -> T {
    tokio::runtime::Handle::current().block_on(work)
}

fn listed(backend: &dyn DriveBackend, id: &str) -> Result<Drive, DriveError> {
    backend
        .list()?
        .into_iter()
        .find(|drive| drive.id == id)
        .ok_or(DriveError::Gone)
}

fn opened(path: String) -> DriveOpened {
    DriveOpened::Opened {
        path,
        remember_failed: None,
    }
}

/// [`files_drive_open`] against any backend and password manager.
pub(crate) fn open(
    backend: &dyn DriveBackend,
    passwords: Option<&PasswordsService>,
    id: &str,
) -> Result<DriveOpened, FilesError> {
    let drive = listed(backend, id)?;
    let path = match drive.state {
        DriveState::Mounted => drive.path.ok_or(DriveError::Gone)?,
        DriveState::Unmounted => backend.mount(id)?,
        DriveState::Locked => return open_locked(backend, passwords, &drive),
    };
    Ok(opened(path))
}

/// Steps 3 and 4 of contracts/commands.md: the system's dialog, the one saved password, or the
/// window's dialog.
fn open_locked(
    backend: &dyn DriveBackend,
    passwords: Option<&PasswordsService>,
    drive: &Drive,
) -> Result<DriveOpened, FilesError> {
    match backend.unlock_mode() {
        UnlockMode::SystemDialog => {
            backend.open_system_unlock(&drive.id)?;
            return Ok(DriveOpened::SystemDialog);
        }
        UnlockMode::None => return Err(DriveError::Unsupported.into()),
        UnlockMode::Passphrase => {}
    }
    let ask = |reason, message, candidates| DriveOpened::NeedsPassword {
        reason,
        message,
        candidates,
    };
    let (Some(passwords), Some(key)) = (passwords, drive.key.as_deref()) else {
        return Ok(ask(DriveNeedsPassword::None, None, Vec::new()));
    };
    let mut matches = match wait(unlock::matching_entries(passwords, key)) {
        Err(DriveError::NoVault) => Vec::new(),
        other => other?,
    };
    match matches.len() {
        0 => Ok(ask(DriveNeedsPassword::None, None, Vec::new())),
        1 => {
            let entry = matches.remove(0);
            let secret = wait(unlock::read_password(passwords, &entry.item_id))?;
            match backend.unlock(&drive.id, &secret) {
                Ok(path) => Ok(opened(path)),
                Err(DriveError::UnlockFailed(message)) => Ok(ask(
                    DriveNeedsPassword::SavedFailed,
                    Some(message),
                    Vec::new(),
                )),
                Err(error) => Err(error.into()),
            }
        }
        _ => Ok(ask(DriveNeedsPassword::Several, None, matches)),
    }
}

/// [`files_drive_unlock`] against any backend and password manager.
pub(crate) fn unlock_drive(
    backend: &dyn DriveBackend,
    passwords: Option<&PasswordsService>,
    id: &str,
    secret: DriveSecretInput,
    remember: DriveRemember,
    entry_title: &str,
) -> Result<DriveOpened, FilesError> {
    let drive = listed(backend, id)?;
    let (secret, picked) = match secret {
        DriveSecretInput::Typed { password } => (DriveSecret::new(password), None),
        DriveSecretInput::Entry { item_id } => {
            let passwords = passwords.ok_or(DriveError::NoVault)?;
            (wait(unlock::read_password(passwords, &item_id))?, Some(item_id))
        }
    };
    let path = backend.unlock(id, &secret)?;
    let remember_failed = match remember {
        DriveRemember::No => None,
        DriveRemember::Yes => {
            remember_it(passwords, drive.key.as_deref(), picked, &secret, entry_title).err()
        }
    };
    Ok(DriveOpened::Opened {
        path,
        remember_failed,
    })
}

/// Remembers the password of a drive that is open now; a failure leaves it open.
fn remember_it(
    passwords: Option<&PasswordsService>,
    key: Option<&str>,
    picked: Option<String>,
    secret: &DriveSecret,
    entry_title: &str,
) -> Result<(), DriveRememberFailed> {
    let (Some(passwords), Some(key)) = (passwords, key) else {
        return Err(DriveRememberFailed::Failed);
    };
    let remembered = match picked {
        None => wait(unlock::remember_typed(passwords, key, entry_title, secret)),
        Some(item_id) => wait(unlock::link_entry(passwords, &item_id, key, false)),
    };
    remembered.map_err(|error| match error {
        DriveError::UrlTaken => DriveRememberFailed::UrlTaken,
        other => {
            log::warn!("files: remembering a drive password failed: {other:?}");
            DriveRememberFailed::Failed
        }
    })
}

/// [`files_drive_link_entry`] against any backend and password manager.
pub(crate) fn link(
    backend: &dyn DriveBackend,
    passwords: Option<&PasswordsService>,
    key: &str,
    item_id: &str,
) -> Result<(), FilesError> {
    if !backend
        .list()?
        .iter()
        .any(|drive| drive.key.as_deref() == Some(key))
    {
        return Err(DriveError::Gone.into());
    }
    let passwords = passwords.ok_or(DriveError::NoVault)?;
    Ok(wait(unlock::link_entry(passwords, item_id, key, true))?)
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
    let drive = listed(backend, id)?;
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
