//! Tool trait, registry, and the Manual/Auto/Plan approval gate.
//!
//! A [`Tool`] is anything the turn loop (`chat/commands.rs`) can hand to an
//! adapter as a [`crate::adapters::types::ToolSpec`] and later execute once
//! the model asks for it. The registry holds every tool from every source
//! (host-CLI, MCP) behind one `Arc<dyn Tool>` so the loop never branches on
//! where a tool came from (host-CLI, MCP, or an action of the app, spec 032).

pub mod action_bridge;
#[cfg(test)]
mod action_bridge_tests;
pub mod action_tool;
#[cfg(test)]
mod action_tool_tests;
pub mod agent_task;
pub mod ask_user;
#[cfg(test)]
mod ask_user_tests;
pub mod availability;
pub mod cli;
#[cfg(test)]
mod cli_tests;
pub mod find_actions;
pub mod mcp;
#[cfg(test)]
mod mcp_tests;
pub mod native_action;
#[cfg(test)]
mod native_action_tests;
pub mod offer;
#[cfg(test)]
mod offer_tests;
pub mod permission;
#[cfg(test)]
mod permission_tests;
pub mod prompt;
#[cfg(test)]
#[path = "prompt_tests.rs"]
mod prompt_tests;
pub mod selftest;

use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio_util::sync::CancellationToken;

/// How far a tool call reaches, which decides whether it may run without
/// confirmation. `Safe` only reads, `Change` alters state that the user can
/// change back (an action with effect `write`, spec 032), `Risky` is
/// everything else: destructive actions, shell commands and MCP tools. See
/// `permission::decide` for the full Manual/Auto/Plan matrix.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RiskClass {
    Safe,
    Change,
    Risky,
}

/// Outcome of [`Tool::execute`]. Never a `Result` — a failed tool call is a
/// normal conversational outcome (spec.md: the model sees the error and can
/// react to it), not a turn-ending error.
#[derive(Debug, Clone)]
pub struct ToolResult {
    pub content: String,
    pub is_error: bool,
    /// Set when the call needs the user to choose first (spec 046): the tool round asks in the
    /// chat and replaces this result; `content` is what the model would see if nobody asked.
    pub choice: Option<ChoiceRequest>,
    /// Images for the model (spec 044 research R2): the tool round sends them along when the model
    /// accepts images, else a note in their place; the history keeps only a note.
    pub images: Vec<ToolImage>,
}

/// An image a tool hands to the model, ready to send (`files.read`: a JPEG within the limits).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolImage {
    pub name: String,
    pub media_type: String,
    pub bytes: Vec<u8>,
}

impl ToolResult {
    pub fn ok(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            is_error: false,
            choice: None,
            images: Vec::new(),
        }
    }

    pub fn error(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            is_error: true,
            choice: None,
            images: Vec::new(),
        }
    }

    /// A result that asks the user to choose before the call can finish.
    pub fn needs_choice(content: impl Into<String>, choice: ChoiceRequest) -> Self {
        Self {
            content: content.into(),
            is_error: true,
            choice: Some(choice),
            images: Vec::new(),
        }
    }

    pub fn with_images(mut self, images: Vec<ToolImage>) -> Self {
        self.images = images;
        self
    }
}

/// A question the tool round puts to the user (spec 046, data-model.md): asked by an action that
/// could not resolve `field` (then the answer runs the action again) or by the agent itself
/// (`ask_user`, then the answer is the result).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChoiceRequest {
    /// The agent's own question; `None` for an action, whose question the chat words itself.
    pub question: Option<String>,
    /// The input field of the action that gets the answer; `None` for `ask_user`.
    pub field: Option<String>,
    /// What was asked for (`"haex"`); empty for `ask_user`.
    pub value: String,
    pub options: Vec<ChoiceOption>,
}

/// One proposed answer of a [`ChoiceRequest`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChoiceOption {
    pub value: String,
    pub label: String,
    /// Why it cannot be picked right now; the chat shows it disabled.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unavailable: Option<String>,
}

/// Where a tool of an extension comes from (spec 018, research R8): shown with its calls and kept
/// in the history, so the history stays readable after the extension is removed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolOrigin {
    pub extension_id: String,
    pub extension_name: String,
    pub tool_title_de: String,
    pub tool_title_en: String,
    /// A development version of the extension.
    pub dev: bool,
}

