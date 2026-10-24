//! One channel: a connection whose datagrams carry the extension's fast messages and whose first
//! stream its reliable ones (research R2, R3), and how it ends (R11, FR-039, FR-040).
//!
//! Three tasks on the session's tracker serve a channel: one writes the reliable messages and, when
//! the channel ends here, `Close` before closing the connection; one reads the stream; one reads
//! the datagrams and notices the end of the connection. Whoever ends a channel first removes it
//! from [`PeerState`](super::PeerState); only that one tells the frames, so each side hears of the
//! end exactly once. No message content is ever logged (FR-042).

use std::sync::{Arc, Weak};
use std::time::Duration;

use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use iroh::endpoint::{Connection, ConnectionError, RecvStream, SendDatagramError, SendStream};
use serde_json::{json, Value};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;
use uuid::Uuid;

use super::limits::{FAST_BYTES, RELIABLE_BYTES};
use super::wire::{self, CloseReason, Message, CHANNEL_FRAME_LIMIT, VERSION};
use super::PeerError;
use crate::extensions::bridge::dispatch::Emit;
use crate::extensions::bridge::events::emit_to_frames;
use crate::extensions::host::ExtensionHost;
use crate::sync::foreign_auth::Verified;
use crate::sync::wire::ErrorCode;

/// A message of the other side, for the frames of the channel's extension.
pub const MESSAGE: &str = "peers:message";
/// The end of a channel, for the frames of its extension.
pub const CHANNEL_CLOSED: &str = "peers:channel-closed";

/// Reliable messages waiting for the stream at once, per channel; one more is refused with
/// `limit`, so an extension cannot pile up memory behind a side that does not read.
pub const RELIABLE_QUEUE: usize = 16;
/// How long `Close` may take to go out before the connection closes anyway.
const CLOSE_GRACE: Duration = Duration::from_millis(500);

/// Why a channel ended, as the frames hear it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EndReason {
    Closed,
    Lost,
    Timeout,
}

impl EndReason {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Closed => "closed",
            Self::Lost => "lost",
            Self::Timeout => "timeout",
        }
    }
}

/// The reason for a connection that ended with `error`: an orderly close by either side is
/// `closed`, the idle time running out `timeout`, anything else `lost`.
pub fn end_reason(error: &ConnectionError) -> EndReason {
    match error {
        ConnectionError::ApplicationClosed(_) | ConnectionError::LocallyClosed => EndReason::Closed,
        ConnectionError::TimedOut => EndReason::Timeout,
        _ => EndReason::Lost,
    }
}

/// How a channel came about.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Origin {
    Invitation {
        invite: Uuid,
    },
    /// A device joined a lobby this device hosts; `key` is holzi's key of that lobby.
    Lobby {
        lobby_id: String,
        key: [u8; 16],
    },
    /// This device joined a lobby of another one.
    Joined {
        lobby_id: String,
    },
    /// Opened by the test support without invitation or lobby.
    #[cfg(test)]
    Test,
}

/// Where the events of channels go: the frames of one extension, through holzi's window. Holds
/// the host weakly, since the host holds the channels.
#[derive(Clone)]
pub struct Frames {
    emitter: Arc<dyn Emit>,
    host: Weak<ExtensionHost>,
}

impl Frames {
    pub fn new(emitter: Arc<dyn Emit>, host: &Arc<ExtensionHost>) -> Self {
        Self {
            emitter,
            host: Arc::downgrade(host),
        }
    }

    pub(super) fn host(&self) -> Option<Arc<ExtensionHost>> {
        self.host.upgrade()
    }

    /// Sends `event` to holzi's own window, where the invitation prompt lives. Never carries a
    /// `peers:*` message, which belongs to the frames of one extension only (FR-005).
    pub(super) fn emit_window(&self, event: &str, payload: Value) {
        self.emitter.emit(event, payload);
    }

    /// Sends `event` to the open frames of `extension_id` and to no other (FR-005).
    pub(super) fn emit(&self, extension_id: Uuid, event: &str, data: &Value) {
        if let Some(host) = self.host() {
            emit_to_frames(self.emitter.as_ref(), &host, extension_id, event, data);
        }
    }
}

/// An admitted connection, ready to become a channel.
pub struct Opening {
    pub extension_id: Uuid,
    pub remote: Verified,
    pub origin: Origin,
    pub connection: Connection,
    /// The first stream, after the exchange and the admission.
    pub send: SendStream,
    pub recv: RecvStream,
}

