//! The running MCP access of one vault session (spec 021 FR-001, FR-002, research R15): the
//! endpoint, everything the connections share, and one token that ends them all when the vault
//! closes or the user turns the access off.

use std::collections::HashSet;
use std::io::ErrorKind;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use super::connection;
use super::delegates::DelegateHub;
use super::dispatch::CallSlots;
use super::endpoint;
use super::hello::CloseReason;
use super::http;
use super::live::Live;
use super::oauth::OAuthState;
use super::prompts::{CallPrompts, PromptClosed, RegistrationGate, RegistrationPrompts};
use super::registry::Transport;
use crate::chat::tools::action_bridge::ActionBridge;
use crate::chat::tools::action_catalog::{ActionCatalog, ExtensionToolCatalog};
use crate::chat::tools::native_action::NativeExecutors;
use crate::chat::tools::Tool;
use crate::vault_events::VaultChanges;
use crate::vault_gate::VaultDb;

/// What the server tells the window.
#[derive(Debug, Clone)]
pub enum Notice {
    /// Agents or their state changed; the settings reload.
    Changed,
    /// A registration dialog or call prompt ended without the window answering it.
    PromptClosed(PromptClosed),
    /// The last connection of an agent ended; what haex-vault keeps for it while connected (its
    /// browser session, spec 065) ends too.
    Disconnected(Uuid),
    /// The agent's profile changed and it has no browser right any more (spec 083); its browser
    /// session ends, as it does when the rights change by hand.
    BrowserRightLost(Uuid),
}

pub type Notifier = Arc<dyn Fn(Notice) + Send + Sync>;

/// The limits of one connection (contracts/agent-helper.md, "Grenzen").
#[derive(Debug, Clone, Copy)]
pub struct Limits {
    /// The helper's greeting must arrive within this time.
    pub greeting_deadline: Duration,
    /// The longest MCP message haex-vault reads.
    pub max_message_bytes: usize,
    /// How long a pending agent may take to send `initialize` before the dialog is skipped.
    pub initialize_deadline: Duration,
    /// How many calls of one agent run at once, prompts included (FR-029, research R11).
    pub max_calls: usize,
    /// How long a further call waits for a free slot before `too_many_calls`.
    pub call_wait: Duration,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            greeting_deadline: Duration::from_secs(5),
            max_message_bytes: 1024 * 1024,
            initialize_deadline: Duration::from_secs(30),
            max_calls: 4,
            call_wait: Duration::from_secs(30),
        }
    }
}

/// Why the server did not start.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum StartError {
    /// Another haex-vault serves the same vault on this device.
    #[error("another haex-vault serves this vault")]
    InUse,
    #[error("could not listen: {0}")]
    Io(String),
}

pub struct StartArgs {
    pub db: VaultDb,
    pub device: Uuid,
    /// Where the Unix socket goes; unused on Windows.
    pub socket_dir: PathBuf,
    /// The vault session's token; the server stops with it.
    pub parent: CancellationToken,
    pub catalog: ActionCatalog,
    /// The tools of extensions (spec 018), offered per agent by its grants (US4).
    pub extension_tools: ExtensionToolCatalog,
    /// The tables each commit changed; a change of the agent profiles changes tool lists (US5).
    pub vault_changes: VaultChanges,
    pub bridge: ActionBridge,
    /// The Rust executors of native catalog actions (ADR 0011).
    pub native: NativeExecutors,
    /// haex-vault's command tool (spec 003); `None` where the device runs no commands (US5).
    pub command_tool: Option<Arc<dyn Tool>>,
    pub prompts: RegistrationPrompts,
    pub calls: CallPrompts,
    /// The passes of delegates' starts (US6); the started server answers them.
    pub delegates: Arc<DelegateHub>,
    pub notify: Notifier,
    pub limits: Limits,
    /// The port of the HTTP way (spec 021 PR G); `Some(0)` picks a free one, `None` serves only the
    /// program way.
    pub http_port: Option<u16>,
    /// The program way (`--mcp-agent` over a socket or pipe); a phone has none (research R12).
    pub program_way: bool,
    /// Whether programs outside haex-vault may connect: the device's external access (spec 102
    /// FR-011). Without it the HTTP way is closed and the program way admits only delegates'
    /// passes.
    pub external: bool,
}

/// What every connection of one running server shares.
pub struct Core {
    pub db: VaultDb,
    pub device: Uuid,
    pub catalog: ActionCatalog,
    pub extension_tools: ExtensionToolCatalog,
    pub vault_changes: VaultChanges,
    pub bridge: ActionBridge,
    pub native: NativeExecutors,
    pub command_tool: Option<Arc<dyn Tool>>,
    pub prompts: RegistrationPrompts,
    pub calls: CallPrompts,
    pub delegates: Arc<DelegateHub>,
    pub slots: CallSlots,
    pub live: Live,
    pub gate: RegistrationGate,
    pub notify: Notifier,
    pub limits: Limits,
    pub token: CancellationToken,
    /// Agents the user declined without remembering it; until the vault closes.
    declined: Mutex<HashSet<(Transport, String)>>,
    closing: Mutex<CloseReason>,
    /// See [`StartArgs::external`]; changes while the server runs.
    external: AtomicBool,
}

impl Core {
    /// Whether programs outside haex-vault may connect now (spec 102 FR-011).
    pub fn external(&self) -> bool {
        self.external.load(Ordering::SeqCst)
    }

    pub fn declined(&self, slug: &str, transport: Transport) -> bool {
        self.declined
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .contains(&(transport, slug.to_owned()))
    }

