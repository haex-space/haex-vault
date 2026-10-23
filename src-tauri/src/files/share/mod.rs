//! File shares (spec 056): folders and files a device serves to contacts, their grants per contact
//! vault, and the switch that lets other own devices reach this device's files. The rows are vault
//! data (data-model.md); the serving device checks them on every request (FR-005).

pub mod device_access;
pub mod store;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::files::SourceRef;

/// The holzi function the contact lookups of file shares run as (`Caller::Internal`).
pub const FEATURE: &str = "fileShares";

/// How far a contact may go in a share (FR-016): read, or read and write below the root.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub enum FileShareLevel {
    Read,
    ReadWrite,
}

impl FileShareLevel {
    /// The text stored in the vault.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Read => "read",
            Self::ReadWrite => "readWrite",
        }
    }

    /// The value of a stored text; `None` for one a newer version wrote.
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "read" => Some(Self::Read),
            "readWrite" => Some(Self::ReadWrite),
            _ => None,
        }
    }
}

/// Whether the root of a share is a folder or a single file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub enum FileShareItemKind {
    Folder,
    File,
}

impl FileShareItemKind {
    /// The text stored in the vault.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Folder => "folder",
            Self::File => "file",
        }
    }

    /// The value of a stored text; `None` for an unknown one.
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "folder" => Some(Self::Folder),
            "file" => Some(Self::File),
            _ => None,
        }
    }
}

/// What the serving device last found at the root of a share (FR-022).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub enum FileShareSourceState {
    Ok,
    Missing,
    Unavailable,
}

impl FileShareSourceState {
    /// The text stored in the vault.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::Missing => "missing",
            Self::Unavailable => "unavailable",
        }
    }

    /// The value of a stored text; an unknown one reads as `Unavailable`.
    pub fn parse(text: &str) -> Self {
        match text {
            "ok" => Self::Ok,
            "missing" => Self::Missing,
            _ => Self::Unavailable,
        }
    }
}

/// The stored text of a share's source: `device` or `storage:<id>`. Sources of other devices and
/// received shares cannot be shared (FR-015): the device that holds a source shares it.
pub fn source_text(source: &SourceRef) -> Option<String> {
    match source {
        SourceRef::Device => Some("device".to_owned()),
        SourceRef::Storage { storage_id } if is_local_storage(storage_id) => {
            Some(format!("storage:{storage_id}"))
        }
        SourceRef::Storage { .. } => None,
    }
}

/// The source a stored text names; `None` for one this version does not know.
pub fn source_of(text: &str) -> Option<SourceRef> {
    if text == "device" {
        return Some(SourceRef::Device);
    }
    let storage_id = text.strip_prefix("storage:")?;
    is_local_storage(storage_id).then(|| SourceRef::Storage {
        storage_id: storage_id.to_owned(),
    })
}

/// Whether `storage_id` names a storage of this vault rather than another device (`peer:`) or a
/// received share (`share:`), which only stand for storages on the requesting side (research R2).
pub fn is_local_storage(storage_id: &str) -> bool {
    !storage_id.is_empty()
        && !storage_id.starts_with(PEER_PREFIX)
        && !storage_id.starts_with(SHARE_PREFIX)
}

/// Prefix of the storage id of another own device: `peer:<vault_device_uuid>`.
pub const PEER_PREFIX: &str = "peer:";
/// Prefix of the storage id of a received share: `share:<received_id>`.
pub const SHARE_PREFIX: &str = "share:";
