//! The messages of `holzi-ext-peer/1` after the exchange of [`crate::sync::foreign_auth`]
//! (contracts/wire-protocol.md): admission, invitation and the channel's reliable messages, on the
//! first stream of the connection, framed as `u32 BE length ‖ postcard` like the sync
//! ([`crate::sync::wire`]). Every message carries `v`; another version ends the connection.

use serde::{Deserialize, Serialize};
use tokio::io::{AsyncRead, AsyncWrite};

use super::limits::RELIABLE_BYTES;
use super::ExtensionRef;
use crate::sync::wire::{read_value, write_value, WireError, HANDSHAKE_FRAME_LIMIT};

/// The version of every message below.
pub const VERSION: u8 = 1;
/// Largest frame before admission: as large as a frame of the exchange before it.
pub const ADMISSION_FRAME_LIMIT: usize = HANDSHAKE_FRAME_LIMIT;
/// Largest frame of an open channel: a reliable message with its head.
pub const CHANNEL_FRAME_LIMIT: usize = RELIABLE_BYTES + 64;

/// Every message of `holzi-ext-peer/1`. The order is part of the format.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Message {
    Invite {
        v: u8,
        invite: [u8; 16],
        extension: ExtensionRef,
        /// JSON, at most [`super::limits::PAYLOAD_BYTES`].
        payload: Vec<u8>,
        /// Unix milliseconds.
        expires_at: u64,
    },
    Join {
        v: u8,
        lobby: [u8; 16],
        proof: [u8; 32],
        extension: ExtensionRef,
        display_name: String,
        payload: Vec<u8>,
    },
    JoinReply {
        v: u8,
        status: JoinStatus,
    },
    InviteReply {
        v: u8,
        invite: [u8; 16],
        status: InviteStatus,
    },
    InviteConfirm {
        v: u8,
        invite: [u8; 16],
    },
    Withdraw {
        v: u8,
        invite: [u8; 16],
    },
    Reliable {
        v: u8,
        data: Vec<u8>,
    },
    Close {
        v: u8,
        reason: CloseReason,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum JoinStatus {
    Accepted,
    Closed,
    NotAdmitted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum InviteStatus {
    Accepted,
    Declined,
    NotInstalled,
}

/// Why a side ends a channel on purpose; anything else is read from how the connection ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CloseReason {
    Closed,
}

impl Message {
    fn version(&self) -> u8 {
        match self {
            Self::Invite { v, .. }
            | Self::Join { v, .. }
            | Self::JoinReply { v, .. }
            | Self::InviteReply { v, .. }
            | Self::InviteConfirm { v, .. }
            | Self::Withdraw { v, .. }
            | Self::Reliable { v, .. }
            | Self::Close { v, .. } => *v,
        }
    }
}

/// Why a message could not be read.
#[derive(Debug, thiserror::Error)]
pub enum MessageError {
    #[error(transparent)]
    Wire(#[from] WireError),
    #[error("a message of version {0}, this side speaks {VERSION}")]
    Version(u8),
}

/// Reads one message of at most `limit` bytes; `Ok(None)` when the stream ended before it.
pub async fn read<R: AsyncRead + Unpin>(
    recv: &mut R,
    limit: usize,
) -> Result<Option<Message>, MessageError> {
    let Some(message) = read_value::<_, Message>(recv, limit).await? else {
        return Ok(None);
    };
    match message.version() {
        VERSION => Ok(Some(message)),
        other => Err(MessageError::Version(other)),
    }
}

/// Writes one message; one over `limit` is not sent.
pub async fn write<W: AsyncWrite + Unpin>(
    send: &mut W,
    message: &Message,
    limit: usize,
) -> Result<(), WireError> {
    write_value(send, message, limit).await
}

#[cfg(test)]
#[path = "wire_tests.rs"]
mod tests;
