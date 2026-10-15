//! When the sync with the phone runs on its own (spec 051 FR-020, FR-021, research R6): after the
//! vault is opened, when holzi comes back to the foreground, and when contacts change in the vault
//! (from this device or received from another one), never in the background (spec 043 FR-018).
//! One run at a time: a start while one runs asks for one more run after it, so starts in quick
//! succession collapse into one.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

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

/// How long changes are collected before a run starts: a page of the vault sync is one burst.
const SETTLE: Duration = Duration::from_millis(500);

/// Whether a change of `table` may concern the phone: contacts, their values, groups, photos and
/// targets. The rows a run writes about itself (links, issues, its state, the known list) are left
/// out, so a run does not start the next one.
fn concerns_phone(table: &str) -> bool {
    table.starts_with("haex_contacts") && !table.starts_with("haex_contacts_phone_")
}

/// Starts a run whenever contacts change in the open vault, until it closes (FR-021, SC-003).
pub fn watch<R: Runtime>(app: &AppHandle<R>) {
    if !crate::platform::capabilities().phone_contacts {
        return;
    }
    let state = app.state::<AppState>();
    let mut changes = state.vault_changes().subscribe();
    let gate = state.gate().clone();
    let token = gate.token();
    let app = app.clone();
    let follow = async move {
        loop {
            let received = tokio::select! {
                () = token.cancelled() => return,
                received = changes.recv() => received,
            };
            let relevant = match received {
                Ok(tables) => tables.iter().any(|table| concerns_phone(table)),
                // Fell behind: anything may have changed.
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => true,
                Err(tokio::sync::broadcast::error::RecvError::Closed) => return,
            };
            if !relevant {
                continue;
            }
            tokio::select! {
                () = token.cancelled() => return,
                () = tokio::time::sleep(SETTLE) => {}
            }
            while changes.try_recv().is_ok() {}
            begin(&app, false);
        }
    };
    if let Err(error) = gate.spawn(follow) {
        log::warn!("contacts: changes will not start the sync with the phone: {error}");
    }
}

/// The one run at a time, shared by the automatic runs and the commands.
#[derive(Default)]
pub struct PhoneRuns {
    lock: Mutex<()>,
    again: AtomicBool,
    /// The run asked for after the current one is a full one (not only for changes in the vault).
    again_full: AtomicBool,
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
    begin(app, true);
}

/// Starts a full run, or one for changes in the vault ([`Mode::Changes`]).
fn begin<R: Runtime>(app: &AppHandle<R>, full: bool) {
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
            if full {
                runs.again_full.store(true, Ordering::SeqCst);
            }
            runs.again.store(true, Ordering::SeqCst);
            return;
        };
        let mut full = full;
        loop {
            runs.again.store(false, Ordering::SeqCst);
            run_once(&app, &db, if full { Mode::Normal } else { Mode::Changes });
            if !runs.again.load(Ordering::SeqCst) {
                break;
            }
            full = runs.again_full.swap(false, Ordering::SeqCst);
        }
    });
    if let Err(error) = started {
        log::warn!("contacts: the sync with the phone could not start: {error}");
    }
}

fn run_once<R: Runtime>(app: &AppHandle<R>, db: &VaultDb, mode: Mode<'_>) {
    let device = db.device_id().to_string();
    let on = {
        let device = device.clone();
        db.read_blocking(move |q| state::enabled(q, &device).map_err(Into::into))
    };
    if !matches!(on, Ok(true)) {
        return;
    }
    let book = AndroidPhoneBook::new(app.clone());
    match run(db, &book, &device, mode, &clock::now()) {
        Ok(summary) => announce(app, db, &summary),
        Err(error) => log::warn!("contacts: the sync with the phone failed: {error}"),
    }
}

#[cfg(test)]
#[path = "trigger_tests.rs"]
mod tests;
