//! The messages of `haex-vault-files/1` after the exchange of [`crate::sync::foreign_auth`]
//! (contracts/files-protocol.md): `Welcome` on the first stream, then one bi-stream per request with
//! a head ([`write_head`], [`read_head`]: length and postcard, at most [`HEAD_LIMIT`]) and, for a
//! read, the raw bytes after the answer's head.

use serde::{Deserialize, Serialize};
use tokio::io::{AsyncRead, AsyncWrite};

use crate::files::share::{FileShareItemKind, FileShareLevel};
use crate::sync::wire::{expect_value, write_value, WireError};

/// The version every message carries.
pub const VERSION: u8 = 1;
/// The largest head of a request or an answer.
pub const HEAD_LIMIT: usize = 64 * 1024;
/// The most entries of one listing (`MAX_ENTRIES` of the file browser).
pub const MAX_LIST: u32 = 100_000;
/// The largest part of an upload.
pub const MAX_PART: u64 = 64 * 1024 * 1024;

/// What the serving device lets the other side do (research R4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Mode {
    /// Another device of the same vault: the whole file system (FR-011, FR-012).
    OwnDevice,
    /// Another device of the same vault, but this device switched it off (FR-014): no request is
    /// answered.
    AccessOff,
    /// A device of a contact: only the roots shared with its vault (FR-004).
    Contact,
}

/// The serving device's answer to the exchange.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Welcome {
    pub v: u8,
    pub mode: Mode,
    /// Whether deleting here goes to a trash (research R8).
    pub trash: bool,
}

/// Who asks on the requesting device: the user, or one of its agents (FR-007: holzi's own data is
/// read-only for the user, blocked for agents).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Caller {
    User,
    Agent,
}

/// One request on its own stream.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Request {
    pub v: u8,
    pub caller: Caller,
    pub op: Op,
}

/// What a request asks for. `share` names the root for a contact and is `None` for an own device;
/// paths are window paths (`/`, `/a/b`) below the device root or the share root.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Op {
    /// The shares for the asking vault (contact).
    Shares,
    /// One level of the folder `path`, at most `max` entries.
    ListDir {
        share: Option<String>,
        path: String,
        max: u32,
    },
    /// Size, time and mark of the entry `path`.
    Stat { share: Option<String>, path: String },
    /// `len` bytes of the file `path` from `start`; the answer's head is followed by the bytes.
    Read {
        share: Option<String>,
        path: String,
        start: u64,
        len: u64,
    },
    /// Starts writing the file `path`; it finishes only while the file is still as `expected`.
    BeginUpload {
        share: Option<String>,
        path: String,
        expected: Expected,
    },
    /// Part `number` (from 1, in order) of an upload of this connection; `len` bytes follow the
    /// head.
    UploadPart {
        upload_id: String,
        number: u16,
        len: u64,
    },
    /// Puts the uploaded file in place, or answers `Conflict` and leaves the old one.
    FinishUpload { upload_id: String },
    /// Drops an upload and its temporary file.
    AbortUpload { upload_id: String },
    /// Creates the folder `path`.
    CreateFolder { share: Option<String>, path: String },
    /// Moves or renames `from` to `to` below the same root.
    Move {
        share: Option<String>,
        from: String,
        to: String,
    },
    /// Removes the file or folder `path`, into the trash where there is one (research R8).
    Delete { share: Option<String>, path: String },
    /// Reports changes in the folder `path` until the stream ends (research R16).
    Watch { share: Option<String>, path: String },
}

/// What a file must still be when an upload finishes (FR-036).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Expected {
    /// There was no file at the path.
    Absent,
    /// The file as the requesting device saw it.
    Version { size: u64, etag: String },
}

impl Op {
    /// The root this request is about, if any.
    pub fn share(&self) -> Option<&str> {
        match self {
            Op::Shares => None,
            Op::ListDir { share, .. }
            | Op::Stat { share, .. }
            | Op::Read { share, .. }
            | Op::BeginUpload { share, .. }
            | Op::CreateFolder { share, .. }
            | Op::Move { share, .. }
            | Op::Delete { share, .. }
            | Op::Watch { share, .. } => share.as_deref(),
            // A part belongs to an upload, which knows its root.
            Op::UploadPart { .. } | Op::FinishUpload { .. } | Op::AbortUpload { .. } => None,
        }
    }

    /// Whether the request changes something on the serving device.
    pub fn writes(&self) -> bool {
        match self {
            Op::Shares
            | Op::ListDir { .. }
            | Op::Stat { .. }
            | Op::Read { .. }
            | Op::Watch { .. } => false,
            Op::BeginUpload { .. }
            | Op::UploadPart { .. }
            | Op::FinishUpload { .. }
            | Op::AbortUpload { .. }
            | Op::CreateFolder { .. }
            | Op::Move { .. }
            | Op::Delete { .. } => true,
        }
    }
}

/// A share as the asking contact sees it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WireShare {
    pub id: String,
    pub name: String,
    pub item_kind: FileShareItemKind,
    pub level: FileShareLevel,
}

/// A place to start in on an own device: a drive or a known place, by its window path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WireRoot {
    pub name: String,
    pub path: String,
}

/// A file or a folder.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WireKind {
    File,
    Folder,
}

/// One entry of a listing or a stat.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WireEntry {
    pub name: String,
    pub kind: WireKind,
    /// Files only; 0 for a folder.
    pub size: u64,
    pub modified_ms: Option<i64>,
    /// Changes with the content (the version of FR-036).
    pub etag: String,
}

/// Why a request failed, without any text of the serving device (FR-042).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, thiserror::Error)]
pub enum FailCode {
    #[error("not found")]
    NotFound,
    /// Not allowed: no grant, a write at level read, holzi's own data, a path out of the root.
    #[error("refused")]
    Refused,
    #[error("conflict")]
    Conflict,
    #[error("too large")]
    TooLarge,
    /// The source at the serving device failed (a storage, a locked folder).
    #[error("source unavailable")]
    SourceUnavailable,
    #[error("busy")]
    Busy,
    /// The serving device does not offer this yet.
    #[error("unsupported")]
    Unsupported,
    /// The name is taken already.
    #[error("name taken")]
    NameTaken,
    #[error("internal")]
    Internal,
}

/// The answer to a [`Request`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Reply {
    Shares(Vec<WireShare>),
    /// The drives and known places of an own device.
    Roots {
        drives: Vec<WireRoot>,
        known: Vec<WireRoot>,
    },
    Entries(Vec<WireEntry>),
    Entry(WireEntry),
    /// The bytes of a read follow: `len` of them, from a file of `size` bytes.
    Reading {
        size: u64,
        len: u64,
    },
    Upload {
        upload_id: String,
    },
    /// The mark of an uploaded part.
    Mark(String),
    Done,
    Deleted {
        trashed: bool,
    },
    /// Something changed in the watched folder.
    Changed,
    Failed(FailCode),
}

/// Writes `value` as one head.
pub async fn write_head<W: AsyncWrite + Unpin, T: Serialize>(
    send: &mut W,
    value: &T,
) -> Result<(), WireError> {
    write_value(send, value, HEAD_LIMIT).await
}

/// Reads one head that must be there.
pub async fn read_head<R: AsyncRead + Unpin, T: serde::de::DeserializeOwned>(
    recv: &mut R,
) -> Result<T, WireError> {
    expect_value(recv, HEAD_LIMIT).await
}

#[cfg(test)]
#[path = "wire_tests.rs"]
mod tests;
