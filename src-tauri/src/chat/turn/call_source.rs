//! Where an executed call's tool came from, as its rows and events record it: the source (`mcp`,
//! `cli`, `action`, `haextension`) and, for an extension's tool, its origin (spec 018, R8).

use crate::chat::tools::{Tool, ToolOrigin};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CallSource {
    pub(crate) source: &'static str,
    pub(crate) origin: Option<ToolOrigin>,
}

impl CallSource {
    pub(crate) fn of(tool: &dyn Tool) -> Self {
        Self {
            source: tool.source(),
            origin: tool.origin(),
        }
    }

    /// A tool no longer in the registry: `mcp` is the more plausible source (a server that
    /// disconnected), since `cli` never disappears.
    pub(crate) fn unknown() -> Self {
        Self {
            source: "mcp",
            origin: None,
        }
    }

    /// The origin as stored in `chat_messages.tool_origin`.
    pub(crate) fn origin_json(&self) -> Option<String> {
        self.origin
            .as_ref()
            .and_then(|origin| serde_json::to_string(origin).ok())
    }
}
