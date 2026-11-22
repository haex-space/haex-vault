//! The commands of the setting "Protokoll" (spec 098; `contracts/commands.md`).

use std::fs::File;
use std::io;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use tauri::{AppHandle, Manager, State};
use tauri_plugin_haex_android::HaexAndroidExt;
use tauri_plugin_opener::OpenerExt;

use super::expiry::{self, DEADLINE};
use super::level::LogLevel;
use super::rotation::{files_oldest_first, FILE_NAME};
use super::{
    apply_state, cancel_expiry, emitter, now_ms, schedule_expiry, status, DiagnosticLogStatus,
    Emitter, SINK,
};
use crate::error::{HaexError, Result};
use crate::state::AppState;
use crate::state_utils::active_database;
use crate::vault_gate::{VaultDb, VaultGate};

/// The folder in the app's cache that holds the copy of the log handed to the share sheet, and the
/// copy's name. The Android plugin's file provider exposes only this folder.
const SNAPSHOT_DIR: &str = "diagnostic-log";
const SNAPSHOT_FILE: &str = "haex-diagnostic-log.txt";

static LEVEL_TRANSITION: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// The severity of a message of the interface.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum UiLevel {
    Error,
    Warn,
    Info,
    Debug,
}

impl From<UiLevel> for log::Level {
    fn from(level: UiLevel) -> Self {
        match level {
            UiLevel::Error => Self::Error,
            UiLevel::Warn => Self::Warn,
            UiLevel::Info => Self::Info,
            UiLevel::Debug => Self::Debug,
        }
    }
}

/// Switches the level at once: stores it, applies it to the sink and the `log` macros, starts or
/// stops the timer of "Ausführlich" and announces it. Choosing the level in force changes nothing.
pub(crate) async fn switch_level(
    db: &VaultDb,
    gate: &VaultGate,
    level: LogLevel,
    emit: &Emitter,
) -> Result<DiagnosticLogStatus> {
    let _transition = LEVEL_TRANSITION.lock().await;
    switch_level_locked(db, gate, level, emit).await
}

/// Applies an expiry only when the timer still belongs to the current verbose selection.
pub(crate) async fn expire_level(
    db: &VaultDb,
    gate: &VaultGate,
    generation: u64,
    until_ms: i64,
    emit: &Emitter,
) -> Result<()> {
    let _transition = LEVEL_TRANSITION.lock().await;
    let Some(timer) = super::take_expiry(generation) else {
        return Ok(());
    };
    if status().level != LogLevel::Verbose || status().until_ms != Some(until_ms) {
        return Ok(());
    }
    match switch_level_locked(db, gate, LogLevel::Standard, emit).await {
        Ok(_) => Ok(()),
        Err(error) => {
            super::restore_expiry(generation, timer);
            Err(error)
        }
    }
}

async fn switch_level_locked(
    db: &VaultDb,
    gate: &VaultGate,
    level: LogLevel,
    emit: &Emitter,
) -> Result<DiagnosticLogStatus> {
    if status().level == level {
        return Ok(status());
    }
    let now = now_ms();
    let state = db
        .write(move |tx| expiry::switch(tx, level, expiry::run_id(), now).map_err(Into::into))
        .await?;
    match level {
        LogLevel::Verbose => DEADLINE.set(std::time::Instant::now()),
        LogLevel::Standard => DEADLINE.clear(),
    }
    let worker_level = state.level;
    match tokio::task::spawn_blocking(move || SINK.set_level(worker_level)).await {
        Ok(Ok(())) => {}
        Ok(Err(error)) => log::warn!("diagnostic log: could not resize its files: {error}"),
        Err(error) => log::warn!("diagnostic log: could not update its writer: {error}"),
    }
    apply_state(state);
    cancel_expiry();
    if let Some(until_ms) = state.until_ms {
        schedule_expiry(db.clone(), gate, until_ms, emit);
    }
    let status = status();
    emit(status.clone());
    Ok(status)
}

