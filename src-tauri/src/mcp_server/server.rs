//! The running MCP access of one vault session (spec 021 FR-001, FR-002, research R15): the
//! endpoint, everything the connections share, and one token that ends them all when the vault
//! closes or the user turns the access off.

use std::collections::HashSet;
use std::io::ErrorKind;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use super::connection;
use super::dispatch::CallSlots;
use super::endpoint;
use super::hello::CloseReason;
use super::live::Live;
use super::prompts::{CallPrompts, PromptClosed, RegistrationGate, RegistrationPrompts};
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
    /// The last connection of an agent ended; what holzi keeps for it while connected (its
    /// browser session, spec 065) ends too.
    Disconnected(Uuid),
}

pub type Notifier = Arc<dyn Fn(Notice) + Send + Sync>;

/// The limits of one connection (contracts/agent-helper.md, "Grenzen").
#[derive(Debug, Clone, Copy)]
pub struct Limits {
    /// The helper's greeting must arrive within this time.
    pub greeting_deadline: Duration,
    /// The longest MCP message holzi reads.
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
    /// Another holzi serves the same vault on this device.
    #[error("another holzi serves this vault")]
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
    /// The tables each commit changed; a change of `agent_file_permissions` changes tool lists (US5).
    pub vault_changes: VaultChanges,
    pub bridge: ActionBridge,
    /// The Rust executors of native catalog actions (ADR 0011).
    pub native: NativeExecutors,
    /// holzi's command tool (spec 003); `None` where the device runs no commands (US5).
    pub command_tool: Option<Arc<dyn Tool>>,
    pub prompts: RegistrationPrompts,
    pub calls: CallPrompts,
    pub notify: Notifier,
    pub limits: Limits,
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
    pub slots: CallSlots,
    pub live: Live,
    pub gate: RegistrationGate,
    pub notify: Notifier,
    pub limits: Limits,
    pub token: CancellationToken,
    /// Agents the user declined without remembering it; until the vault closes.
    declined: Mutex<HashSet<String>>,
    closing: Mutex<CloseReason>,
}

impl Core {
    pub fn declined(&self, slug: &str) -> bool {
        self.declined
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .contains(slug)
    }

    pub fn decline(&self, slug: &str) {
        self.declined
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(slug.to_owned());
    }

    /// Allows an agent the user unblocked to ask again during this vault session.
    pub fn undecline(&self, slug: &str) {
        self.declined
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(slug);
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

/// A running server.
pub struct Running {
    core: Arc<Core>,
    accept: JoinHandle<()>,
}

impl Running {
    pub fn core(&self) -> &Arc<Core> {
        &self.core
    }

    /// Ends every connection with `reason` and the open dialogs and prompts, and removes the
    /// endpoint.
    pub async fn stop(self, reason: CloseReason) {
        self.core.close(reason);
        self.core.prompts.cancel_all();
        self.core.calls.cancel_all();
        let _ = self.accept.await;
    }
}

/// Binds the endpoint and starts accepting.
pub async fn start(args: StartArgs) -> Result<Running, StartError> {
    let listener = endpoint::bind(&args.socket_dir, args.device)
        .await
        .map_err(|error| match error.kind() {
            ErrorKind::AddrInUse => StartError::InUse,
            _ => StartError::Io(error.to_string()),
        })?;
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
        slots: CallSlots::default(),
        live: Live::default(),
        gate: RegistrationGate::default(),
        notify: args.notify,
        limits: args.limits,
        token: args.parent.child_token(),
        declined: Mutex::new(HashSet::new()),
        closing: Mutex::new(CloseReason::VaultClosed),
    });
    let accept = tokio::spawn(accept_loop(Arc::clone(&core), listener));
    log::info!("mcp server: listening");
    Ok(Running { core, accept })
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