pub struct Channel {
    pub id: Uuid,
    pub extension_id: Uuid,
    pub remote: Verified,
    pub origin: Origin,
    connection: Connection,
    reliable: mpsc::Sender<Vec<u8>>,
    /// Fires when this side ends the channel: `Close`, then the connection closes.
    closing: CancellationToken,
    frames: Frames,
}

/// Registers `opening` as a channel of its extension and starts it on `tracker`. Refused, and the
/// connection closed, when the extension has [`super::limits::CHANNELS_PER_EXTENSION`] channels
/// already or the connection cannot carry a fast message of [`FAST_BYTES`].
pub fn open(
    opening: Opening,
    frames: &Frames,
    tracker: &TaskTracker,
) -> Result<Arc<Channel>, PeerError> {
    open_announced(opening, frames, tracker, |_| {})
}

/// [`open`], calling `announce` once the channel is registered and before it delivers anything,
/// so the event that names a channel reaches the frames before its first message.
pub fn open_announced(
    opening: Opening,
    frames: &Frames,
    tracker: &TaskTracker,
    announce: impl FnOnce(&Channel),
) -> Result<Arc<Channel>, PeerError> {
    let Opening {
        extension_id,
        remote,
        origin,
        connection,
        send,
        recv,
    } = opening;
    let refuse = |connection: &Connection, error: PeerError| {
        connection.close(ErrorCode::Closed.as_u32().into(), b"closed");
        Err(error)
    };
    let Some(host) = frames.host() else {
        return refuse(&connection, PeerError::NetworkUnavailable);
    };
    if connection
        .max_datagram_size()
        .is_none_or(|size| size < FAST_BYTES)
    {
        return refuse(&connection, PeerError::NetworkUnavailable);
    }
    let (reliable, outgoing) = mpsc::channel(RELIABLE_QUEUE);
    let channel = Arc::new(Channel {
        id: Uuid::new_v4(),
        extension_id,
        remote,
        origin,
        connection,
        reliable,
        closing: CancellationToken::new(),
        frames: frames.clone(),
    });
    if let Err(error) = host.peers.add(Arc::clone(&channel)) {
        return refuse(&channel.connection, error);
    }
    announce(&channel);
    tracker.spawn(write_loop(Arc::clone(&channel), send, outgoing));
    tracker.spawn(read_loop(Arc::clone(&channel), recv));
    tracker.spawn(receive_datagrams(Arc::clone(&channel)));
    Ok(channel)
}

impl Channel {
    /// Sends a fast message: never waits, and when the send buffer is full the oldest unsent
    /// datagram goes (FR-036, FR-038).
    pub fn send_fast(&self, data: &[u8]) -> Result<(), PeerError> {
        if data.len() > FAST_BYTES {
            return Err(PeerError::TooLarge { limit: FAST_BYTES });
        }
        if self.is_closed() {
            return Err(PeerError::ChannelClosed);
        }
        self.connection
            .send_datagram(data.to_vec().into())
            .map_err(|error| match error {
                SendDatagramError::TooLarge => PeerError::TooLarge { limit: FAST_BYTES },
                _ => PeerError::ChannelClosed,
            })
    }

    /// Queues a reliable message; it arrives complete and in order.
    pub fn send_reliable(&self, data: Vec<u8>) -> Result<(), PeerError> {
        if data.len() > RELIABLE_BYTES {
            return Err(PeerError::TooLarge {
                limit: RELIABLE_BYTES,
            });
        }
        if self.is_closed() {
            return Err(PeerError::ChannelClosed);
        }
        self.reliable.try_send(data).map_err(|error| match error {
            mpsc::error::TrySendError::Full(_) => PeerError::Limit {
                limit: RELIABLE_QUEUE,
            },
            mpsc::error::TrySendError::Closed(_) => PeerError::ChannelClosed,
        })
    }

    /// Whether the channel has ended here or its connection has.
    pub fn is_closed(&self) -> bool {
        self.closing.is_cancelled() || self.connection.close_reason().is_some()
    }

    /// The largest datagram the connection can carry now, for the tests of research R2.
    #[cfg(test)]
    pub(crate) fn max_datagram_size(&self) -> Option<usize> {
        self.connection.max_datagram_size()
    }

    /// Ends this channel through its extension's state, so the end is told once.
    fn end(&self, reason: EndReason) {
        match self.frames.host() {
            Some(host) => host.peers.finish(self.id, reason),
            None => self.shut(),
        }
    }

