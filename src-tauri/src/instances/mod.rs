//! Instance lifecycle: create, open, close, list.
//!
//! Contract: `specs/001-frontend-onboarding/contracts/tauri-commands.md`.
//! One file per command mirrors haex-vault 2.x's (Legacy) convention.

pub mod cleanup;
pub mod close;
pub mod link_vault;
pub mod lock_retry;
pub mod passphrase;
pub mod paths;
pub mod presence;
pub mod startup;
pub mod vault_config;
pub mod vault_id;

mod close_effects;
mod create;
mod events;
mod import;
mod info;
mod list;
mod open;
mod passphrase_change;

pub use close::{close_instance, start_close, take_over_exit};
pub use create::{create_instance, create_instance_core, CreateInstanceArgs, CreateInstanceResult};
pub use import::{import_instance, import_instance_core, ImportInstanceArgs};
pub use info::InstanceInfo;
pub use list::{active_instance_name, list_instances};
pub use lock_retry::retry_while_locked;
pub use open::{open_instance, open_instance_core, OpenInstanceArgs};
pub use passphrase_change::{
    change_vault_passphrase, change_vault_passphrase_core, ChangePassphraseArgs,
};
pub use presence::ProcessPresence;
pub use startup::cleanup_orphans_on_startup;

#[cfg(test)]
mod paths_tests;
#[cfg(test)]
mod startup_tests;
