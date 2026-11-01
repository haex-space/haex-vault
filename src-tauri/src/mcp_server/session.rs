//! One agent's MCP session (spec 021, contracts/mcp-surface.md): the server handler rmcp drives for
//! one connection. Until the agent is registered it sees no tool and every call is refused; after
//! the registration its rights decide both the list and each call.

use std::collections::HashSet;
use std::future::Future;
use std::sync::{Arc, Mutex, PoisonError, RwLock};

use rmcp::model::{
    CallToolRequestParams, CallToolResponse, Implementation, InitializeResult, ListToolsResult,
    PaginatedRequestParams, ServerCapabilities, ServerConfig,
};
use rmcp::service::{MaybeSendFuture, NotificationContext, RequestContext, RoleServer};
use rmcp::{ErrorData as McpError, ServerHandler};
use tokio::sync::watch;
use uuid::Uuid;

use super::command_tool;
use super::dispatch;
use super::extension_tools;
use super::grants::{self, Rights};
use super::live::Link;
use super::registry::AgentMode;
use super::results;
use super::server::Core;

/// What a registered agent reads in `initialize` (contracts/mcp-surface.md, "Server").
pub const INSTRUCTIONS_REGISTERED: &str = "These tools operate holzi on this device. Name targets \
    such as tabs, windows and workspaces explicitly by their id. A result with the code \
    `needs_choice` lists candidates; call again with one of them.";

/// What an agent waiting for its registration reads.
pub const INSTRUCTIONS_WAITING: &str =
    "Waiting for the registration in holzi. Confirm the dialog in the holzi window.";

/// The registered agent behind a session.
#[derive(Debug, Clone)]
pub struct SessionAgent {
    pub id: Uuid,
    pub rights: Rights,
    pub mode: AgentMode,
    /// The agent's own claim, shown in call prompts.
    pub client_name: String,
}

/// The server handler of one connection. Cheap to clone; the clones share the state.
#[derive(Clone)]
pub struct Session {
    core: Arc<Core>,
    agent: Arc<RwLock<Option<SessionAgent>>>,
    initialized: Arc<watch::Sender<bool>>,
    link: Arc<Link>,
    /// Every tool name a list of this connection held, so a tool that went away since reads as
    /// unavailable rather than as not allowed (US4 scenario 4).
    seen: Arc<Mutex<HashSet<String>>>,
}

impl Session {
    pub fn new(core: Arc<Core>, agent: Option<SessionAgent>, link: Arc<Link>) -> Self {
        let (initialized, _) = watch::channel(false);
        Self {
            core,
            agent: Arc::new(RwLock::new(agent)),
            initialized: Arc::new(initialized),
            link,
            seen: Arc::default(),
        }
    }

    /// How this connection ends from holzi's side.
    pub fn link(&self) -> &Arc<Link> {
        &self.link
    }

    /// Changes the registered agent in place (US7: rights or mode changed while connected).
    pub fn update_agent(&self, change: impl FnOnce(&mut SessionAgent)) {
        if let Some(agent) = self
            .agent
            .write()
            .unwrap_or_else(PoisonError::into_inner)
            .as_mut()
        {
            change(agent);
        }
    }

    pub fn agent(&self) -> Option<SessionAgent> {
        self.agent
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// After the registration: the agent gets its tools.
    pub fn set_agent(&self, agent: SessionAgent) {
        *self.agent.write().unwrap_or_else(PoisonError::into_inner) = Some(agent);
    }

    /// Resolves once the client sent `notifications/initialized`.
    pub fn initialized(&self) -> watch::Receiver<bool> {
        self.initialized.subscribe()
    }

    async fn tools(&self) -> Vec<rmcp::model::Tool> {
        let Some(agent) = self.agent() else {
            return Vec::new();
        };
        let reach = dispatch::reach(&self.core, agent.id).await;
        let catalog = self.core.catalog.snapshot();
        let extension_tools = self.core.extension_tools.snapshot();
        let tools: Vec<_> = grants::offers(&agent.rights, &reach, &catalog)
            .into_iter()
            .map(dispatch::tool_of)
            .chain(
                grants::extension_offers(&agent.rights, &extension_tools)
                    .into_iter()
                    .map(|tool| extension_tools::tool_of(tool.as_ref())),
            )
            .chain(
                command_tool::offer(&agent.rights, self.core.command_tool.as_ref())
                    .map(|tool| extension_tools::tool_of(tool.as_ref())),
            )
            .collect();
        self.seen
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .extend(tools.iter().map(|tool| tool.name.to_string()));
        tools
    }

    fn was_seen(&self, tool_name: &str) -> bool {
        self.seen
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .contains(tool_name)
    }
}

impl ServerHandler for Session {
    fn get_info(&self) -> ServerConfig {
        let capabilities = ServerCapabilities::builder()
            .enable_tools()
            .enable_tool_list_changed()
            .build();
        let instructions = if self.agent().is_some() {
            INSTRUCTIONS_REGISTERED
        } else {
            INSTRUCTIONS_WAITING
        };
        InitializeResult::new(capabilities)
            .with_server_info(Implementation::new("holzi", env!("CARGO_PKG_VERSION")))
            .with_instructions(instructions)
    }

    fn on_initialized(
        &self,
        _context: NotificationContext<RoleServer>,
    ) -> impl Future<Output = ()> + MaybeSendFuture + '_ {
        self.initialized.send_replace(true);
        std::future::ready(())
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, McpError> {
        Ok(ListToolsResult {
            tools: self.tools().await,
            ..Default::default()
        })
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, McpError> {
        let Some(agent) = self.agent() else {
            return Ok(results::not_allowed().into());
        };
        // A call ends when the agent cancels it, holzi ends the connection or the server stops.
        let cancel = self.link.close.child_token();
        let agent_cancel = context.ct.clone();
        let guard = cancel.clone();
        let watcher = tokio::spawn(async move {
            agent_cancel.cancelled().await;
            guard.cancel();
        });
        let result = dispatch::call(
            &self.core,
            &agent,
            &self.link,
            request.name.as_ref(),
            request.arguments,
            self.was_seen(request.name.as_ref()),
            &cancel,
        )
        .await;
        watcher.abort();
        Ok(result.into())
    }
}
