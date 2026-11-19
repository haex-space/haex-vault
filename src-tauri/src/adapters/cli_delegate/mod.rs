//! `cli_delegate` provider adapters (spec 007-cli-delegate) — Claude Code
//! (`claude -p`) and Codex (`codex app-server --stdio`) as chat backends
//! that reuse an existing subscription instead of a metered API key.
//!
//! Unlike [`super::anthropic::AnthropicAdapter`]/[`super::local`], a
//! [`CliDelegateAdapter`] drives an external CLI subprocess to
//! completion per [`super::ProviderAdapter::stream_chat`] call and
//! streams its answer back as plain [`super::StreamChunk::Delta`]/
//! [`super::StreamChunk::Done`] — it never emits `ToolCalls`, since the
//! delegate performs its own tool use internally (research.md §4). That
//! internal tool use still needs to cross holzi's *existing* approval
//! gate (`chat/tools/permission.rs`, `ChatState.pending_tool_approvals`)
//! so Manual/Auto/Plan apply identically to a delegate as to any other
//! backend — hence this module's otherwise-unusual dependency on
//! `crate::chat::tools::ApprovalDecision` (data-model.md
//! "approval_bridge.rs").

pub mod approval_bridge;
#[cfg(test)]
mod approval_bridge_tests;
pub mod autonomy;
mod claude;
#[cfg(test)]
mod claude_tests;
mod codex;
mod codex_models;
pub(crate) use codex_models::LEGACY_MODEL_ID;
#[cfg(test)]
mod codex_tests;
pub mod connect_claude;
#[cfg(test)]
mod connect_claude_tests;
pub mod connect_codex;
#[cfg(test)]
mod connect_codex_tests;
pub mod haex_mcp;
#[cfg(test)]
mod haex_mcp_tests;
#[cfg(test)]
#[path = "mod_tests.rs"]
mod mod_tests;
pub(crate) mod permission_mcp_server;
mod process;
#[cfg(test)]
mod process_tests;
mod subagents;
#[cfg(test)]
mod subagents_tests;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use tokio::sync::oneshot;
use uuid::Uuid;

use super::{AdapterError, AdapterStream, ChatRequest, ChatRole, ProviderAdapter, ProviderModel};
use crate::chat::tools::ApprovalDecision;
use crate::cli_tools::runtime::{DelegateTools, RuntimeSource};
use crate::vault_gate::{ChildRegistry, VaultDb};

/// Builds the plain-text transcript both `claude -p` and `codex`'s
/// `turn/start` receive as their conversation input. Holzi remains the
/// sole source of conversation history for either backend — neither
/// vendor's own session persistence has anything to resume under a
/// fresh, isolated `CLAUDE_CONFIG_DIR`/`CODEX_HOME` per invocation
/// (research.md §3), so relying on it would find nothing even if used.
pub(super) fn build_transcript_prompt(req: &ChatRequest) -> String {
    let mut prompt = String::new();
    let mut attachment_index = 0;
    for message in &req.messages {
        match &message.role {
            ChatRole::User => {
                prompt.push_str("Human: ");
                prompt.push_str(&message.content);
                // Attachment bytes are written into the invocation's own
                // working directory (spawn_claude_invocation) — mention
                // each by its in-sandbox filename so Claude Code's own
                // Read tool can find it (research.md §3).
                for attachment in &message.attachments {
                    prompt.push_str(&format!(
                        "\n[Attached file: attachment-{attachment_index}-{}]",
                        attachment.name
                    ));
                    attachment_index += 1;
                }
                prompt.push_str("\n\n");
            }
            ChatRole::Assistant => {
                prompt.push_str("Assistant: ");
                prompt.push_str(&message.content);
                prompt.push_str("\n\n");
            }
            // A prior turn in this thread may have been answered by a
            // different backend (spec.md Edge Cases: switching backends
            // mid-conversation) — summarize rather than silently drop.
            ChatRole::ToolCall { name, .. } => {
                prompt.push_str(&format!("[Assistant used a tool: {name}]\n\n"));
            }
            ChatRole::ToolResult { is_error, .. } => {
                let outcome = if *is_error { "failed" } else { "succeeded" };
                prompt.push_str(&format!("[Tool use {outcome}]\n\n"));
            }
        }
    }
    prompt
}

/// Shared shape of `ChatState.pending_tool_approvals` (`chat/session.rs`),
/// reused verbatim rather than re-declared so the type stays in lockstep
/// with the built-in tool loop's own map.
pub type PendingToolApprovals = Arc<Mutex<HashMap<Uuid, oneshot::Sender<ApprovalDecision>>>>;