    /// Stops the channel here: the writer sends `Close` and closes the connection.
    pub(super) fn shut(&self) {
        self.closing.cancel();
    }

    /// Tells the frames of the channel's extension that it ended.
    pub(super) fn tell_closed(&self, reason: EndReason) {
        self.frames.emit(
            self.extension_id,
            CHANNEL_CLOSED,
            &json!({ "channelId": self.id.to_string(), "reason": reason.as_str() }),
        );
    }

    /// ponytail: one event per message, fast ones included (research R8). Upgrade: bundle the
    /// datagrams of 5 ms into one event if the added delay misses SC-004.
    fn deliver(&self, data: &[u8], reliable: bool) {
        let message = json!({
            "channelId": self.id.to_string(),
            "data": STANDARD.encode(data),
            "reliable": reliable,
        });
        // A channel whose extension has no frame yet keeps its messages until there is one.
        let message = match self.frames.host() {
            Some(host) => host.peers.hold(self.id, MESSAGE, message, reliable),
            None => Some(message),
        };
        if let Some(message) = message {
            self.frames.emit(self.extension_id, MESSAGE, &message);
        }
    }
}

/// Writes queued reliable messages until the channel ends here, then `Close` and the close of
/// the connection.
async fn write_loop(
    channel: Arc<Channel>,
    mut send: SendStream,
    mut outgoing: mpsc::Receiver<Vec<u8>>,
) {
    loop {
        let data = tokio::select! {
            biased;
            () = channel.closing.cancelled() => break,
            next = outgoing.recv() => match next {
                Some(data) => data,
                None => break,
            },
        };
        let message = Message::Reliable { v: VERSION, data };
        tokio::select! {
            biased;
            () = channel.closing.cancelled() => break,
            written = wire::write(&mut send, &message, CHANNEL_FRAME_LIMIT) => {
                if written.is_err() {
                    // The connection is going; the datagram task tells why.
                    return;
                }
            }
        }
    }
    if channel.connection.close_reason().is_none() {
        let close = Message::Close {
            v: VERSION,
            reason: CloseReason::Closed,
        };
        let _ = tokio::time::timeout(
            CLOSE_GRACE,
            wire::write(&mut send, &close, CHANNEL_FRAME_LIMIT),
        )
        .await;
        channel
            .connection
            .close(ErrorCode::Closed.as_u32().into(), b"closed");
    }
}

/// Reads the reliable messages of the other side and its `Close`.
async fn read_loop(channel: Arc<Channel>, mut recv: RecvStream) {
    loop {
        let read = tokio::select! {
            biased;
            () = channel.closing.cancelled() => return,
            read = wire::read(&mut recv, CHANNEL_FRAME_LIMIT) => read,
        };
        match read {
            Ok(Some(Message::Reliable { data, .. })) if data.len() <= RELIABLE_BYTES => {
                channel.deliver(&data, true)
            }
            Ok(Some(Message::Reliable { .. })) => {
                log::debug!(
                    "peers: channel {} sent an oversized reliable message",
                    channel.id
                );
                channel.end(EndReason::Lost);
                channel
                    .connection
                    .close(ErrorCode::Protocol.as_u32().into(), b"protocol");
                return;
            }
            Ok(Some(Message::Close { .. })) => return channel.end(EndReason::Closed),
            // The stream ended; the end of the connection follows and is told by its reason.
            Ok(None) => return,
            Ok(Some(_)) | Err(_) if channel.connection.close_reason().is_some() => return,
            Ok(Some(_)) | Err(_) => {
                log::debug!("peers: channel {} broke the protocol", channel.id);
                channel.end(EndReason::Lost);
                channel
                    .connection
                    .close(ErrorCode::Protocol.as_u32().into(), b"protocol");
                return;
            }
        }
    }
}

/// Reads the other side's fast messages until the connection ends, then ends the channel with
/// the reason the connection gives.
async fn receive_datagrams(channel: Arc<Channel>) {
    loop {
        match channel.connection.read_datagram().await {
            Ok(data) if data.len() <= FAST_BYTES => channel.deliver(&data, false),
            // Larger than any side of this protocol sends: dropped like a lost one.
            Ok(_) => {}
            Err(error) => return channel.end(end_reason(&error)),
        }
    }
}

#[cfg(test)]
#[path = "channel_tests.rs"]
mod tests;
