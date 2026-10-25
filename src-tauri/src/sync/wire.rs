//! Frames and messages of `holzi-sync/1` (contracts/sync-protocol.md,
//! research R6).
//!
//! A frame is `u32 BE length ‖ postcard(message)`. The length is checked
//! against the limit before any buffer is allocated: 64 KiB before
//! `Accept`, 4 MiB after it. A frame that is too large, or a message this
//! version does not know, ends the connection with an [`ErrorCode`].

use serde::{Deserialize, Serialize};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

use crate::sync::change::Page;
use crate::sync::progress::Vector;

/// ALPN of the sync protocol; the version is part of it (research R14).
pub const SYNC_ALPN: &[u8] = b"holzi-sync/1";
/// ALPN of linking a new installation (user story 5).
pub const LINK_ALPN: &[u8] = b"holzi-link/1";
/// ALPN of channels between extensions on devices of different vaults (spec 057 research R1).
pub const EXT_PEER_ALPN: &[u8] = b"haex-vault-ext-peer/1";
/// ALPN of file access between own devices and file shares with contacts (spec 056 research R5).
pub const FILES_ALPN: &[u8] = b"haex-vault-files/1";

/// Largest frame before the handshake accepted the peer.
pub const HANDSHAKE_FRAME_LIMIT: usize = 64 * 1024;
/// Largest frame after the handshake.
pub const FRAME_LIMIT: usize = 4 * 1024 * 1024;

/// Version of the messages below, sent in `Challenge` and `Response`. It is the first field, so
/// the byte after the variant tag of both frames, and a side reads it before it decodes anything
/// else of a `Challenge` (spec 059 research R2): version 2 added `Challenge::schema`.
pub const PROTOCOL_VERSION: u8 = 2;

/// What the handshake compares; devices sync only with equal versions
/// (FR-029).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SchemaVersion {
    pub protocol: u32,
    pub holzi_migration: u32,
    pub crdt_trigger: u32,
}

/// The device list a side holds as effective.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListRef {
    pub generation: u64,
    pub list_hash: [u8; 32],
}

/// A Schnorr signature; serde has no impl for 64-byte arrays.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Signature(pub [u8; 64]);

impl std::fmt::Debug for Signature {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Signature(..)")
    }
}

impl Serialize for Signature {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_bytes(&self.0)
    }
}

impl<'de> Deserialize<'de> for Signature {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let bytes: Vec<u8> = serde::Deserialize::deserialize(deserializer)?;
        let bytes: [u8; 64] = bytes
            .try_into()
            .map_err(|_| serde::de::Error::invalid_length(64, &"64 bytes"))?;
        Ok(Self(bytes))
    }
}

/// Why the accepting side refuses a peer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RejectCode {
    ForeignVault,
    NotOnList,
    Removed,
    BadSignature,
    Incompatible,
    Duplicate,
    Closing,
}

/// Why a `Pull` is answered with `Resync` (research R20, user story 3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResyncReason {
    TombstonesExpired,
}

/// Every message of `holzi-sync/1`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Message {
    Challenge {
        v: u8,
        nonce_a: [u8; 32],
        endpoint_a: [u8; 32],
        /// The accepting side's effective list, so the dialing side can
        /// push a better one before its `Response`.
        list: ListRef,
        /// The accepting side's version state, named before either side decides, so the dialing
        /// side knows it also when it refuses (spec 059 FR-005). Last field: an older reader
        /// stops before it.
        schema: SchemaVersion,
    },
    Response {
        v: u8,
        device_d: [u8; 32],
        vault: [u8; 32],
        nonce_d: [u8; 32],
        schema: SchemaVersion,
        list: ListRef,
        sig_d: Signature,
    },
    Accept {
        device_a: [u8; 32],
        schema: SchemaVersion,
        list: ListRef,
        sig_a: Signature,
    },
    Reject {
        code: RejectCode,
    },
    DeviceListPush {
        payload: Vec<u8>,
        signature: Signature,
    },
    Progress {
        vector: Vector,
        /// When this device last saw each device, in epoch milliseconds.
        last_seen: Vec<([u8; 32], u64)>,
    },
    Pull {
        vector: Vector,
        replace: bool,
    },
    Page(Page),
    Resync {
        reason: ResyncReason,
    },
}

/// Application error codes a connection closes with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum ErrorCode {
    /// Session end or an orderly close.
    Closed = 0,
    /// A frame over its limit.
    FrameTooLarge = 1,
    /// A frame that does not decode, or a message out of place.
    Protocol = 2,
    /// The handshake refused the peer.
    Rejected = 3,
    /// The pull failed on the receiving side.
    PullFailed = 4,
}

impl ErrorCode {
    pub fn as_u32(self) -> u32 {
        self as u32
    }
}