/// Emits a named event with a JSON-serializable payload. In production
/// this is a thin wrapper around `AppHandle::emit` (`chat/model_loading.rs`
/// builds the real one); tests substitute a plain closure instead of
/// needing a live Tauri app — the same shape
/// `tests/common/tool_loop_fixture.rs` already uses for the built-in
/// tool loop's own event emission, kept consistent here rather than
/// introducing a second, `AppHandle`-only pattern this codebase's tests
/// have no way to drive.
pub type EventEmitter = Arc<dyn Fn(&str, serde_json::Value) + Send + Sync>;

/// What `build_adapter` needs, beyond the provider row itself, to
/// construct a `CliDelegateAdapter` capable of real `stream_chat` (not
/// just `list_models`) — see `CliDelegateAdapter`'s own doc comment.
#[derive(Clone)]
pub struct DelegateChatContext {
    pub pending_tool_approvals: PendingToolApprovals,
    pub emit: EventEmitter,
    pub database: Option<VaultDb>,
    /// Where the delegated CLI's process registers, so ending the vault ends it.
    pub children: ChildRegistry,
    /// holzi's own MCP server for a registered delegate (spec 021 US6); `None` for a job.
    pub haex: Option<Arc<dyn haex_mcp::HaexAccess>>,
    /// What the permissive posture refuses for this turn: the deny rules of the agent profile of the
    /// model (spec 083 FR-030). Set per turn from the request; empty in a context built before.
    pub deny_rules: Vec<autonomy::DenyCategory>,
    /// The tools installed in the open vault (spec 094). A context without them can list models
    /// but cannot start a tool: `stream_chat` ends with "not installed in this vault".
    pub tools: Option<DelegateTools>,
}

/// Which subscription vendor a `cli_delegate` provider row talks to.
/// Parsed from `providers.adapter` (`storage/providers.rs`), mirroring
/// [`crate::storage::providers::ProviderKind`]'s own `parse`/`as_str`
/// pattern.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DelegateVendor {
    Claude,
    Codex,
}

impl DelegateVendor {
    /// Returns the stable provider discriminator used in storage and executable defaults.
    pub fn as_str(self) -> &'static str {
        match self {
            DelegateVendor::Claude => "claude",
            DelegateVendor::Codex => "codex",
        }
    }

    /// Parses a stored provider discriminator into a supported delegate vendor.
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "claude" => Some(DelegateVendor::Claude),
            "codex" => Some(DelegateVendor::Codex),
            _ => None,
        }
    }
}

/// `ProviderAdapter` implementation for a `cli_delegate` provider row.
/// Constructed once per model load (`build_adapter`, `providers/mod.rs`)
/// and held as `Arc<dyn ProviderAdapter>` on the active session, exactly
/// like `AnthropicAdapter`/the local llama.cpp adapter.
///
/// The chat context is absent when built from a refresh-only path
/// (`providers/mod.rs::do_refresh`) and present when loaded for a real chat
/// turn (`chat/model_loading.rs::load_api_key_model`).
pub struct CliDelegateAdapter {
    vendor: DelegateVendor,
    /// Decrypted `providers.credentials`: a Claude OAuth token (UTF-8
    /// bytes) or a Codex `auth.json` file's raw bytes, depending on
    /// `vendor`.
    credentials: Vec<u8>,
    /// Where the vault's installation of the tool is found; resolved at each start, so there is
    /// no host lookup and no binary name in the provider row (spec 094 FR-003).
    runtime: RuntimeSource,
    chat_context: Option<DelegateChatContext>,
    /// The provider row this adapter talks to: with the remote id of a request it names the model
    /// (`<provider id>:<remote id>`) whose agent profile limits what the delegate may do with
    /// holzi's tools (spec 083 FR-028).
    provider_id: Option<Uuid>,
}

impl CliDelegateAdapter {
    /// Creates an adapter for a connected CLI delegate provider.
    pub fn new(
        vendor: DelegateVendor,
        credentials: Vec<u8>,
        runtime: impl Into<RuntimeSource>,
        chat_context: Option<DelegateChatContext>,
    ) -> Self {
        Self {
            vendor,
            credentials,
            runtime: runtime.into(),
            chat_context,
            provider_id: None,
        }
    }

    /// The adapter of the provider row `provider_id`.
    pub fn for_provider(mut self, provider_id: Uuid) -> Self {
        self.provider_id = Some(provider_id);
        self
    }
}

