//! Who may do what with files (spec 044 FR-031–FR-033, FR-037, research R14), pure and testable
//! without a database, like `passwords/access.rs`.
//!
//! - The user reaches the device and every storage; holzi's own places only read-only.
//! - Agents (built-in, and external ones of spec 021) never reach holzi's own places, before any
//!   grant is asked. The device needs the grant "files of the device": the built-in agent has it
//!   unless revoked, an external agent only once granted. A storage needs a grant for exactly that
//!   storage; without one the caller asks the user, `read` never writes, `denied` refuses.
//! - An encrypted folder of a storage (spec 048 FR-030 to FR-032) needs, on top of the storage's
//!   grant, one for exactly that folder, never more than the storage's. It covers either only local
//!   models or also cloud models; an answer without "Erlaubnis merken" holds only for the task.
//! - Extensions and holzi functions use their own entrances (`extensions::fs`, `remote_storage`).
//!
//! Paths are checked as resolved targets ([`crate::files::local::resolve`]), never as spelled.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::files::local::OwnPlaces;
use crate::files::{FilesError, FilesErrorCode};
use crate::passwords::access::Caller;

/// What a call touches.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    /// A resolved path on this device.
    Device(PathBuf),
    /// A storage of spec 038, by its id.
    Storage(String),
}

/// What a call does with its target.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Want {
    Read,
    Write,
}

/// The agent's grant "files of the device".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceGrant {
    Granted,
    Denied,
}

/// The agent's grant for one storage.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StorageGrant {
    Read,
    ReadWrite,
    Denied,
}

/// Where the model of an agent runs (spec 048 FR-030a); for a stored folder grant, which models it
/// covers: `Local` only local ones, `Cloud` also cloud ones.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Locality {
    Local,
    Cloud,
}

/// The agent's stored grant for one encrypted folder.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FolderGrant {
    pub grant: StorageGrant,
    pub reach: Locality,
}

/// The stored grants of one agent (`agent_file_permissions`, data-model.md); absent means no row.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AgentGrants {
    pub device: Option<DeviceGrant>,
    pub storages: HashMap<String, StorageGrant>,
    /// By `<storage_id>/<folder_id>`.
    pub folders: HashMap<String, FolderGrant>,
}

impl AgentGrants {
    pub fn with_storage(mut self, storage_id: &str, grant: StorageGrant) -> Self {
        self.storages.insert(storage_id.to_owned(), grant);
        self
    }
}

/// The answer for one call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    Allowed,
    /// The user reads one of holzi's own places (FR-037).
    ReadOnly,
    /// No grant for this storage yet: ask the user, or refuse when no one can be asked.
    Ask,
    Refused(FilesError),
}

/// Whether `caller` may `want` `target`.
pub fn check(
    caller: &Caller,
    target: &Target,
    want: Want,
    own: &OwnPlaces,
    grants: &AgentGrants,
) -> Verdict {
    match caller {
        Caller::User => match target {
            Target::Device(path) if own.contains(path) => match want {
                Want::Read => Verdict::ReadOnly,
                Want::Write => refuse(FilesErrorCode::HolziOwned, "holzi's own data is read-only"),
            },
            _ => Verdict::Allowed,
        },
        Caller::BuiltinAgent | Caller::ExternalAgent { .. } => {
            agent(caller, target, want, own, grants)
        }
        Caller::Extension { .. } | Caller::Internal { .. } => refuse(
            FilesErrorCode::NotGranted,
            "this caller reaches files through its own entrance",
        ),
    }
}

