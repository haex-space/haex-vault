//! Listening on the loopback interface (spec 049, FR-002, FR-003, research R1, R14): IPv4 always,
//! IPv6 where the device has it, at most a fixed number of connections, everything ended by one
//! token.

use std::io::ErrorKind;
use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr};
use std::sync::Arc;

use tokio::net::TcpListener;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use super::connection;
use super::prompts::BridgePrompts;
use super::runtime::{BridgeCore, Limits, Notifier};
use crate::vault_gate::VaultDb;

/// Why the bridge did not start.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum StartError {
    /// Another program listens on the port: another holzi, haex-vault 2.x or anything else
    /// (FR-002). holzi never moves to another port by itself.
    #[error("the port is in use")]
    PortInUse,
    #[error("could not listen: {0}")]
    Io(String),
}

pub struct StartArgs {
    pub db: VaultDb,
    pub device: Uuid,
    /// 0 picks a free port (tests).
    pub port: u16,
    /// The vault session's token; the bridge stops with it (FR-003).
    pub parent: CancellationToken,
    pub prompts: BridgePrompts,
    pub notify: Notifier,
    pub limits: Limits,
}

/// A running bridge.
pub struct Running {
    core: Arc<BridgeCore>,
    port: u16,
    loops: Vec<JoinHandle<()>>,
}

impl Running {
    pub fn port(&self) -> u16 {
        self.port
    }

    pub fn core(&self) -> &Arc<BridgeCore> {
        &self.core
    }

    /// Stops listening, ends every connection and question, and waits until the port is free
    /// (SC-008).
    pub async fn stop(self) {
        self.core.token.cancel();
        for handle in self.loops {
            let _ = handle.await;
        }
    }
}

/// Binds and starts accepting.
pub async fn start(args: StartArgs) -> Result<Running, StartError> {
    let v4 = TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, args.port)))
        .await
        .map_err(|error| match error.kind() {
            ErrorKind::AddrInUse => StartError::PortInUse,
            _ => StartError::Io(error.to_string()),
        })?;
    let port = v4
        .local_addr()
        .map_err(|error| StartError::Io(error.to_string()))?
        .port();
    let mut listeners = vec![v4];
    // The extension calls `localhost`, which a browser may resolve to ::1 first. Where the device
    // has no IPv6 loopback, IPv4 alone serves; a stranger on [::1] means the port is taken.
    match TcpListener::bind(SocketAddr::from((Ipv6Addr::LOCALHOST, port))).await {
        Ok(v6) => listeners.push(v6),
        Err(error) if error.kind() == ErrorKind::AddrInUse => return Err(StartError::PortInUse),
        Err(error) => {
            log::info!("browser bridge: no IPv6 loopback, listening on IPv4 only ({error})")
        }
    }
    let core = Arc::new(BridgeCore::new(
        args.db,
        args.device,
        args.parent.child_token(),
        args.prompts,
        args.notify,
        args.limits,
    ));
    let loops = listeners
        .into_iter()
        .map(|listener| tokio::spawn(accept_loop(Arc::clone(&core), listener)))
        .collect();
    log::info!("browser bridge: listening on port {port}");
    Ok(Running { core, port, loops })
}

async fn accept_loop(core: Arc<BridgeCore>, listener: TcpListener) {
    loop {
        let accepted = tokio::select! {
            _ = core.token.cancelled() => break,
            accepted = listener.accept() => accepted,
        };
        let Ok((stream, _)) = accepted else { continue };
        // Beyond the limit a connection is dropped before the upgrade (research R1).
        let Ok(slot) = Arc::clone(&core.slots).try_acquire_owned() else {
            continue;
        };
        tokio::spawn(connection::serve(Arc::clone(&core), stream, slot));
    }
}
