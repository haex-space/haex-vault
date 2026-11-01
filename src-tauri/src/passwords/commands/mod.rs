//! The Tauri commands of the password manager (spec 034, `contracts/tauri-commands.md`). Each calls
//! only the [`PasswordsService`], with the caller that its entrance stands for: the commands of the
//! window are `Caller::User`; the agents' actions run in Rust (`passwords::agent_actions`). No
//! command takes a caller as an argument.

pub mod attachments;
pub mod copy;
pub mod history;
pub mod import;
pub mod items;
pub mod organize;
pub mod passkeys;
pub mod presets;
pub mod read;
pub mod references;
pub mod trash;
pub mod usage;

use tauri::State;

use super::service::PasswordsService;
use crate::error::Result;
use crate::state::AppState;
use crate::state_utils::active_database;

/// The service over the active vault; `VaultClosed` or `NoActiveInstance` when there is none.
pub(crate) fn service(state: &State<'_, AppState>) -> Result<PasswordsService> {
    Ok(PasswordsService::with_usage(
        active_database(state)?,
        state.usage(),
    ))
}

pub use crate::passwords::model::{Target, TargetKind};