/// Why reading or writing a frame failed.
#[derive(Debug, thiserror::Error)]
pub enum WireError {
    #[error("a frame of {len} bytes exceeds the limit of {limit} bytes")]
    FrameTooLarge { len: usize, limit: usize },
    #[error("a frame does not encode or decode: {0}")]
    Codec(postcard::Error),
    #[error("the stream ended")]
    Closed,
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

impl WireError {
    /// The code to close the connection with.
    pub fn code(&self) -> ErrorCode {
        match self {
            WireError::FrameTooLarge { .. } => ErrorCode::FrameTooLarge,
            WireError::Codec(_) => ErrorCode::Protocol,
            WireError::Closed | WireError::Io(_) => ErrorCode::Closed,
        }
    }
}

/// Writes `message` as one frame. A message over `limit` is not sent.
pub async fn write_frame<W: AsyncWrite + Unpin>(
    writer: &mut W,
    message: &Message,
    limit: usize,
) -> Result<(), WireError> {
    write_value(writer, message, limit).await
}

/// [`write_frame`] for any message type, as the link protocol has its own.
pub async fn write_value<W: AsyncWrite + Unpin, T: Serialize>(
    writer: &mut W,
    message: &T,
    limit: usize,
) -> Result<(), WireError> {
    let bytes = postcard::to_stdvec(message).map_err(WireError::Codec)?;
    if bytes.len() > limit {
        return Err(WireError::FrameTooLarge {
            len: bytes.len(),
            limit,
        });
    }
    let len = u32::try_from(bytes.len()).map_err(|_| WireError::FrameTooLarge {
        len: bytes.len(),
        limit,
    })?;
    writer.write_all(&len.to_be_bytes()).await?;
    writer.write_all(&bytes).await?;
    Ok(())
}

/// Reads one frame of at most `limit` bytes. `Ok(None)` when the stream
/// ended cleanly before a frame started.
pub async fn read_frame<R: AsyncRead + Unpin>(
    reader: &mut R,
    limit: usize,
) -> Result<Option<Message>, WireError> {
    read_value(reader, limit).await
}

/// [`read_frame`] for any message type.
pub async fn read_value<R: AsyncRead + Unpin, T: serde::de::DeserializeOwned>(
    reader: &mut R,
    limit: usize,
) -> Result<Option<T>, WireError> {
    let Some(bytes) = read_raw(reader, limit).await? else {
        return Ok(None);
    };
    postcard::from_bytes(&bytes)
        .map(Some)
        .map_err(WireError::Codec)
}

/// Reads one frame of at most `limit` bytes without decoding it. `Ok(None)` when the stream
/// ended cleanly before a frame started.
pub async fn read_raw<R: AsyncRead + Unpin>(
    reader: &mut R,
    limit: usize,
) -> Result<Option<Vec<u8>>, WireError> {
    let mut len = [0u8; 4];
    match reader.read_exact(&mut len).await {
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(e) => return Err(e.into()),
    }
    let len = u32::from_be_bytes(len) as usize;
    if len > limit {
        return Err(WireError::FrameTooLarge { len, limit });
    }
    let mut bytes = vec![0u8; len];
    reader.read_exact(&mut bytes).await.map_err(|e| {
        if e.kind() == std::io::ErrorKind::UnexpectedEof {
            WireError::Closed
        } else {
            e.into()
        }
    })?;
    Ok(Some(bytes))
}

/// [`expect_frame`] for any message type.
pub async fn expect_value<R: AsyncRead + Unpin, T: serde::de::DeserializeOwned>(
    reader: &mut R,
    limit: usize,
) -> Result<T, WireError> {
    read_value(reader, limit).await?.ok_or(WireError::Closed)
}

/// Reads one frame that must be there.
pub async fn expect_frame<R: AsyncRead + Unpin>(
    reader: &mut R,
    limit: usize,
) -> Result<Message, WireError> {
    read_frame(reader, limit).await?.ok_or(WireError::Closed)
}

/// Why reading the accepting side's first message failed.
#[derive(Debug, thiserror::Error)]
pub enum ChallengeError {
    /// The frame speaks another protocol version (its first byte). Nothing else of it is read:
    /// an older `Challenge` has no `schema` and would not decode.
    #[error("the accepting side speaks protocol version {0}")]
    Version(u8),
    #[error(transparent)]
    Wire(#[from] WireError),
}

/// The postcard tag of [`Message::Challenge`]: the index of the first variant. A test pins it.
const CHALLENGE_TAG: u8 = 0;

/// Reads the `Challenge` of the accepting side: the version byte (right after the variant tag)
/// is checked before the frame is decoded, so a device of another protocol version ends with
/// [`ChallengeError::Version`] instead of a decode error (spec 059 research R2).
pub async fn expect_challenge<R: AsyncRead + Unpin>(
    reader: &mut R,
    limit: usize,
) -> Result<Message, ChallengeError> {
    let bytes = read_raw(reader, limit).await?.ok_or(WireError::Closed)?;
    match (bytes.first(), bytes.get(1)) {
        (Some(&CHALLENGE_TAG), Some(&v)) if v != PROTOCOL_VERSION => {
            Err(ChallengeError::Version(v))
        }
        _ => postcard::from_bytes(&bytes).map_err(|e| ChallengeError::Wire(WireError::Codec(e))),
    }
}

#[cfg(test)]
#[path = "wire_tests.rs"]
mod tests;
