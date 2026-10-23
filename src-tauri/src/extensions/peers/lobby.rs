//! Lobbies on the host (spec 057 US4, US5; research R6, R7; FR-020–FR-028): an extension opens
//! a lobby and gets a join code; a device that presents the code's proof joins and becomes a
//! channel; closing the lobby kills the code and keeps the channels; removing a joined device
//! closes its channel and bans its endpoint and, when verified, its vault from the lobby.
//!
//! Who may join is decided in one place, [`Lobbies::decide`], on one path for every refusal
//! (FR-025): an unknown or closed lobby, a wrong proof and another extension all compute the
//! same HMAC (a fixed stand-in secret for an unknown lobby) and end in the same `Closed`.
//! The joining side lives in [`super::join`].

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, Instant};

use iroh::endpoint::{Connection, RecvStream, SendStream};
use serde_json::{json, Value};
use tokio_util::task::TaskTracker;
use uuid::Uuid;
use zeroize::Zeroizing;

use super::admission::{refuse, PeerContext};
use super::channel::{self, Channel, Opening, Origin};
use super::limits::{DISPLAY_NAME_CHARS, LOBBIES_PER_EXTENSION, PAYLOAD_BYTES};
use super::throttle::Throttle;
use super::wire::{
    self, join_proof_holds, JoinCode, JoinStatus, Message, ADMISSION_FRAME_LIMIT, VERSION,
};
use super::{contact_in_scope, ExtensionRef, PeerError, PeerState};
use crate::extensions::host::ExtensionHost;
use crate::sync::foreign_auth::{Trust, Verified};
use crate::sync::keys::random_bytes;

/// A device joined a lobby, for the frames of the host's extension.
pub const JOINED: &str = "peers:joined";
/// The secret an unknown lobby is checked against, so its refusal takes the path of a wrong one.
const STAND_IN_SECRET: [u8; 16] = [0; 16];
/// How long a refused joiner has to read the answer before the host closes the connection.
const REPLY_GRACE: Duration = Duration::from_secs(2);

/// Who may join a lobby.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Visibility {
    /// Own devices and contacts (the default).
    Contacts,
    /// Anyone with the code; devices of vaults that are no contact join as not verified
    /// (ADR 0016).
    Public,
}

/// An open lobby (data-model.md). Its `secret` leaves the device only in the code.
struct Lobby {
    lobby_id: String,
    extension_id: Uuid,
    extension: ExtensionRef,
    visibility: Visibility,
    secret: Zeroizing<[u8; 16]>,
    banned_endpoints: HashSet<[u8; 32]>,
    banned_vaults: HashSet<[u8; 32]>,
}

/// Which lobbies are open on this device, as the admission needs to know before a `Join`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Openness {
    None,
    ContactsOnly,
    Public,
}

/// The answer to a join.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    Accepted {
        key: [u8; 16],
        lobby_id: String,
        extension_id: Uuid,
    },
    Closed,
    NotAdmitted,
}

/// A `Join` as it came over the wire.
pub struct JoinRequest {
    pub lobby: [u8; 16],
    pub proof: [u8; 32],
    pub extension: ExtensionRef,
    pub display_name: String,
    pub payload: Vec<u8>,
}

/// The open lobbies of this device, by holzi's random key.
#[derive(Default)]
pub struct Lobbies(HashMap<[u8; 16], Lobby>);

impl Lobbies {
    /// Opens `lobby_id` for `extension_id`: its key and secret. A malformed id, or one of an
    /// open lobby of the same extension, is `invalid_input`; one more than
    /// [`LOBBIES_PER_EXTENSION`] is `limit`.
    fn open(
        &mut self,
        extension_id: Uuid,
        extension: ExtensionRef,
        lobby_id: &str,
        visibility: Visibility,
    ) -> Result<([u8; 16], Zeroizing<[u8; 16]>), PeerError> {
        if !valid_lobby_id(lobby_id) {
            return Err(PeerError::InvalidInput);
        }
        let own = || self.0.values().filter(|l| l.extension_id == extension_id);
        if own().any(|lobby| lobby.lobby_id == lobby_id) {
            return Err(PeerError::InvalidInput);
        }
        if own().count() >= LOBBIES_PER_EXTENSION {
            return Err(PeerError::Limit {
                limit: LOBBIES_PER_EXTENSION,
            });
        }
        let key = *random_bytes::<16>();
        let secret = random_bytes::<16>();
        self.0.insert(
            key,
            Lobby {
                lobby_id: lobby_id.to_owned(),
                extension_id,
                extension,
                visibility,
                secret: secret.clone(),
                banned_endpoints: HashSet::new(),
                banned_vaults: HashSet::new(),
            },
        );
        Ok((key, secret))
    }