/// Anthropic's public API base URL. A `cli_delegate` provider row has no address of its own, so
/// the models-API lookup below uses this fixed constant.
const CLAUDE_API_BASE_URL: &str = "https://api.anthropic.com";

/// Decodes and validates a `cli_delegate` provider's stored Claude OAuth
/// token: UTF-8, trimmed, and non-empty. Shared by `list_models`'s model
/// catalog refresh and `claude::spawn_claude_invocation`'s real chat turn
/// so both reject a blank/invalid token the same way instead of drifting
/// if one copy's validation changes and the other doesn't.
fn decode_claude_oauth_token(credentials: &[u8]) -> Result<String, AdapterError> {
    let token = std::str::from_utf8(credentials)
        .map_err(|_| AdapterError::InvalidCredentials)?
        .trim()
        .to_string();
    if token.is_empty() {
        return Err(AdapterError::InvalidCredentials);
    }
    Ok(token)
}

/// Fetches Claude's live model catalog via Anthropic's `/v1/models` API
/// (the same endpoint and pagination `AnthropicAdapter` uses for
/// `api_key` providers — see `anthropic::fetch_models`), authenticated
/// with the OAuth access token `claude setup-token` issued instead of a
/// metered API key. A connected delegate therefore always reflects
/// whatever models the subscription currently has, with no hardcoded
/// list to fall out of date. A free function (rather than inlined into
/// `list_models`) so it is directly unit-testable against a wiremock
/// server without needing a full `CliDelegateAdapter`.
async fn fetch_claude_models(
    base_url: &str,
    oauth_token: &str,
) -> Result<Vec<ProviderModel>, AdapterError> {
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(15))
        .user_agent(concat!("holzi/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|e| AdapterError::Http {
            reason: format!("build reqwest client: {e}"),
        })?;
    crate::adapters::anthropic::fetch_models(
        &client,
        base_url,
        &crate::adapters::anthropic::ModelsAuth::OAuthBearer(oauth_token.to_string()),
    )
    .await
}

#[async_trait]
impl ProviderAdapter for CliDelegateAdapter {
    /// Claude's model list comes straight from Anthropic's own API (see
    /// `fetch_claude_models`) rather than a hardcoded list. Codex's comes from its own program
    /// (`codex_models`).
    async fn list_models(&self) -> Result<Vec<ProviderModel>, AdapterError> {
        match self.vendor {
            DelegateVendor::Claude => {
                let token = decode_claude_oauth_token(&self.credentials)?;
                fetch_claude_models(CLAUDE_API_BASE_URL, &token).await
            }
            DelegateVendor::Codex => {
                // Asked of the program itself (`model/list`), so the list is what the account
                // offers now. Without the program in the vault there is no list: a refresh then
                // fails and the cached one stays.
                let runtime =
                    self.runtime
                        .resolve()
                        .map_err(|error| AdapterError::Unavailable {
                            reason: error.reason(),
                        })?;
                codex_models::list_models(runtime, self.credentials.clone()).await
            }
        }
    }

    async fn stream_chat(&self, req: ChatRequest) -> Result<AdapterStream, AdapterError> {
        process::ensure_available(crate::platform::capabilities().cli_delegates)?;
        // The installation is looked up now, so the version is the one in place when this run
        // starts (spec FR-022). A missing or unfitting one ends the run before anything starts.
        let runtime = self
            .runtime
            .resolve()
            .map_err(|error| AdapterError::Unavailable {
                reason: error.reason(),
            })?;
        runtime
            .record
            .append("run_started", &[("vendor", &runtime.vendor.vendor)]);
        let context = self
            .chat_context
            .clone()
            .ok_or_else(|| AdapterError::Http {
                reason: "cli_delegate adapter built without chat context (list_models-only \
                         construction)"
                    .into(),
            })?;
        let context = DelegateChatContext {
            deny_rules: req.autonomy_deny.clone(),
            ..context
        };
        // The id the model has as a subject of the agent profiles.
        let model = match self.provider_id {
            Some(provider) => format!("{provider}:{}", req.model_id),
            None => req.model_id.clone(),
        };
        match self.vendor {
            DelegateVendor::Claude => {
                claude::spawn_claude_invocation(
                    runtime,
                    self.credentials.clone(),
                    req,
                    context,
                    model,
                )
                .await
            }
            DelegateVendor::Codex => {
                codex::spawn_codex_app_server(
                    runtime,
                    self.credentials.clone(),
                    req,
                    context,
                    model,
                )
                .await
            }
        }
    }

    fn delegate_vendor(&self) -> Option<DelegateVendor> {
        Some(self.vendor)
    }
}
