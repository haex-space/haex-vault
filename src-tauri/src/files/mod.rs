//! Files in holzi. Spec 043: files the person hands to holzi, chosen in the system's dialog, read
//! and written the same way on every platform (`picked`, `commands`). Spec 044: the file browser's
//! core, files of the device and of storages for the window, the built-in agent and extensions
//! (`local`, [`FilesError`]).

pub mod access;
pub mod agent;
pub mod browser_commands;
pub mod commands;
pub mod encrypted;
pub mod extract;
pub mod icons;
pub mod kind;
pub mod local;
pub mod media;
pub mod peer;
pub mod permissions;
pub mod picked;
pub mod search;
pub mod search_device;
pub mod search_manager;
pub mod share;
pub mod state;
pub mod storage_source;
pub mod storage_time;
pub mod streaming;
pub mod thumbnails;
pub mod transfer;

pub use picked::PickedFile;

#[cfg(test)]
pub mod test_support;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Why a file operation was refused (contracts/tauri-commands.md). The message never holds
/// credentials or file contents (FR-033, FR-038).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS, thiserror::Error)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
#[error("{message}")]
pub struct FilesError {
    pub code: FilesErrorCode,
    pub message: String,
}

/// The kinds of [`FilesError`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub enum FilesErrorCode {
    /// The path is not absolute, holds `.`/`..` after its existing part, or a broken link.
    InvalidPath,
    /// A name that is empty, too long or holds a character some system refuses.
    InvalidName,
    Exists,
    IntoItself,
    NoSpace,
    HolziOwned,
    NoAccess,
    NotFound,
    Binary,
    Broken,
    TooLarge,
    Blocked,
    NotGranted,
    Unsupported,
    /// A storage refused its credentials, or they are not on this device (spec 038).
    Credentials,
    /// A storage did not answer (network, timeout).
    Unreachable,
    /// The file changed since the caller last saw it (spec 056 FR-036); nothing was written.
    Conflict,
    /// A storage's server showed a host key other than the confirmed one (spec 050).
    HostKey,
    /// Not inside an encrypted folder (spec 048): another encrypted folder, for one.
    InEncrypted,
    /// Not in the area of an extension in a storage (spec 048 FR-001, 038 FR-010).
    InExtensionArea,
    /// This device holds no content key of the vault yet (before the first data sync).
    NoKey,
    /// An encrypted folder or an entry in it failed its checks (FR-022 to FR-025).
    Damaged,
    /// An encrypted folder of another vault.
    OtherVault,
    /// An encrypted folder of a newer format version.
    NewerFormat,
    /// An encrypted folder granted to an agent for reading only (spec 048, contracts/agent-actions.md).
    ReadOnly,
    /// Opening a file of an encrypted folder with the system's app needs the notice first
    /// (spec 048 FR-028); the window asks and calls again with `confirmed`.
    NeedsNotice,
    /// The system's prompt to mount, unlock or eject a drive was cancelled or refused (spec 064
    /// FR-007).
    DriveCancelled,
    /// Another program holds the drive (spec 064 FR-012).
    DriveBusy,
    /// The drive is not there any more.
    DriveGone,
    /// The system refused a drive action; the message is the system's.
    DriveFailed,
    /// Unlocking failed, mostly a wrong password; the message is the system's (FR-015).
    UnlockFailed,
    /// Unlocked, but no file system haex can open, such as LVM (FR-020).
    DriveNoFilesystem,
    /// This device cannot do that with its drives (no udisks2, Android).
    DriveUnsupported,
    /// The password manager needs an open vault (FR-018).
    NoVault,
    /// A password entry picked for a drive already has another address (FR-016).
    UrlTaken,
}

impl FilesError {
    pub fn new(code: FilesErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    pub fn invalid_path(message: impl Into<String>) -> Self {
        Self::new(FilesErrorCode::InvalidPath, message)
    }
}

/// Where the file browser looks (spec 044 FR-002): the device, or a storage of spec 038.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub enum SourceRef {
    Device,
    Storage {
        #[serde(rename = "storageId")]
        storage_id: String,
    },
}

/// A file or a folder.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub enum EntryKind {
    File,
    Dir,
}

/// One entry of a folder (data-model.md).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct Entry {
    pub name: String,
    /// Device: the absolute path; storage: the key, folders ending in `/`.
    pub path: String,
    pub kind: EntryKind,
    /// Files only.
    #[ts(type = "number | null")]
    pub size: Option<u64>,
    #[ts(type = "number | null")]
    pub modified_ms: Option<i64>,
    pub mime: Option<String>,
    pub hidden: bool,
    pub symlink: bool,
    /// The system denies access.
    pub no_access: bool,
    /// In one of holzi's own places: read-only for the user (FR-037).
    pub holzi_owned: bool,
    /// An encrypted folder or an entry in one (spec 048).
    pub encrypted: Option<EncryptedInfo>,
    /// A conflict copy after the format's rules; the name holds the addition already.
    pub conflict: bool,
}

/// Where an entry stands to encryption (spec 048, contracts/tauri-commands.md).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct EncryptedInfo {
    pub role: EncryptedRole,
    pub state: EncryptedState,
    /// Files in the folder that are not part of the format (research R19), for the folder itself
    /// when it was opened; 0 for an entry inside and in a listing of the folder's parent.
    pub foreign: u32,
}

/// The encrypted folder itself, or an entry in one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub enum EncryptedRole {
    Folder,
    Inside,
}

/// What holzi can do with an encrypted folder or an entry (data-model.md). A `damaged` entry can
/// only be deleted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub enum EncryptedState {
    Readable,
    OtherVault,
    NewerFormat,
    Damaged,
}
