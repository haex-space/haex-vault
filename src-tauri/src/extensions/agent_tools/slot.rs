//! The tool connection of one frame session (spec 018, research R10, R11): opened on the first
//! call, closed with the session, and replaced after the frame loads a new page. The session also
//! counts the tool calls in flight, so a host prompt the extension raises meanwhile can name the
//! agent as its cause.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use futures::channel::mpsc::{unbounded, UnboundedSender};
use rmcp::service::{Peer, RoleClient};
use serde_json::Value;

use super::link::{connect, Link};

/// An open connection and the tools the extension reported in `tools/list`.
pub struct McpLink {
    link: Link,
    tools: Vec<String>,
}

impl McpLink {
    pub fn peer(&self) -> &Peer<RoleClient> {
        self.link.peer()
    }

    /// The names from `tools/list`, in the extension's order.
    pub fn tools(&self) -> &[String] {
        &self.tools
    }

    fn cancel(&self) {
        self.link.cancellation_token().cancel();
    }
}

/// Why no connection could be opened.
#[derive(Debug, thiserror::Error)]
pub enum LinkError {
    #[error("the frame session is closed")]
    Closed,
    #[error("the extension did not complete the MCP handshake: {0}")]
    Handshake(String),
    #[error("the extension did not list its tools: {0}")]
    List(String),
}

/// The connection slot of a frame session.
#[derive(Default)]
pub struct McpSlot {
    /// Where the extension's messages go while a connection is open or opening.
    inbound: Mutex<Option<UnboundedSender<Value>>>,
    current: Mutex<Option<Arc<McpLink>>>,
    /// Only one caller opens a connection; the others wait for it and reuse it.
    opening: tokio::sync::Mutex<()>,
    closed: AtomicBool,
    in_flight: AtomicUsize,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

impl McpSlot {
    /// A message of the extension (`extension_mcp_send`); dropped while no connection is open.
    pub fn deliver(&self, message: Value) {
        match lock(&self.inbound).as_ref() {
            Some(inbound) => {
                let _ = inbound.unbounded_send(message);
            }
            None => log::debug!("agent tools: dropped a message for a frame without a connection"),
        }
    }

    /// The open connection, or a new one: handshake and `tools/list`, with `send` carrying
    /// holzi's messages to the frame. No time limit of its own; the caller bounds it (R9).
    pub async fn link<F>(&self, send: F) -> Result<Arc<McpLink>, LinkError>
    where
        F: Fn(Value) + Send + 'static,
    {
        let _opening = self.opening.lock().await;
        if self.closed.load(Ordering::SeqCst) {
            return Err(LinkError::Closed);
        }
        if let Some(link) = lock(&self.current).clone() {
            if !link.link.is_closed() {
                return Ok(link);
            }
        }
        let (inbound, receive) = unbounded();
        *lock(&self.inbound) = Some(inbound);
        let opened = async {
            let link = connect(send, receive)
                .await
                .map_err(|error| LinkError::Handshake(error.to_string()))?;
            let tools = link
                .peer()
                .list_all_tools()
                .await
                .map_err(|error| LinkError::List(error.to_string()))?;
            Ok(Arc::new(McpLink {
                link,
                tools: tools.iter().map(|tool| tool.name.to_string()).collect(),
            }))
        }
        .await;
        let link = match opened {
            Ok(link) => link,
            Err(error) => {
                lock(&self.inbound).take();
                return Err(error);
            }
        };
        if self.closed.load(Ordering::SeqCst) {
            link.cancel();
            return Err(LinkError::Closed);
        }
        *lock(&self.current) = Some(Arc::clone(&link));
        Ok(link)
    }

    /// Ends the connection; calls waiting for the extension fail. The next call opens a new one,
    /// as after the frame loaded a new page.
    pub fn reset(&self) {
        lock(&self.inbound).take();
        if let Some(link) = lock(&self.current).take() {
            link.cancel();
        }
    }

    /// Ends the connection for good: the session is closed.
    pub fn close(&self) {
        self.closed.store(true, Ordering::SeqCst);
        self.reset();
    }

    /// Counts a tool call until the returned guard is dropped.
    pub fn begin_call(&self) -> CallGuard<'_> {
        self.in_flight.fetch_add(1, Ordering::SeqCst);
        CallGuard(&self.in_flight)
    }

    pub fn calls_in_flight(&self) -> usize {
        self.in_flight.load(Ordering::SeqCst)
    }
}

impl std::fmt::Debug for McpSlot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("McpSlot")
            .field("open", &lock(&self.current).is_some())
            .field("closed", &self.closed.load(Ordering::SeqCst))
            .field("in_flight", &self.calls_in_flight())
            .finish()
    }
}

/// One tool call in flight; see [`McpSlot::begin_call`].
pub struct CallGuard<'a>(&'a AtomicUsize);

impl Drop for CallGuard<'_> {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}
