//! The requesting side of `haex-vault-files/1`: one verified connection to a serving device
//! ([`PeerLink`]) that sends a request per bi-stream, and the seam that finds such a link for a
//! target ([`PeerLinks`]). Finding the address (presence for own devices, the knock for contacts)
//! and keeping one link per device belong to the implementations of the seam.

use async_trait::async_trait;
use iroh::endpoint::{Connection, RecvStream};
use iroh::EndpointAddr;

use crate::files::peer::handshake::{self, DialError};
use crate::files::peer::wire::{
    read_head, write_head, Caller, Mode, Op, Reply, Request, Welcome, VERSION,
};
use crate::remote_storage::StorageError;
use crate::sync::endpoint::SyncNode;
use crate::sync::foreign_auth::{Local, Verified};
use crate::sync::wire::FILES_ALPN;

/// A verified connection to a serving device.
#[derive(Debug)]
pub struct PeerLink {
    connection: Connection,
    pub verified: Verified,
    pub welcome: Welcome,
}

impl PeerLink {
    /// Dials `addr` on `haex-vault-files/1` and runs the exchange.
    pub async fn open(
        node: &SyncNode,
        local: &Local<'_>,
        addr: EndpointAddr,
    ) -> Result<Self, DialError> {
        let connection = node
            .dial_alpn(addr, FILES_ALPN)
            .await
            .map_err(|_| DialError::Unreachable)?;
        Self::over(connection, local).await
    }

    /// Runs the exchange on an open `connection`.
    pub async fn over(connection: Connection, local: &Local<'_>) -> Result<Self, DialError> {
        let (verified, welcome) = handshake::dial(local, &connection).await?;
        Ok(Self {
            connection,
            verified,
            welcome,
        })
    }

    /// Whether the connection has ended.
    pub fn is_closed(&self) -> bool {
        self.connection.close_reason().is_some()
    }

    /// Ends the connection; the serving device drops what belongs to it (its uploads).
    pub fn close(&self) {
        self.connection.close(0u32.into(), b"");
    }

    /// Whether this link can serve requests. An `AccessOff` welcome is a live handshake result,
    /// but it must be retried after the serving device enables access again.
    pub fn is_reusable(&self) -> bool {
        !self.is_closed() && self.welcome.mode != Mode::AccessOff
    }

    /// Sends `op` as `caller` on a new stream and reads the answer's head; for a read, the bytes
    /// follow on the returned stream, for a watch every further head.
    pub async fn ask(&self, caller: Caller, op: Op) -> Result<(Reply, RecvStream), StorageError> {
        self.ask_with(caller, op, &[]).await
    }

    /// [`PeerLink::ask`] with `body` after the request's head (a part of an upload).
    pub async fn ask_with(
        &self,
        caller: Caller,
        op: Op,
        body: &[u8],
    ) -> Result<(Reply, RecvStream), StorageError> {
        let (mut send, mut recv) = self
            .connection
            .open_bi()
            .await
            .map_err(|_| StorageError::Network)?;
        let request = Request {
            v: VERSION,
            caller,
            op,
        };
        write_head(&mut send, &request)
            .await
            .map_err(|_| StorageError::Network)?;
        send.write_all(body)
            .await
            .map_err(|_| StorageError::Network)?;
        send.finish().map_err(|_| StorageError::Network)?;
        let reply = read_head(&mut recv)
            .await
            .map_err(|_| StorageError::Network)?;
        Ok((reply, recv))
    }
}

/// Finds a link for a target: `peer:<vault_device_uuid>` for another own device, `share:<id>` for a
/// received share.
#[async_trait]
pub trait PeerLinks: Send + Sync {
    async fn link(&self, target: &str) -> Result<std::sync::Arc<PeerLink>, StorageError>;
}
