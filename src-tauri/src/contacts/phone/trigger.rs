//! When the sync with the phone runs on its own (spec 051 FR-020, research R6): after the vault is
//! opened and when holzi comes back to the foreground, never in the background (spec 043 FR-018).
//! One run at a time: a start while one runs asks for one more run after it, so starts in quick
//! succession collapse into one.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use tauri::{AppHandle, Emitter, Manager, Runtime};

use super::android::AndroidPhoneBook;
use super::{run, state, Mode, Paused, RunSummary};
use crate::passwords::clock;
use crate::state::AppState;
use crate::vault_gate::VaultDb;

/// Sent when the state of the sync changed (contracts/tauri-commands.md).
pub const STATUS_EVENT: &str = "contacts-phone-status";
/// Sent when a run held back deletions (FR-016).
pub const MASS_DELETE_EVENT: &str = "contacts-phone-mass-delete";

/// The one run at a time, shared by the automatic runs and the commands.
#[derive(Default)]
pub struct PhoneRuns {
    lock: Mutex<()>,
    again: AtomicBool,
}

impl PhoneRuns {
    /// Waits for a running run and holds the next one.
    pub fn hold(&self) -> MutexGuard<'_, ()> {
        self.lock.lock().unwrap_or_else(|error| error.into_inner())
    }
}

/// Tells the window what a run did.
pub fn announce<R: Runtime>(app: &AppHandle<R>, db: &VaultDb, summary: &RunSummary) {
    let device = db.device_id().to_string();
    if let Ok(status) = db.read_blocking(move |q| state::status(q, &device).map_err(Into::into)) {
        let _ = app.emit(STATUS_EVENT, &status);
    }
    if let (Some(Paused::MassDelete), Some(held)) = (summary.paused, &summary.mass_delete) {
        let _ = app.emit(MASS_DELETE_EVENT, held);
    }
}

/// Starts a run of the open vault in the background, if this device has a phone address book and
/// the sync is on. A start during a run asks for one more run after it.
pub fn start<R: Runtime>(app: &AppHandle<R>) {
    if !crate::platform::capabilities().phone_contacts {
        return;
    }
    let Some(runs) = app.try_state::<Arc<PhoneRuns>>() else {
        return;
    };
    let runs = Arc::clone(&runs);
    let state = app.state::<AppState>();
    let Ok(db) = state.database() else {
        return;
    };
    let app = app.clone();
    let started = state.gate().spawn_blocking(move || {
        let Ok(_held) = runs.lock.try_lock() else {
            runs.again.store(true, Ordering::SeqCst);
            return;
        };
        loop {
            runs.again.store(false, Ordering::SeqCst);
            run_once(&app, &db);
            if !runs.again.load(Ordering::SeqCst) {
                break;
            }
        }
    });
    if let Err(error) = started {
        log::warn!("contacts: the sync with the phone could not start: {error}");
    }
}

fn run_once<R: Runtime>(app: &AppHandle<R>, db: &VaultDb) {
    let device = db.device_id().to_string();
    let on = {
        let device = device.clone();
        db.read_blocking(move |q| state::enabled(q, &device).map_err(Into::into))
    };
    if !matches!(on, Ok(true)) {
        return;
    }
    let book = AndroidPhoneBook::new(app.clone());
    match run(db, &book, &device, Mode::Normal, &clock::now()) {
        Ok(summary) => announce(app, db, &summary),
        Err(error) => log::warn!("contacts: the sync with the phone failed: {error}"),
    }
}
