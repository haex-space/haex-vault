//! The messages of `haex-vault-ext-peer/1` after the exchange of [`crate::sync::foreign_auth`]
//! (contracts/wire-protocol.md): admission, invitation and the channel's reliable messages, on the
//! first stream of the connection, framed as `u32 BE length ‖ postcard` like the sync
//! ([`crate::sync::wire`]). Every message carries `v`; another version ends the connection.
//!
//! The join code of a lobby and the proof of its secret live here too: both are part of the
//! format, not of the lobby's state.

use std::fmt;

use data_encoding::BASE32_NOPAD;
use hmac::{Hmac, KeyInit, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use tokio::io::{AsyncRead, AsyncWrite};
use zeroize::Zeroizing;

use super::limits::{PAYLOAD_BYTES, RELIABLE_BYTES};
use super::ExtensionRef;
use crate::sync::foreign_auth::Nonces;
use crate::sync::signing::lp;
use crate::sync::wire::{read_value, write_value, WireError, HANDSHAKE_FRAME_LIMIT};

/// The version of every message below.
pub const VERSION: u8 = 1;
/// Largest frame before admission: as large as a frame of the exchange before it.
pub const ADMISSION_FRAME_LIMIT: usize = HANDSHAKE_FRAME_LIMIT;
/// Largest frame of an open channel: a reliable message with its head.
pub const CHANNEL_FRAME_LIMIT: usize = RELIABLE_BYTES + 64;
/// Largest admission frame of a device that is not verified: one `Join` with its payload, display
/// name and extension, never more (FR-031).
pub const JOIN_FRAME_LIMIT: usize = PAYLOAD_BYTES + 1024;

/// Every message of `haex-vault-ext-peer/1`. The order is part of the format.
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum JoinStatus {
    /// Admitted: the connection is the channel now. The host names its extension's `lobbyId`,
    /// which the joining extension gets as the result of its join (contracts/bridge.md).
    Accepted {
        lobby_id: String,
    },
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

/// The text every join code starts with (spec 054's naming scheme).
pub const JOIN_CODE_PREFIX: &str = "haex-vault:join:";
/// Longest text read as a join code; anything longer is not one, and is not decoded.
const JOIN_CODE_MAX_CHARS: usize = 512;

/// What a join code carries (research R6): the host's endpoint and relay to reach it, holzi's
/// random key of the lobby and its secret; no name, no vault and no contact (FR-021). The text
/// is `haex-vault:join:` + Base32 without padding of the postcard bytes. Never printed by
/// `Debug`, never logged.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JoinCode {
    pub v: u8,
    pub endpoint_id: [u8; 32],
    pub relay: Option<String>,
    pub lobby: [u8; 16],
    pub secret: Zeroizing<[u8; 16]>,
}

impl fmt::Debug for JoinCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("JoinCode(..)")
    }
}

impl JoinCode {
    /// The code as the extension hands it on.
    pub fn text(&self) -> String {
        // Fixed-size fields, an option and a string: postcard has nothing that could fail.
        let bytes = Zeroizing::new(postcard::to_stdvec(self).expect("a join code serializes"));
        format!("{JOIN_CODE_PREFIX}{}", BASE32_NOPAD.encode(&bytes))
    }

    /// The code in `text`, surrounding white space ignored; `None` for anything that is not a
    /// code of this version, which the joining side answers like a closed lobby without dialing.
    pub fn parse(text: &str) -> Option<Self> {
        let body = text.trim().strip_prefix(JOIN_CODE_PREFIX)?;
        if body.len() > JOIN_CODE_MAX_CHARS {
            return None;
        }
        let bytes = Zeroizing::new(BASE32_NOPAD.decode(body.as_bytes()).ok()?);
        let (code, rest) = postcard::take_from_bytes::<Self>(&bytes).ok()?;
        (rest.is_empty() && code.v == VERSION).then_some(code)
    }
}

/// The proof of a join: `HMAC-SHA256(secret, lp(nonce_dial) ‖ lp(nonce_accept) ‖ lp(lobby))`
/// over the nonces of this connection's exchange, so a recorded `Join` is worth nothing on
/// another connection and the secret itself never travels (research R6).
pub fn join_proof(secret: &[u8; 16], nonces: &Nonces, lobby: &[u8; 16]) -> [u8; 32] {
    join_mac(secret, nonces, lobby)
        .finalize()
        .into_bytes()
        .into()
}

/// Whether `proof` is the proof of `secret` for this exchange and `lobby`, in constant time.
pub fn join_proof_holds(
    secret: &[u8; 16],
    nonces: &Nonces,
    lobby: &[u8; 16],
    proof: &[u8; 32],
) -> bool {
    join_mac(secret, nonces, lobby).verify_slice(proof).is_ok()
}

fn join_mac(secret: &[u8; 16], nonces: &Nonces, lobby: &[u8; 16]) -> Hmac<Sha256> {
    let mut mac = Hmac::<Sha256>::new_from_slice(secret).expect("HMAC takes any key length");
    mac.update(&lp(&nonces.dial));
    mac.update(&lp(&nonces.accept));
    mac.update(&lp(lobby));
    mac
}

#[cfg(test)]
#[path = "wire_tests.rs"]
mod tests;
