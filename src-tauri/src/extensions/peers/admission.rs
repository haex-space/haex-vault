//! The handler of `haex-vault-ext-peer/1` on the session endpoint (contracts/wire-protocol.md, research
//! R7): every incoming connection runs on the session's tracker, passes the [`Throttle`], proves
//! itself ([`foreign_auth`]) and then has [`ADMISSION_LIMIT`] to ask for admission (FR-032).
//!
//! A device of a vault that is no contact enters only while a lobby is open
//! (`docs/adr/0016-non-contact-devices-for-public-lobbies.md`): with a public lobby it may join as
//! not verified; with only lobbies for contacts it may send one bounded `Join`, which ends with
//! `NotAdmitted` after a valid proof and without an answer otherwise (FR-031); with none it is cut
//! right after the exchange. A `Join` goes to [`lobby::admit_join`]; an `Invite` of a contact to
//! [`invited::admit_invite`], and one of anybody else ends the connection, as silence does after
//! the limit. Every refusal looks the same from outside.
//!
//! [`ADMISSION_LIMIT`]: super::limits::ADMISSION_LIMIT

use std::sync::Arc;
use std::time::{Duration, Instant};

use iroh::endpoint::{Connection, RecvStream, SendStream};
use iroh::protocol::{AcceptError, ProtocolHandler};

use super::channel::Frames;
use super::invited::{self, InviteRequest, Inviting};
use super::limits::ADMISSION_LIMIT;
use super::lobby::{self, JoinRequest, Joining, Openness};
use super::throttle::Throttle;
use super::wire::{self, Message, ADMISSION_FRAME_LIMIT, JOIN_FRAME_LIMIT};
use super::FEATURE;
use crate::sync::endpoint::NodeTasks;
use crate::sync::foreign_auth::{self, Local, Purpose, SeenGenerations, Trust, Verified};
use crate::sync::keys::DeviceKeys;
use crate::sync::signing::Domain;
use crate::sync::wire::{ErrorCode, EXT_PEER_ALPN};
use crate::vault_gate::VaultDb;

/// What the proofs of `haex-vault-ext-peer/1` are bound to.
pub const PURPOSE: Purpose = Purpose {
    alpn: EXT_PEER_ALPN,
    domain: Domain::ExtPeerAuth,
};

/// What both sides of a channel need of this device: its keys and vault for the exchange, a vault
/// handle for the lookups, and the frames for the events.
pub struct PeerContext {
    pub keys: DeviceKeys,
    pub vault: [u8; 32],
    pub db: VaultDb,
    pub frames: Frames,
}

impl PeerContext {
    /// This device as [`foreign_auth`] presents it, with the session's `seen` generations.
    pub fn local<'a>(&'a self, seen: &'a SeenGenerations) -> Local<'a> {
        Local {
            keys: &self.keys,
            vault: self.vault,
            db: &self.db,
            feature: FEATURE,
            seen,
        }
    }
}

/// The handler the router calls for every connection on `haex-vault-ext-peer/1`.
#[derive(Clone)]
pub struct PeerProtocol {
    ctx: Arc<PeerContext>,
    tasks: NodeTasks,
    throttle: Throttle,
    admission_limit: Duration,
}

impl std::fmt::Debug for PeerProtocol {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PeerProtocol").finish_non_exhaustive()
    }
}

impl PeerProtocol {
    pub fn new(ctx: Arc<PeerContext>, tasks: NodeTasks) -> Self {
        Self {
            ctx,
            tasks,
            throttle: Throttle::default(),
            admission_limit: ADMISSION_LIMIT,
        }
    }

    /// The handler with a shorter wait for the admission message, so a test need not wait out
    /// [`ADMISSION_LIMIT`].
    #[cfg(test)]
    pub(crate) fn with_admission_limit(mut self, limit: Duration) -> Self {
        self.admission_limit = limit;
        self
    }
}

