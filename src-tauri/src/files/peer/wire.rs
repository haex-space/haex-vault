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
}

impl Op {
    /// The root this request is about, if any.
    pub fn share(&self) -> Option<&str> {
        match self {
            Op::Shares => None,
            Op::ListDir { share, .. } | Op::Stat { share, .. } | Op::Read { share, .. } => {
                share.as_deref()
            }
        }
    }

    /// Whether the request changes something on the serving device.
    pub fn writes(&self) -> bool {
        match self {
            Op::Shares | Op::ListDir { .. } | Op::Stat { .. } | Op::Read { .. } => false,
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
    #[error("internal")]
    Internal,
}

/// The answer to a [`Request`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Reply {
    Shares(Vec<WireShare>),
    Entries(Vec<WireEntry>),
    Entry(WireEntry),
    /// The bytes of a read follow: `len` of them, from a file of `size` bytes.
    Reading {
        size: u64,
        len: u64,
    },
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
