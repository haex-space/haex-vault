//! `PasswordsService`: the single entrance to the data of the password manager (spec 034,
//! `contracts/access.md` §Dienst, FR-024). The window, the built-in agent, extensions, external
//! agents and holzi functions all come through here, each as a [`Caller`]; no code reads or writes
//! the `haex_passwords_*` tables around it, and the Tauri commands call nothing else.
//!
//! Every method takes the caller first and checks it before it touches data. The caller is the
//! entrance, never an argument of a command: a command fixes it (`Caller::User` for the window,
//! `Caller::BuiltinAgent` for the chat action), a holzi function passes `Caller::Internal` with a
//! grant fixed in its code. Each area adds its methods in its own file (`items`, `organize`,
//! `trash`, `history`, `attachments`, `passkeys`, `presets`, `import`, `usage`).
//!
//! The rules themselves are the pure functions of [`super::access`]; a method calls the one it needs
//! and passes the `Denied` through `?`, which maps it to `PasswordsForbidden` or
//! `PasswordsNotFound`. Everything beyond the five item methods is for the user alone ([`require_user`],
//! rule Z11).

use std::sync::Arc;

use super::access::{self, Caller};
use super::usage::UsageRegistry;

mod attachments;
mod by_site;
mod copy;
mod history;
pub mod import;
mod items;
mod organize;
mod owned;
mod passkeys;
mod presets;
mod references;
mod trash;
mod usage;

use crate::error::Result;
use crate::vault_gate::VaultDb;
pub use items::Headers;

/// The service over one open vault. Cheap to build per request; it holds only the tracked handle.
#[derive(Clone)]
pub struct PasswordsService {
    db: VaultDb,
    usage: Arc<UsageRegistry>,
}

impl PasswordsService {
    /// A service with no function that reports entries in use (tests, and a vault without any).
    pub fn new(db: VaultDb) -> Self {
        Self::with_usage(db, Arc::new(UsageRegistry::new()))
    }

    /// A service that asks `usage` which functions use an entry.
    pub fn with_usage(db: VaultDb, usage: Arc<UsageRegistry>) -> Self {
        Self { db, usage }
    }

    pub(super) fn usage(&self) -> &UsageRegistry {
        &self.usage
    }

    /// The vault handle for the area files of this module; nothing outside `passwords::service`
    /// gets it, so no code can bypass the checks.
    pub(super) fn db(&self) -> &VaultDb {
        &self.db
    }
}

/// Rule Z11: the method is for the user alone. Call it first in every method that is not one of the
/// five item methods.
pub(super) fn require_user(caller: &Caller) -> Result<()> {
    access::require_user(caller).map_err(Into::into)
}