    pub fn decline(&self, slug: &str, transport: Transport) {
        self.declined
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert((transport, slug.to_owned()));
    }

    /// Allows an agent the user unblocked to ask again during this vault session.
    pub fn undecline(&self, slug: &str, transport: Transport) {
        self.declined
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(&(transport, slug.to_owned()));
    }

    /// Why the connections end once [`Self::token`] fired.
    pub fn closing_reason(&self) -> CloseReason {
        *self.closing.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn close(&self, reason: CloseReason) {
        *self.closing.lock().unwrap_or_else(PoisonError::into_inner) = reason;
        self.token.cancel();
    }
}

/// The HTTP way while it runs: where it listens, its own stop and its task.
struct HttpWay {
    address: SocketAddr,
    stop: CancellationToken,
    task: JoinHandle<()>,
}

impl HttpWay {
    async fn start(core: &Arc<Core>, port: u16) -> Result<Self, StartError> {
        let listener = http::bind(port)
            .await
            .map_err(|error| StartError::Io(error.to_string()))?;
        let address = listener
            .local_addr()
            .map_err(|error| StartError::Io(error.to_string()))?;
        // A child of the server's token: it also ends when the vault closes.
        let stop = core.token.child_token();
        let oauth = Arc::new(OAuthState::default());
        let task = tokio::spawn(http::serve(Arc::clone(core), oauth, listener, stop.clone()));
        Ok(Self {
            address,
            stop,
            task,
        })
    }

    async fn stop(self) {
        self.stop.cancel();
        let _ = self.task.await;
    }
}

/// A running server.
pub struct Running {
    core: Arc<Core>,
    accept: Option<JoinHandle<()>>,
    http: Option<HttpWay>,
}

impl Running {
    pub fn core(&self) -> &Arc<Core> {
        &self.core
    }

    /// Where the HTTP way listens, when it runs.
    pub fn http_address(&self) -> Option<SocketAddr> {
        self.http.as_ref().map(|way| way.address)
    }

    /// Opens or closes the server to programs outside haex-vault (spec 102 FR-011), without a
    /// restart: running delegates keep their connections. Turning it off closes the HTTP way and
    /// ends every connection that is not a delegate's; turning it on opens the HTTP way on
    /// `http_port` (`Some(0)` picks a free one, `None` where the platform has none).
    pub async fn set_external(
        &mut self,
        external: bool,
        http_port: Option<u16>,
    ) -> Result<(), StartError> {
        if !external {
            self.core.external.store(false, Ordering::SeqCst);
            if let Some(way) = self.http.take() {
                way.stop().await;
            }
            self.core.live.close_outside(CloseReason::Disabled);
            return Ok(());
        }
        if let (None, Some(port)) = (&self.http, http_port) {
            // Do not admit outside program connections until the requested HTTP path is ready.
            let way = HttpWay::start(&self.core, port).await?;
            self.http = Some(way);
        }
        self.core.external.store(true, Ordering::SeqCst);
        Ok(())
    }

    /// Ends every connection with `reason` and the open dialogs and prompts, and removes the
    /// endpoint.
    pub async fn stop(self, reason: CloseReason) {
        self.core.close(reason);
        self.core.prompts.cancel_all();
        self.core.calls.cancel_all();
        if let Some(accept) = self.accept {
            let _ = accept.await;
        }
        if let Some(way) = self.http {
            way.stop().await;
        }
    }
}

/// Binds the endpoint and starts accepting.
pub async fn start(args: StartArgs) -> Result<Running, StartError> {
    let listener = if args.program_way {
        Some(
            endpoint::bind(&args.socket_dir, args.device)
                .await
                .map_err(|error| match error.kind() {
                    ErrorKind::AddrInUse => StartError::InUse,
                    _ => StartError::Io(error.to_string()),
                })?,
        )
    } else {
        None
    };
    let http_port = args.http_port.filter(|_| args.external);
    let core = Arc::new(Core {
        db: args.db,
        device: args.device,
        catalog: args.catalog,
        extension_tools: args.extension_tools,
        vault_changes: args.vault_changes,
        bridge: args.bridge,
        native: args.native,
        command_tool: args.command_tool,
        prompts: args.prompts,
        calls: args.calls,
        delegates: args.delegates,
        slots: CallSlots::default(),
        live: Live::default(),
        gate: RegistrationGate::default(),
        notify: args.notify,
        limits: args.limits,
        token: args.parent.child_token(),
        declined: Mutex::new(HashSet::new()),
        closing: Mutex::new(CloseReason::VaultClosed),
        external: AtomicBool::new(args.external),
    });
    // The HTTP way first: a port that cannot be bound leaves nothing running.
    let http = match http_port {
        Some(port) => Some(HttpWay::start(&core, port).await?),
        None => None,
    };
    core.delegates.attach(&core);
    let accept = listener.map(|listener| tokio::spawn(accept_loop(Arc::clone(&core), listener)));
    log::info!("mcp server: listening");
    Ok(Running { core, accept, http })
}

async fn accept_loop(core: Arc<Core>, mut listener: endpoint::Listener) {
    loop {
        let accepted = tokio::select! {
            _ = core.token.cancelled() => break,
            accepted = listener.accept() => accepted,
        };
        match accepted {
            Ok(Some(stream)) => {
                tokio::spawn(connection::serve(Arc::clone(&core), stream));
            }
            Ok(None) => log::warn!("mcp server: refused a connection of another user"),
            Err(error) => {
                log::warn!("mcp server: accept failed: {error}");
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        }
    }
    // Dropping the listener removes the socket.
}
