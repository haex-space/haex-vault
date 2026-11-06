//! One MCP session on the HTTP way (spec 021 US1, US7, US8, T093). rmcp serves the session; the
//! HTTP layer (`http.rs`) has checked the access token and put its agent on every request. The
//! session takes that agent at `notifications/initialized` or the first request, whichever holzi
//! handles first, joins the open connections as the program way does (`attach.rs`), and answers
//! only requests that carry the same agent. When holzi
//! ends the link (revocation, access off, vault closed) rmcp closes the session.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use hyper::http::request::Parts;
use rmcp::model::{
    CallToolRequestParams, CallToolResponse, Extensions, ListToolsResult, PaginatedRequestParams,
    ProtocolVersion, ServerConfig,
};
use rmcp::service::{NotificationContext, Peer, RequestContext, RoleServer};
use rmcp::transport::streamable_http_server::session::local::LocalSessionManager;
use rmcp::transport::streamable_http_server::{SessionId, SessionManager};
use rmcp::{ErrorData as McpError, ServerHandler};
use std::borrow::Cow;
use tokio::sync::OnceCell;
use tokio::task::JoinHandle;
use uuid::Uuid;

use super::attach;
use super::live::{Link, LiveGuard};
use super::server::{Core, Notice};
use super::session::{Session, INSTRUCTIONS_REGISTERED};

/// The agent of a request's access token, put on the request by `http.rs`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HttpAgent(pub Uuid);

/// The header rmcp names a session by.
const SESSION_HEADER: &str = "mcp-session-id";

fn agent_of(extensions: &Extensions) -> Option<Uuid> {
    extensions
        .get::<Parts>()
        .and_then(|parts| parts.extensions.get::<HttpAgent>())
        .map(|agent| agent.0)
}

struct Joined {
    agent: Uuid,
    _live: LiveGuard,
    tasks: Vec<JoinHandle<()>>,
}

/// Counts the sessions alive; the factory refuses more than the limit (contracts/http-auth.md).
#[derive(Clone, Default)]
pub struct SessionCount(Arc<AtomicUsize>);

impl SessionCount {
    pub fn get(&self) -> usize {
        self.0.load(Ordering::SeqCst)
    }

    /// One more session, or `None` at `max`.
    fn take(&self, max: usize) -> Option<SessionSlot> {
        let mut open = self.0.load(Ordering::SeqCst);
        loop {
            if open >= max {
                return None;
            }
            match self
                .0
                .compare_exchange(open, open + 1, Ordering::SeqCst, Ordering::SeqCst)
            {
                Ok(_) => return Some(SessionSlot(Arc::clone(&self.0))),
                Err(now) => open = now,
            }
        }
    }
}

struct SessionSlot(Arc<AtomicUsize>);

impl Drop for SessionSlot {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

pub struct HttpSession {
    core: Arc<Core>,
    session: Session,
    link: Arc<Link>,
    manager: Arc<LocalSessionManager>,
    /// Set once: the joined agent, or `None` when the first message carried no agent.
    joined: OnceCell<Option<Joined>>,
    _slot: SessionSlot,
}

impl HttpSession {
    /// A session for rmcp's factory; `Err` when `max` sessions are open.
    pub fn new(
        core: &Arc<Core>,
        manager: &Arc<LocalSessionManager>,
        count: &SessionCount,
        max: usize,
    ) -> std::io::Result<Self> {
        let slot = count
            .take(max)
            .ok_or_else(|| std::io::Error::other("too many MCP sessions"))?;
        let link = Arc::new(Link::new(core.token.child_token()));
        Ok(Self {
            core: Arc::clone(core),
            session: Session::new(Arc::clone(core), None, Arc::clone(&link)),
            link,
            manager: Arc::clone(manager),
            joined: OnceCell::new(),
            _slot: slot,
        })
    }

    /// The agent this session belongs to. The first message joins: rmcp answers
    /// `notifications/initialized` before its handler ran, so a client that lists its tools right
    /// away must not overtake the join.
    async fn joined_agent(&self, extensions: &Extensions, peer: &Peer<RoleServer>) -> Option<Uuid> {
        self.joined
            .get_or_init(|| self.join(extensions, peer.clone()))
            .await
            .as_ref()
            .map(|joined| joined.agent)
    }

    /// Whether the request carries the agent this session belongs to.
    async fn same_agent(&self, context: &RequestContext<RoleServer>) -> bool {
        let joined = self.joined_agent(&context.extensions, &context.peer).await;
        matches!((joined, agent_of(&context.extensions)), (Some(joined), Some(agent)) if joined == agent)
    }

    async fn join(&self, extensions: &Extensions, peer: Peer<RoleServer>) -> Option<Joined> {
        let Some(agent) = agent_of(extensions) else {
            self.link.end(super::hello::CloseReason::Revoked);
            return None;
        };
        let session_id: Option<SessionId> = extensions
            .get::<Parts>()
            .and_then(|parts| parts.headers.get(SESSION_HEADER))
            .and_then(|value| value.to_str().ok())
            .map(Into::into);
        let (live, tables) = attach::join(
            &self.core,
            &self.session,
            &peer,
            &self.link,
            Some(agent),
            None,
        )
        .await;
        let watcher = tokio::spawn(attach::tell_catalog_changes(
            Arc::clone(&self.core),
            self.session.clone(),
            peer,
            tables,
        ));
        let mut tasks = vec![watcher];
        if let Some(session_id) = session_id {
            let (close, manager) = (self.link.close.clone(), Arc::clone(&self.manager));
            tasks.push(tokio::spawn(async move {
                close.cancelled().await;
                let _ = manager.close_session(&session_id).await;
            }));
        }
        Some(Joined {
            agent,
            _live: live,
            tasks,
        })
    }
}

impl Drop for HttpSession {
    fn drop(&mut self) {
        let Some(joined) = self.joined.take().flatten() else {
            return;
        };
        for task in &joined.tasks {
            task.abort();
        }
        let agent = joined.agent;
        drop(joined);
        if !self.core.live.connected(agent) {
            (self.core.notify)(Notice::Disconnected(agent));
        }
    }
}

impl ServerHandler for HttpSession {
    fn get_info(&self) -> ServerConfig {
        // Only an agent with a valid access token reaches a session: it is registered.
        self.session
            .get_info()
            .with_instructions(INSTRUCTIONS_REGISTERED)
    }

    /// Versions with `initialize` only: a session needs the handshake to join `live`.
    fn supported_protocol_versions(&self) -> Cow<'static, [ProtocolVersion]> {
        Cow::Borrowed(ProtocolVersion::known_up_to(
            &ProtocolVersion::LATEST_WITH_INITIALIZE,
        ))
    }

    async fn on_initialized(&self, context: NotificationContext<RoleServer>) {
        self.joined_agent(&context.extensions, &context.peer).await;
    }

    async fn list_tools(
        &self,
        request: Option<PaginatedRequestParams>,
        context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, McpError> {
        if !self.same_agent(&context).await {
            return Ok(ListToolsResult::default());
        }
        self.session.list_tools(request, context).await
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, McpError> {
        if !self.same_agent(&context).await {
            return Ok(super::results::not_allowed().into());
        }
        self.session.call_tool(request, context).await
    }
}