    /// Closes `lobby_id` of `extension_id`, if open.
    fn close(&mut self, extension_id: Uuid, lobby_id: &str) {
        self.0
            .retain(|_, lobby| !(lobby.extension_id == extension_id && lobby.lobby_id == lobby_id));
    }

    /// Closes every lobby of `extension_id`.
    pub(super) fn close_all(&mut self, extension_id: Uuid) {
        self.0.retain(|_, lobby| lobby.extension_id != extension_id);
    }

    pub(super) fn clear(&mut self) {
        self.0.clear();
    }

    /// The extensions with open lobbies.
    pub(super) fn extensions(&self) -> impl Iterator<Item = Uuid> + '_ {
        self.0.values().map(|lobby| lobby.extension_id)
    }

    pub(super) fn openness(&self) -> Openness {
        let mut open = self.0.values().map(|lobby| lobby.visibility).peekable();
        if open.peek().is_none() {
            Openness::None
        } else if open.any(|visibility| visibility == Visibility::Public) {
            Openness::Public
        } else {
            Openness::ContactsOnly
        }
    }

    /// Bans `remote` from the lobby `key`: its endpoint, and its vault when it is verified.
    fn ban(&mut self, key: &[u8; 16], remote: &Verified) {
        if let Some(lobby) = self.0.get_mut(key) {
            lobby.banned_endpoints.insert(remote.endpoint_id);
            if remote.trust != Trust::Unverified {
                lobby.banned_vaults.insert(remote.vault);
            }
        }
    }

    /// Whether `join` from `remote` may enter (FR-024, FR-025). Every refusal before the proof
    /// holds takes the same path to `Closed`; `NotAdmitted` only follows a valid proof.
    pub fn decide(&self, join: &JoinRequest, remote: &Verified) -> Decision {
        let lobby = self.0.get(&join.lobby);
        let secret = lobby.map_or(&STAND_IN_SECRET, |lobby| &*lobby.secret);
        let proof_holds = join_proof_holds(secret, &remote.nonces, &join.lobby, &join.proof);
        let same_extension = lobby.is_some_and(|lobby| lobby.extension == join.extension);
        let Some(lobby) = lobby.filter(|_| proof_holds && same_extension) else {
            return Decision::Closed;
        };
        let banned = lobby.banned_endpoints.contains(&remote.endpoint_id)
            || lobby.banned_vaults.contains(&remote.vault);
        let refused_stranger =
            lobby.visibility == Visibility::Contacts && remote.trust == Trust::Unverified;
        if banned || refused_stranger {
            return Decision::NotAdmitted;
        }
        Decision::Accepted {
            key: join.lobby,
            lobby_id: lobby.lobby_id.clone(),
            extension_id: lobby.extension_id,
        }
    }
}

/// `lobbyId`: 1–64 characters of `[A-Za-z0-9._-]`.
pub fn valid_lobby_id(lobby_id: &str) -> bool {
    (1..=64).contains(&lobby_id.len())
        && lobby_id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
}

/// A display name of a join: 1–64 characters, chosen by the joiner and not checked further.
pub fn valid_display_name(name: &str) -> bool {
    DISPLAY_NAME_CHARS.contains(&name.chars().count())
}

/// Opens a lobby of `extension_id` (`extension_peers_lobby_open`) and returns its join code,
/// made from the endpoint's current address: its id and its relay.
pub fn open(
    peers: &PeerState,
    extension_id: Uuid,
    extension: ExtensionRef,
    lobby_id: &str,
    visibility: Visibility,
) -> Result<String, PeerError> {
    let session = peers.session().ok_or(PeerError::NetworkUnavailable)?;
    // ponytail: the address as it is now; a code made before the endpoint reached its home
    // relay names none and is reachable only where the host's addresses are known otherwise.
    // Upgrade: wait a moment for the home relay before making the code.
    let addr = session.node.addr();
    let (key, secret) = peers
        .lobbies()
        .open(extension_id, extension, lobby_id, visibility)?;
    let code = JoinCode {
        v: VERSION,
        endpoint_id: *addr.id.as_bytes(),
        relay: addr.relay_urls().next().map(ToString::to_string),
        lobby: key,
        secret,
    };
    Ok(code.text())
}

/// Closes a lobby (`extension_peers_lobby_close`): its code stops working at once, its
/// channels stay (FR-022). An unknown `lobby_id` is no error.
pub fn close(peers: &PeerState, extension_id: Uuid, lobby_id: &str) {
    peers.lobbies().close(extension_id, lobby_id);
}