/// Writes a message of the interface into the log of the vault that is open, if `session_id` is
/// that vault's token; otherwise it is dropped (spec FR-007).
pub(crate) fn ui_message(session_id: &str, level: UiLevel, message: &str) {
    SINK.write_ui(session_id, level.into(), message);
}

/// The level, its end, whether the log can be written, and the token of this unlock.
#[tauri::command]
pub async fn diagnostic_log_get(state: State<'_, AppState>) -> Result<DiagnosticLogStatus> {
    active_database(&state)?;
    Ok(status())
}

/// Switches the level; it applies at once, for the backend and the interface.
#[tauri::command]
pub async fn diagnostic_log_set(
    app: AppHandle,
    state: State<'_, AppState>,
    level: LogLevel,
) -> Result<DiagnosticLogStatus> {
    let db = active_database(&state)?;
    switch_level(&db, state.gate(), level, &emitter(&app)).await
}

/// A message of the interface (a warning, an error, an uncaught error). Text and stack only; the
/// interface has cut and rate limited it already.
#[tauri::command]
pub fn diagnostic_log_ui_message(session_id: String, level: UiLevel, message: String) {
    ui_message(&session_id, level, &message);
}

/// The error of a function this platform does not have.
fn unsupported() -> HaexError {
    HaexError::InvalidInput {
        reason: "unsupported".to_string(),
    }
}

fn require(supported: bool) -> Result<()> {
    if supported {
        Ok(())
    } else {
        Err(unsupported())
    }
}

/// Joins the log files into `target`, in time order. The files stay as they are and the log keeps
/// writing; a file that rotates away while this runs is skipped.
pub(crate) fn snapshot_log(folder: &Path, target: &Path) -> io::Result<()> {
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut out = File::create(target)?;
    for file in files_oldest_first(folder) {
        match File::open(&file) {
            Ok(mut source) => {
                io::copy(&mut source, &mut out)?;
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
    }
    Ok(())
}

/// The log folder of the vault that is open.
fn log_folder(app: &AppHandle, state: &AppState) -> Result<PathBuf> {
    let name = state.active_name()?.ok_or(HaexError::NoActiveInstance)?;
    Ok(crate::instances::paths::vault_paths(app)?.logs_dir(&name))
}

/// Shows the current log file in the file manager (a computer only).
#[tauri::command]
pub async fn diagnostic_log_reveal(app: AppHandle, state: State<'_, AppState>) -> Result<()> {
    require(super::can_reveal())?;
    active_database(&state)?;
    let file = log_folder(&app, &state)?.join(FILE_NAME);
    app.opener()
        .reveal_item_in_dir(&file)
        .map_err(|error| HaexError::InvalidInput {
            reason: format!("the log cannot be shown: {error}"),
        })
}

/// Hands a copy of the log to the system's share sheet (Android only).
#[tauri::command]
pub async fn diagnostic_log_share(app: AppHandle, state: State<'_, AppState>) -> Result<()> {
    require(super::can_share())?;
    active_database(&state)?;
    let folder = log_folder(&app, &state)?;
    let target = app
        .path()
        .app_cache_dir()
        .map_err(|error| HaexError::InvalidInput {
            reason: format!("no cache folder: {error}"),
        })?
        .join(SNAPSHOT_DIR)
        .join(SNAPSHOT_FILE);
    let copy = target.clone();
    tokio::task::spawn_blocking(move || {
        SINK.flush_sync();
        snapshot_log(&folder, &copy)
    })
    .await
    .map_err(|error| HaexError::InvalidInput {
        reason: format!("the log copy did not finish: {error}"),
    })??;
    app.haex_android()
        .share_file(&target, "text/plain")
        .map_err(|reason| HaexError::InvalidInput { reason })
}

#[cfg(test)]
#[path = "commands_tests.rs"]
mod tests;
