//! The commands of the setting "Protokoll" (spec 098; `contracts/commands.md`).

use serde::Deserialize;
use tauri::{AppHandle, State};

use super::expiry::{self, DEADLINE};
use super::level::LogLevel;
use super::{
    apply_state, cancel_expiry, emitter, now_ms, schedule_expiry, status, DiagnosticLogStatus,
    Emitter, SINK,
};
use crate::error::Result;
use crate::state::AppState;
use crate::state_utils::active_database;
use crate::vault_gate::{VaultDb, VaultGate};

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

#[cfg(test)]
#[path = "commands_tests.rs"]
mod tests;
