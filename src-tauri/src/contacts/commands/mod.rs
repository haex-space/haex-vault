//! The Tauri commands of the address book (spec 051, contracts/tauri-commands.md). Each calls only
//! the [`ContactsService`], with the caller its entrance stands for: the commands of the window
//! are `Caller::User`. No command takes a caller as an argument.

pub mod cards;
pub mod contacts;
pub mod phone;
pub mod targets;

use tauri::State;

use super::service::ContactsService;
use crate::error::Result;
use crate::state::AppState;
use crate::state_utils::active_database;

/// The service over the active vault; `VaultClosed` or `NoActiveInstance` when there is none.
pub(crate) fn service(state: &State<'_, AppState>) -> Result<ContactsService> {
    Ok(ContactsService::new(active_database(state)?))
}
