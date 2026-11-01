//! Whether a turn offers tools, and what the person is told about it (spec 032 US4, data-model.md
//! §6, research R11/R14). Derived per turn from the kind of provider and the model's `tool_use`
//! capability; never stored.

use serde::Serialize;
use uuid::Uuid;

use crate::mcp_server::registry::AgentKind;
use crate::model_capabilities::{ModelCapabilities, ToolSupport};
use crate::storage::providers::ProviderKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ToolAvailability {
    /// The model is known to call tools.
    Offered,
    /// Nothing is known about the model: tools are offered with a hint.
    OfferedUnverified,
    /// The model cannot call tools: it chats without them.
    Unsupported,
    /// A CLI delegate runs its own agent; holzi's tools reach it only over MCP (spec 021).
    Delegate,
}

impl ToolAvailability {
    pub fn of(kind: ProviderKind, capabilities: Option<&ModelCapabilities>) -> Self {
        if kind == ProviderKind::CliDelegate {
            return Self::Delegate;
        }
        match capabilities.and_then(|c| c.tool_use).map(|t| t.support) {
            Some(ToolSupport::Supported) => Self::Offered,
            Some(ToolSupport::Unsupported) => Self::Unsupported,
            None => Self::OfferedUnverified,
        }
    }

    pub fn offers_tools(self) -> bool {
        matches!(self, Self::Offered | Self::OfferedUnverified)
    }
}

/// Payload of `chat-tool-availability`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolAvailabilityEvent {
    pub thread_id: Uuid,
    pub state: ToolAvailability,
    /// Which delegate runs, for a `Delegate` turn: the chat asks for its MCP rights (spec 021 US6).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delegate_kind: Option<AgentKind>,
}

#[cfg(test)]
#[path = "availability_tests.rs"]
mod availability_tests;