impl ProtocolHandler for PeerProtocol {
    async fn accept(&self, connection: Connection) -> Result<(), AcceptError> {
        // Tracked like the sync sessions: the router drops this future at its shutdown, the
        // node's shutdown waits for the tracker.
        self.tasks.tracker.spawn(self.clone().admit(connection));
        Ok(())
    }
}

impl PeerProtocol {
    async fn admit(self, connection: Connection) {
        let endpoint = *connection.remote_id().as_bytes();
        let Some(handshake) = self.throttle.handshake(endpoint, Instant::now()) else {
            return refuse(&connection);
        };
        let Some((remote, send, mut recv)) = verify_incoming(&self.ctx, &connection).await else {
            return refuse(&connection);
        };
        drop(handshake);
        let Some(host) = self.ctx.frames.host() else {
            return refuse(&connection);
        };
        // Only a device that is not verified waits in the throttle's pending places, and it
        // may send one bounded join only.
        let (frame_limit, _pending) = if remote.trust == Trust::Unverified {
            if host.peers.openness() == Openness::None {
                return refuse(&connection);
            }
            let Some(pending) = self.throttle.unverified(Instant::now()) else {
                return refuse(&connection);
            };
            (JOIN_FRAME_LIMIT, Some(pending))
        } else {
            (ADMISSION_FRAME_LIMIT, None)
        };
        let limit = self.admission_limit;
        let first = match tokio::time::timeout(limit, wire::read(&mut recv, frame_limit)).await {
            Ok(Ok(Some(message))) => message,
            Ok(Ok(None) | Err(_)) => return refuse(&connection),
            Err(_) => {
                log::debug!("peers: no admission message within {limit:?}");
                return refuse(&connection);
            }
        };
        match first {
            Message::Join {
                lobby,
                proof,
                extension,
                display_name,
                payload,
                ..
            } => {
                let joining = Joining {
                    ctx: &self.ctx,
                    host: &host,
                    tracker: &self.tasks.tracker,
                    throttle: &self.throttle,
                    connection,
                    remote,
                    send,
                    recv,
                };
                let request = JoinRequest {
                    lobby,
                    proof,
                    extension,
                    display_name,
                    payload,
                };
                lobby::admit_join(joining, request).await;
            }
            // Only a contact invites; anyone else, an own device included, is cut without a word.
            Message::Invite {
                invite,
                extension,
                payload,
                ..
            } if matches!(remote.trust, Trust::Contact { .. }) => {
                let inviting = Inviting {
                    ctx: &self.ctx,
                    host: &host,
                    tracker: &self.tasks.tracker,
                    connection,
                    remote,
                    send,
                    recv,
                };
                let request = InviteRequest {
                    invite,
                    extension,
                    payload,
                };
                invited::admit_invite(inviting, request).await;
            }
            _ => {
                log::debug!("peers: an admission message this device does not take");
                refuse(&connection);
            }
        }
    }
}

/// Runs the exchange on an incoming connection, within [`foreign_auth::HANDSHAKE_LIMIT`] from
/// the start: the verified other side and the first stream, or `None` after a refusal.
pub(super) async fn verify_incoming(
    ctx: &PeerContext,
    connection: &Connection,
) -> Option<(Verified, SendStream, RecvStream)> {
    let host = ctx.frames.host()?;
    let remote = *connection.remote_id().as_bytes();
    let exchange = async {
        let (mut send, mut recv) = connection.accept_bi().await.ok()?;
        let local = ctx.local(host.peers.seen());
        match foreign_auth::accept(&mut send, &mut recv, &local, PURPOSE, remote).await {
            Ok(verified) => Some((verified, send, recv)),
            Err(error) => {
                log::debug!("peers: an incoming device was refused: {error}");
                None
            }
        }
    };
    tokio::time::timeout(foreign_auth::HANDSHAKE_LIMIT, exchange)
        .await
        .ok()
        .flatten()
}

/// Ends a connection without telling why; every refusal looks the same.
pub(super) fn refuse(connection: &Connection) {
    connection.close(ErrorCode::Rejected.as_u32().into(), b"");
}

#[cfg(test)]
#[path = "admission_tests.rs"]
mod tests;