/// One callable tool, regardless of source (host-CLI, MCP). `execute` takes
/// the already-parsed JSON input the model produced and never panics —
/// execution failures (bad input, a crashed process, an unreachable MCP
/// server) come back as `ToolResult { is_error: true, .. }`.
#[async_trait]
pub trait Tool: Send + Sync {
    /// Unique within the registry for a turn. For a disambiguated MCP tool
    /// this is the prefixed `registry_name` (data-model.md), not the raw
    /// name the server reported.
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    /// `mcp`, `cli` or `action` — persisted verbatim into `chat_messages.tool_source`
    /// (data-model.md).
    fn source(&self) -> &'static str;
    /// JSON-Schema-shaped object, reused as-is for both the Anthropic
    /// `input_schema` and mistralrs' `Function.parameters` (research.md
    /// §1/§2 — both are JSON-Schema-shaped).
    fn input_schema(&self) -> Value;
    fn risk_class(&self) -> RiskClass;
    /// Returns the frontend definition when this tool is a built-in action.
    /// Other tool sources leave it absent so offer construction can preserve
    /// them without downcasting trait objects.
    fn action_definition(&self) -> Option<&action_tool::AgentActionDef> {
        None
    }
    /// The extension a tool comes from (spec 018, research R8); `None` for holzi's own tools.
    fn origin(&self) -> Option<ToolOrigin> {
        None
    }
    /// Every call needs the user's consent, in every mode, "Plan" included (spec 018: a tool the
    /// user confirmed with "ask").
    fn always_ask(&self) -> bool {
        false
    }
    /// `cancel` fires when `abort_current_generation` cancels the turn
    /// this call belongs to (T032). Implementations that own a cancellable
    /// resource (a child process, an MCP request) MUST race it against
    /// `cancel.cancelled()` and tear that resource down on the spot —
    /// dropping the returned future alone is not enough to, e.g., reap a
    /// child process without leaving a zombie.
    async fn execute(&self, input: Value, cancel: CancellationToken) -> ToolResult;
}

/// Every tool available this turn, from every source. Built once at
/// `ChatState` construction (host-CLI) and refreshed as MCP servers
/// (re)connect (T019). Tools are `Arc`-shared rather than owned outright so
/// the turn loop can clone one out and run its (possibly slow) `execute()`
/// without holding `ChatState.tool_registry`'s lock for the duration.
#[derive(Default)]
pub struct ToolRegistry {
    tools: Vec<Arc<dyn Tool>>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self { tools: Vec::new() }
    }

    /// Registers a tool. If `name()` collides with an already-registered
    /// tool, the newly-registered one wins the slot under its own name.
    pub fn register(&mut self, tool: Arc<dyn Tool>) {
        if let Some(existing) = self
            .tools
            .iter_mut()
            .find(|existing| existing.name() == tool.name())
        {
            *existing = tool;
        } else {
            self.tools.push(tool);
        }
    }

    /// Drops every tool of one source. `set_agent_actions` replaces all tools of the source
    /// `action` this way and leaves the host-CLI and MCP tools alone (spec 032).
    pub fn remove_source(&mut self, source: &str) {
        self.tools.retain(|tool| tool.source() != source);
    }

    /// Drops every currently-registered tool. Used before an MCP
    /// reconnect repopulates the registry (T019).
    pub fn clear(&mut self) {
        self.tools.clear();
    }

    pub fn get(&self, name: &str) -> Option<Arc<dyn Tool>> {
        self.tools.iter().find(|t| t.name() == name).cloned()
    }

    pub fn iter(&self) -> impl Iterator<Item = &Arc<dyn Tool>> {
        self.tools.iter()
    }

    pub fn action_defs(&self) -> Vec<action_tool::AgentActionDef> {
        self.tools
            .iter()
            .filter_map(|tool| tool.action_definition().cloned())
            .collect()
    }
}

/// Resolution of one `tool-permission-request` (contracts/tauri-commands.md
/// §respond_tool_permission). Distinct from `permission::Decision`, which is
/// the gate's own Allow/Ask/Deny verdict before a human is ever asked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApprovalDecision {
    Allow,
    Deny,
}