fn agent(
    caller: &Caller,
    target: &Target,
    want: Want,
    own: &OwnPlaces,
    grants: &AgentGrants,
) -> Verdict {
    match target {
        Target::Device(path) => {
            if own.contains(path) {
                return refuse(
                    FilesErrorCode::Blocked,
                    "holzi's own data is not available to agents",
                );
            }
            if device_granted(caller, grants) {
                Verdict::Allowed
            } else {
                refuse(
                    FilesErrorCode::NotGranted,
                    "the files of the device are not granted",
                )
            }
        }
        Target::Storage(id) => match (grants.storages.get(id), want) {
            (None, _) => Verdict::Ask,
            (Some(StorageGrant::Denied), _) => {
                refuse(FilesErrorCode::NotGranted, "this storage is not granted")
            }
            (Some(StorageGrant::Read), Want::Write) => refuse(
                FilesErrorCode::NotGranted,
                "this storage is granted for reading only",
            ),
            (Some(StorageGrant::Read | StorageGrant::ReadWrite), _) => Verdict::Allowed,
        },
    }
}

/// Whether an agent may `want` the encrypted folder `folder` (`<storage_id>/<folder_id>`) of
/// `storage_id` while its model runs `locally` or not. `held` is the answer of this task without
/// "Erlaubnis merken"; it comes before a stored row. The storage's grant is settled first (the
/// caller asked for it already) and caps the folder's.
pub fn check_folder(
    caller: &Caller,
    storage_id: &str,
    folder: &str,
    want: Want,
    grants: &AgentGrants,
    held: Option<StorageGrant>,
    model: Locality,
) -> Verdict {
    if !matches!(caller, Caller::BuiltinAgent | Caller::ExternalAgent { .. }) {
        return check(
            caller,
            &Target::Storage(storage_id.to_owned()),
            want,
            &OwnPlaces::default(),
            grants,
        );
    }
    match (grants.storages.get(storage_id), want) {
        (None | Some(StorageGrant::Denied), _) => {
            return refuse(FilesErrorCode::NotGranted, "this storage is not granted");
        }
        (Some(StorageGrant::Read), Want::Write) => {
            return refuse(
                FilesErrorCode::NotGranted,
                "this storage is granted for reading only",
            );
        }
        _ => {}
    }
    let grant = match (held, grants.folders.get(folder)) {
        (Some(held), _) => held,
        (None, None) => return Verdict::Ask,
        // Granted for local models only, and this task uses a cloud model: ask again.
        (None, Some(stored)) if stored.reach == Locality::Local && model == Locality::Cloud => {
            return Verdict::Ask;
        }
        (None, Some(stored)) => stored.grant,
    };
    match (grant, want) {
        (StorageGrant::Denied, _) => refuse(
            FilesErrorCode::NotGranted,
            "this encrypted folder is not granted",
        ),
        (StorageGrant::Read, Want::Write) => refuse(
            FilesErrorCode::ReadOnly,
            "this encrypted folder is granted for reading only",
        ),
        (StorageGrant::Read | StorageGrant::ReadWrite, _) => Verdict::Allowed,
    }
}

fn device_granted(caller: &Caller, grants: &AgentGrants) -> bool {
    match grants.device {
        Some(DeviceGrant::Granted) => true,
        Some(DeviceGrant::Denied) => false,
        None => matches!(caller, Caller::BuiltinAgent),
    }
}

/// Whether an entry at the resolved `path` may appear in a list or search result for `caller`:
/// agents never see holzi's own places (FR-033).
pub fn visible_to(caller: &Caller, path: &Path, own: &OwnPlaces, grants: &AgentGrants) -> bool {
    match caller {
        Caller::BuiltinAgent | Caller::ExternalAgent { .. } => {
            !own.contains_resolved(path) && device_granted(caller, grants)
        }
        _ => true,
    }
}

/// Whether a storage appears for `caller` at all: agents see only storages they hold a grant for
/// (FR-031a).
pub fn storage_listed(caller: &Caller, storage_id: &str, grants: &AgentGrants) -> bool {
    match caller {
        Caller::BuiltinAgent | Caller::ExternalAgent { .. } => matches!(
            grants.storages.get(storage_id),
            Some(StorageGrant::Read | StorageGrant::ReadWrite)
        ),
        _ => true,
    }
}

fn refuse(code: FilesErrorCode, message: &str) -> Verdict {
    Verdict::Refused(FilesError::new(code, message))
}

#[cfg(test)]
#[path = "access_tests.rs"]
mod tests;
