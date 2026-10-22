//! Further protocols on the session endpoint beside `holzi-sync/1` (spec 057 research R1; spec 056
//! extends `bind` the same way).
//!
//! iroh takes the ALPNs of a [`Router`](iroh::protocol::Router) from its handlers when it spawns,
//! and none can be added afterwards, so the handlers are handed to [`SyncNode::bind_with`]. Each
//! is built once the node exists, from the node's [`NodeTasks`]: its connections then run as
//! tracked work of the session, which [`SyncNode::shutdown`] cancels and waits for, like the sync
//! sessions themselves.

use iroh::endpoint::Connection;
use iroh::protocol::{DynProtocolHandler, ProtocolHandler, RouterBuilder};
use iroh::EndpointAddr;
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;

use super::{connect_alpn, NodeError, SyncNode};
use crate::sync::wire::SYNC_ALPN;

/// Where a further protocol runs its work: the node's tracker and the token that fires when the
/// node shuts down.
#[derive(Debug, Clone)]
pub struct NodeTasks {
    pub tracker: TaskTracker,
    pub cancel: CancellationToken,
}

type BuildHandler = Box<dyn FnOnce(NodeTasks) -> Box<dyn DynProtocolHandler> + Send>;

/// Protocol handlers to register beside `holzi-sync/1`, each by its ALPN.
#[derive(Default)]
pub struct Protocols(Vec<(&'static [u8], BuildHandler)>);

impl Protocols {
    /// Adds the handler `build` makes for `alpn`. `holzi-sync/1` stays the node's own.
    pub fn with<H: ProtocolHandler>(
        mut self,
        alpn: &'static [u8],
        build: impl FnOnce(NodeTasks) -> H + Send + 'static,
    ) -> Self {
        debug_assert_ne!(alpn, SYNC_ALPN, "holzi-sync/1 is the node's own protocol");
        self.0
            .push((alpn, Box::new(move |tasks| Box::new(build(tasks)))));
        self
    }

    /// Registers every handler on `router`.
    pub(super) fn register(self, mut router: RouterBuilder, tasks: &NodeTasks) -> RouterBuilder {
        for (alpn, build) in self.0 {
            router = router.accept(alpn, build(tasks.clone()));
        }
        router
    }
}

impl SyncNode {
    /// Dials `addr` on `alpn` and hands the connection to the caller, which runs the protocol
    /// itself. The address is kept for iroh, as for every dial of this node.
    pub async fn dial_alpn(
        &self,
        addr: EndpointAddr,
        alpn: &[u8],
    ) -> Result<Connection, NodeError> {
        connect_alpn(&self.inner, addr, alpn).await
    }

    /// Where work that belongs to this node's session runs.
    pub fn tasks(&self) -> NodeTasks {
        NodeTasks {
            tracker: self.inner.tracker.clone(),
            cancel: self.inner.cancel.clone(),
        }
    }
}

#[cfg(test)]
#[path = "endpoint_protocols_tests.rs"]
mod tests;
