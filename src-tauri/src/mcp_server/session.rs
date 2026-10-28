//! One agent's MCP session (spec 021, contracts/mcp-surface.md): the server handler rmcp drives for
//! one connection. Until the agent is registered it sees no tool and every call is refused; after
//! the registration its rights decide both the list and each call.

use std::future::Future;
use std::sync::{Arc, PoisonError, RwLock};

use rmcp::model::{
    CallToolRequestParams, CallToolResponse, Implementation, InitializeResult, ListToolsResult,
    PaginatedRequestParams, ServerCapabilities, ServerConfig,
};
use rmcp::service::{MaybeSendFuture, NotificationContext, RequestContext, RoleServer};
use rmcp::{ErrorData as McpError, ServerHandler};
use tokio::sync::watch;
use uuid::Uuid;

use super::dispatch;
use super::grants::{self, Rights};
use super::registry::AgentMode;
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
}

impl Session {
    pub fn new(core: Arc<Core>, agent: Option<SessionAgent>) -> Self {
        let (initialized, _) = watch::channel(false);
        Self {
            core,
            agent: Arc::new(RwLock::new(agent)),
            initialized: Arc::new(initialized),
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

    fn tools(&self) -> Vec<rmcp::model::Tool> {
        let Some(agent) = self.agent() else {
            return Vec::new();
        };
        let catalog = self.core.catalog.snapshot();
        grants::offers(&agent.rights, &catalog)
            .into_iter()
            .map(dispatch::tool_of)
            .collect()
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

    fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> impl Future<Output = Result<ListToolsResult, McpError>> + MaybeSendFuture + '_ {
        std::future::ready(Ok(ListToolsResult {
            tools: self.tools(),
            ..Default::default()
        }))
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, McpError> {
        let Some(agent) = self.agent() else {
            return Ok(dispatch::not_allowed().into());
        };
        // A call ends when the agent cancels it or the server stops.
        let cancel = self.core.token.child_token();
        let agent_cancel = context.ct.clone();
        let guard = cancel.clone();
        let watcher = tokio::spawn(async move {
            agent_cancel.cancelled().await;
            guard.cancel();
        });
        let result = dispatch::call(
            &self.core,
            &agent,
            request.name.as_ref(),
            request.arguments,
            &cancel,
        )
        .await;
        watcher.abort();
        Ok(result.into())
    }
}
