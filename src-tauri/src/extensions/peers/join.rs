//! Joining a lobby of another device with its code (spec 057 T052, FR-023, FR-025): read the
//! code, dial the host's endpoint with the code's relay, prove this device
//! ([`foreign_auth::dial`]), send `Join` with the proof of the code's secret, and wait for the
//! answer, all within [`JOIN_LIMIT`].
//!
//! The extension learns one of three things: the host was not reached (`network_unavailable`),
//! it was reached and refused the join without saying more (`lobby_closed`: an unreadable code,
//! an unknown or closed lobby, a wrong secret, another extension, a blocked vault, a throttled
//! device), or it refused this device after a valid proof (`not_admitted`). A code that cannot be
//! read never leaves the device.
//!
//! [`foreign_auth::dial`]: crate::sync::foreign_auth::dial

use std::sync::Arc;

use iroh::endpoint::{Connection, RecvStream, SendStream};
use serde_json::Value;
use tokio::time::{timeout_at, Instant};
use uuid::Uuid;

use super::admission::{refuse, PURPOSE};
use super::channel::{self, Channel, Opening, Origin};
use super::limits::{JOIN_LIMIT, PAYLOAD_BYTES};
use super::lobby::{valid_display_name, valid_lobby_id};
use super::state::Session;
use super::wire::{
    self, join_proof, JoinCode, JoinStatus, Message, ADMISSION_FRAME_LIMIT, VERSION,
};
use super::{ExtensionRef, PeerError, PeerState};
use crate::sync::foreign_auth;
use crate::sync::presence::endpoint_addr_of;
use crate::sync::wire::EXT_PEER_ALPN;

/// A join the host admitted: the channel and the host's `lobbyId`.
pub struct Joined {
    pub channel: Arc<Channel>,
    pub lobby_id: String,
}

/// Joins the lobby of `code` for `extension_id` (`extension_peers_join`), named on the wire as
/// `extension`, with a display name and a JSON payload for the host's extension.
pub async fn join(
    peers: &PeerState,
    extension_id: Uuid,
    extension: ExtensionRef,
    code: &str,
    display_name: &str,
    payload: &Value,
) -> Result<Joined, PeerError> {
    if !valid_display_name(display_name) {
        return Err(PeerError::InvalidInput);
    }
    let payload = serde_json::to_vec(payload).map_err(|_| PeerError::InvalidInput)?;
    if payload.len() > PAYLOAD_BYTES {
        return Err(PeerError::TooLarge {
            limit: PAYLOAD_BYTES,
        });
    }
    let code = JoinCode::parse(code).ok_or(PeerError::LobbyClosed)?;
    let session = peers.session().ok_or(PeerError::NetworkUnavailable)?;
    if peers.channels_full(extension_id) {
        return Err(PeerError::Limit {
            limit: super::limits::CHANNELS_PER_EXTENSION,
        });
    }
    let deadline = Instant::now() + JOIN_LIMIT;
    let addr = endpoint_addr_of(&code.endpoint_id, code.relay.as_deref(), &[])
        .map_err(|_| PeerError::LobbyClosed)?;
    let connection = match timeout_at(deadline, session.node.dial_alpn(addr, EXT_PEER_ALPN)).await {
        Ok(Ok(connection)) => connection,
        Ok(Err(error)) => {
            log::debug!("peers: the host of a lobby was not reached: {error}");
            return Err(PeerError::NetworkUnavailable);
        }
        Err(_) => return Err(PeerError::NetworkUnavailable),
    };
    let request = Request {
        extension,
        display_name: display_name.to_owned(),
        payload,
        code,
    };
    let asked = timeout_at(deadline, ask(&session, peers, &connection, request)).await;
    let (status, send, recv, remote) = match asked {
        Ok(Ok(answered)) => answered,
        Ok(Err(error)) => {
            refuse(&connection);
            return Err(error);
        }
        Err(_) => {
            refuse(&connection);
            return Err(PeerError::LobbyClosed);
        }
    };
    let lobby_id = match status {
        JoinStatus::Accepted { lobby_id } if valid_lobby_id(&lobby_id) => lobby_id,
        JoinStatus::NotAdmitted => {
            refuse(&connection);
            return Err(PeerError::NotAdmitted);
        }
        JoinStatus::Accepted { .. } | JoinStatus::Closed => {
            refuse(&connection);
            return Err(PeerError::LobbyClosed);
        }
    };
    let opening = Opening {
        extension_id,
        remote,
        origin: Origin::Joined {
            lobby_id: lobby_id.clone(),
        },
        connection,
        send,
        recv,
    };
    let channel = channel::open(opening, &session.ctx.frames, &session.node.tasks().tracker)?;
    Ok(Joined { channel, lobby_id })
}

/// What goes into the `Join`.
struct Request {
    extension: ExtensionRef,
    display_name: String,
    payload: Vec<u8>,
    code: JoinCode,
}

/// The exchange, the `Join` and the host's answer on a connection to the host. Anything that
/// ends it early is the host's refusal as the extension sees it: `lobby_closed`.
async fn ask(
    session: &Session,
    peers: &PeerState,
    connection: &Connection,
    request: Request,
) -> Result<(JoinStatus, SendStream, RecvStream, foreign_auth::Verified), PeerError> {
    let closed = |_| PeerError::LobbyClosed;
    let (mut send, mut recv) = connection.open_bi().await.map_err(closed)?;
    let remote = *connection.remote_id().as_bytes();
    let local = session.ctx.local(peers.seen());
    let verified = foreign_auth::dial(&mut send, &mut recv, &local, PURPOSE, remote)
        .await
        .map_err(|error| {
            log::debug!("peers: the exchange with a lobby's host ended: {error}");
            PeerError::LobbyClosed
        })?;
    let Request {
        extension,
        display_name,
        payload,
        code,
    } = request;
    let join = Message::Join {
        v: VERSION,
        lobby: code.lobby,
        proof: join_proof(&code.secret, &verified.nonces, &code.lobby),
        extension,
        display_name,
        payload,
    };
    wire::write(&mut send, &join, ADMISSION_FRAME_LIMIT)
        .await
        .map_err(|_| PeerError::LobbyClosed)?;
    match wire::read(&mut recv, ADMISSION_FRAME_LIMIT).await {
        Ok(Some(Message::JoinReply { status, .. })) => Ok((status, send, recv, verified)),
        _ => Err(PeerError::LobbyClosed),
    }
}

#[cfg(test)]
#[path = "join_tests.rs"]
mod tests;
