//! A connected subscription (Claude Code, Codex) as the model of a job (spec 055 FR-032, R16).
//!
//! The agent runs with the access it has in haex-vault and haex-vault does not narrow it for a job: the user
//! has been told so when the model was granted. A job has no chat to ask in and no thread to keep
//! an audit in, so the agent runs without approvals (`Ungated`), not in a mode that would ask
//! nobody or deny every call (`GatedPermissive` denies when there is no thread to audit against).

use std::sync::Arc;

use async_trait::async_trait;

use crate::adapters::cli_delegate::autonomy::AutonomyMode;
use crate::adapters::cli_delegate::DelegateChatContext;
use crate::adapters::{AdapterError, AdapterStream, ChatRequest, ProviderAdapter, ProviderModel};
use crate::cli_tools::runtime::DelegateTools;
use crate::vault_gate::ChildRegistry;

/// The tools of the vault that is open when a job asks; looked up per job, because the source of
/// adapters outlives a vault session (spec 094).
pub(crate) type ToolsOfOpenVault = Arc<dyn Fn() -> Option<DelegateTools> + Send + Sync>;

/// What an agent that runs a job gets from haex-vault: no chat to approve in, no vault to read deny rules
/// from, and its processes registered with the vault so that closing it ends them.
pub(crate) fn job_context(
    children: ChildRegistry,
    tools: Option<DelegateTools>,
) -> DelegateChatContext {
    DelegateChatContext {
        pending_tool_approvals: Default::default(),
        emit: Arc::new(|_event: &str, _payload: serde_json::Value| {}),
        database: None,
        children,
        // A job has no chat to ask in, so it gets no haex-vault tools (spec 021 US6).
        haex: None,
        deny_rules: Vec::new(),
        tools,
    }
}

/// The request as the agent of a job gets it: full access and no conversation.
pub(crate) fn job_request(mut request: ChatRequest) -> ChatRequest {
    request.autonomy_mode = AutonomyMode::Ungated;
    request.thread_id = None;
    request
}

/// The adapter of a subscription for a job: it asks the agent the way [`job_request`] says.
pub(crate) struct JobAgent {
    inner: Box<dyn ProviderAdapter>,
}

impl JobAgent {
    pub(crate) fn new(inner: Box<dyn ProviderAdapter>) -> Self {
        Self { inner }
    }
}

#[async_trait]
impl ProviderAdapter for JobAgent {
    async fn list_models(&self) -> Result<Vec<ProviderModel>, AdapterError> {
        self.inner.list_models().await
    }

    async fn stream_chat(&self, req: ChatRequest) -> Result<AdapterStream, AdapterError> {
        self.inner.stream_chat(job_request(req)).await
    }
}
