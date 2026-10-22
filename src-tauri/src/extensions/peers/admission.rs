//! The handler of `holzi-ext-peer/1` on the session endpoint (contracts/wire-protocol.md, research
//! R7): every incoming connection runs on the session's tracker, proves itself
//! ([`foreign_auth`]) and then has [`ADMISSION_LIMIT`] to ask for admission (FR-032).
//!
//! This delivery holds the frame only. A device that is not verified is cut right after the
//! exchange: public lobbies, the one place it may enter (ADR 0016), come with the lobbies. Neither
//! invitations nor lobbies exist yet, so no admission message can open a channel; each ends the
//! connection, as silence does after the limit. Every refusal looks the same from outside.
//!
//! [`ADMISSION_LIMIT`]: super::limits::ADMISSION_LIMIT

use std::sync::Arc;
use std::time::Duration;

use iroh::endpoint::{Connection, RecvStream, SendStream};
use iroh::protocol::{AcceptError, ProtocolHandler};

use super::channel::Frames;
use super::limits::ADMISSION_LIMIT;
use super::wire::{self, ADMISSION_FRAME_LIMIT};
use super::FEATURE;
use crate::sync::endpoint::NodeTasks;
use crate::sync::foreign_auth::{self, Local, Purpose, SeenGenerations, Trust, Verified};
use crate::sync::keys::DeviceKeys;
use crate::sync::signing::Domain;
use crate::sync::wire::{ErrorCode, EXT_PEER_ALPN};
use crate::vault_gate::VaultDb;

/// What the proofs of `holzi-ext-peer/1` are bound to.
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

/// The handler the router calls for every connection on `holzi-ext-peer/1`.
#[derive(Clone)]
pub struct PeerProtocol {
    ctx: Arc<PeerContext>,
    tasks: NodeTasks,
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
        let (ctx, limit) = (Arc::clone(&self.ctx), self.admission_limit);
        self.tasks.tracker.spawn(admit(ctx, connection, limit));
        Ok(())
    }
}

async fn admit(ctx: Arc<PeerContext>, connection: Connection, limit: Duration) {
    let Some((verified, _send, mut recv)) = verify_incoming(&ctx, &connection).await else {
        return refuse(&connection);
    };
    if verified.trust == Trust::Unverified {
        return refuse(&connection);
    }
    match tokio::time::timeout(limit, wire::read(&mut recv, ADMISSION_FRAME_LIMIT)).await {
        Ok(Ok(Some(_not_offered_yet))) => {
            log::debug!("peers: an admission message this delivery does not offer");
        }
        Ok(Ok(None) | Err(_)) => {}
        Err(_) => log::debug!("peers: no admission message within {limit:?}"),
    }
    refuse(&connection);
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
fn refuse(connection: &Connection) {
    connection.close(ErrorCode::Rejected.as_u32().into(), b"");
}

#[cfg(test)]
#[path = "admission_tests.rs"]
mod tests;
