//! `ContactsService`: the single entrance to the address book (spec 051 FR-028, research R1). The
//! window, the built-in agent, extensions, external agents and holzi's own functions all come
//! through here, each as a [`Caller`]; no code reads or writes the `haex_contacts_*` tables around
//! it, and the Tauri commands call nothing else.
//!
//! Every method takes the caller first and checks it with [`super::access`] before it touches
//! data. The caller is the entrance, never an argument of a command. Each area adds its methods in
//! its own file (`cards`, `contacts`, `groups`, `targets`).

mod cards;
mod contacts;
mod groups;
mod share;
mod targets;
#[cfg(test)]
pub(crate) mod test_support;

use crate::vault_gate::VaultDb;

pub use super::access::{Caller, Grant};
pub use cards::VaultCard;
pub use contacts::MAX_NOTE_BYTES;
pub use targets::{OnPhone, Selection};

/// The service over one open vault. Cheap to build per request.
#[derive(Clone)]
pub struct ContactsService {
    db: VaultDb,
}

impl ContactsService {
    pub fn new(db: VaultDb) -> Self {
        Self { db }
    }

    /// The vault handle for the area files of this module; nothing outside `contacts::service`
    /// gets it, so no code can bypass the checks.
    fn db(&self) -> &VaultDb {
        &self.db
    }

    /// The `vault_device_uuid` of this device, the origin of what is created here.
    fn device(&self) -> String {
        self.db.device_id().to_string()
    }
}

/// How a caller is written into `origin_caller`.
fn caller_text(caller: &Caller) -> String {
    match caller {
        Caller::User => "user".to_owned(),
        Caller::BuiltinAgent => "builtin_agent".to_owned(),
        Caller::Extension { id } => format!("extension:{id}"),
        Caller::ExternalAgent { id } => format!("external_agent:{id}"),
        Caller::Internal { feature } => format!("internal:{feature}"),
        Caller::BrowserClient { id } => format!("browser_client:{id}"),
    }
}
