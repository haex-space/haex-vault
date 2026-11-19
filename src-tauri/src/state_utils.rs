//! Helpers for pulling the active `Database` out of `AppState` behind
//! the standard error mapping. Every command that reads or writes the
//! active vault uses this.

use tauri::State;

use crate::error::Result;
use crate::state::AppState;
use crate::vault_gate::VaultDb;

/// Returns the currently-active database as a tracked [`VaultDb`],
/// [`crate::HaexError::VaultClosed`] once a close has started, or
/// [`crate::HaexError::NoActiveInstance`] if no instance is open.
pub fn active_database(state: &State<'_, AppState>) -> Result<VaultDb> {
    state.database()
}