/// Removes a joined device (`extension_peers_remove`, FR-027): its channel closes, and its
/// endpoint and, when verified, its vault cannot join that lobby again while it is open. Only
/// for channels that came from a lobby of this extension on this device.
pub fn remove(peers: &PeerState, extension_id: Uuid, channel_id: Uuid) -> Result<(), PeerError> {
    let channel = peers.channel(extension_id, channel_id)?;
    let Origin::Lobby { key, .. } = &channel.origin else {
        return Err(PeerError::ChannelUnknown);
    };
    peers.lobbies().ban(key, &channel.remote);
    peers.close(extension_id, channel_id)
}

/// Everything the host's side of a join needs besides the request.
pub(super) struct Joining<'a> {
    pub ctx: &'a PeerContext,
    pub host: &'a Arc<ExtensionHost>,
    pub tracker: &'a TaskTracker,
    pub throttle: &'a Throttle,
    pub connection: Connection,
    pub remote: Verified,
    pub send: SendStream,
    pub recv: RecvStream,
}

/// Answers a `Join` and, when it is admitted, makes the connection a channel and tells the host's
/// extension (FR-026). A refused device that is not verified, while no public lobby is open,
/// gets no answer at all (FR-031); every other refusal is answered and the connection closed.
pub(super) async fn admit_join(joining: Joining<'_>, join: JoinRequest) {
    let Joining {
        ctx,
        host,
        tracker,
        throttle,
        connection,
        remote,
        mut send,
        recv,
    } = joining;
    let payload = (valid_display_name(&join.display_name) && join.payload.len() <= PAYLOAD_BYTES)
        .then(|| serde_json::from_slice::<Value>(&join.payload).ok())
        .flatten();
    let decided = host.peers.lobbies().decide(&join, &remote);
    let now = Instant::now();
    let decision = match decided {
        _ if throttle.banned(remote.endpoint_id, now) || payload.is_none() => Decision::Closed,
        Decision::Accepted { extension_id, .. } if host.peers.channels_full(extension_id) => {
            Decision::Closed
        }
        decided => decided,
    };
    let (status, admitted) = match decision {
        Decision::Accepted {
            key,
            lobby_id,
            extension_id,
        } => (
            JoinStatus::Accepted {
                lobby_id: lobby_id.clone(),
            },
            Some((key, lobby_id, extension_id)),
        ),
        Decision::NotAdmitted => (JoinStatus::NotAdmitted, None),
        Decision::Closed => {
            throttle.join_failed(remote.endpoint_id, now);
            if remote.trust == Trust::Unverified && host.peers.openness() != Openness::Public {
                return refuse(&connection);
            }
            (JoinStatus::Closed, None)
        }
    };
    let reply = Message::JoinReply { v: VERSION, status };
    if wire::write(&mut send, &reply, ADMISSION_FRAME_LIMIT)
        .await
        .is_err()
    {
        return refuse(&connection);
    }
    let (Some((key, lobby_id, extension_id)), Some(payload)) = (admitted, payload) else {
        // The joiner closes once it read the answer; it gets a moment to do so.
        let _ = send.finish();
        let _ = tokio::time::timeout(REPLY_GRACE, connection.closed()).await;
        return refuse(&connection);
    };
    let contact_id = match &remote.trust {
        Trust::Contact { contact_id, .. }
            if contact_in_scope(ctx, host, extension_id, contact_id).await =>
        {
            Some(contact_id.clone())
        }
        _ => None,
    };
    let verified = remote.trust != Trust::Unverified;
    let opening = Opening {
        extension_id,
        remote,
        origin: Origin::Lobby {
            lobby_id: lobby_id.clone(),
            key,
        },
        connection,
        send,
        recv,
    };
    let announce = |channel: &Channel| {
        let mut joined = json!({
            "lobbyId": lobby_id,
            "channelId": channel.id.to_string(),
            "displayName": join.display_name,
            "verified": verified,
            "payload": payload,
        });
        if let Some(contact_id) = contact_id {
            joined["contactId"] = Value::String(contact_id);
        }
        ctx.frames.emit(extension_id, JOINED, &joined);
    };
    // The limit was checked before the answer; only a join that raced another one past it, or a
    // path that cannot carry a fast message, ends here, which the joiner hears as `closed`.
    if let Err(error) = channel::open_announced(opening, &ctx.frames, tracker, announce) {
        log::debug!("peers: an admitted join did not become a channel: {error}");
    }
}

#[cfg(test)]
#[path = "lobby_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "lobby_uniform_tests.rs"]
mod uniform_tests;
