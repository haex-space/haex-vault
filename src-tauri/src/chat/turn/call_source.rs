//! Where an executed call's tool came from, as its rows and events record it: the source (`mcp`,
//! `cli`, `action`, `haextension`) and, for an extension's tool, its origin (spec 018, R8).

use std::cell::Cell;

use serde_json::Value;
use tokio_util::sync::CancellationToken;

use crate::agent_memory::foreign::CONTACT_SHARE;
use crate::chat::tools::{Tool, ToolOrigin, ToolResult};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CallSource {
    pub(crate) source: &'static str,
    pub(crate) origin: Option<ToolOrigin>,
    /// The scope of one of haex-vault's own actions (spec 097: what counts as foreign content).
    pub(crate) scope: Option<String>,
    /// Whether execution reached the tool. An executed tool can return foreign text as an error.
    pub(crate) ran: bool,
    pub(crate) contact_share: bool,
}

impl CallSource {
    pub(crate) fn of(tool: &dyn Tool) -> Self {
        Self {
            source: tool.source(),
            origin: tool.origin(),
            scope: tool.action_definition().map(|def| def.scope.clone()),
            ran: false,
            contact_share: false,
        }
    }

    /// A tool no longer in the registry: `mcp` is the more plausible source (a server that
    /// disconnected), since `cli` never disappears.
    pub(crate) fn unknown() -> Self {
        Self {
            source: "mcp",
            origin: None,
            scope: None,
            ran: false,
            contact_share: false,
        }
    }

    /// The origin as stored in `chat_messages.tool_origin`.
    pub(crate) fn origin_json(&self) -> Option<String> {
        self.origin
            .as_ref()
            .and_then(|origin| serde_json::to_string(origin).ok())
    }

    pub(crate) async fn run(
        &mut self,
        tool: &dyn Tool,
        input: Value,
        cancel: CancellationToken,
    ) -> ToolResult {
        self.ran = true;
        let (result, contact_share) = CONTACT_SHARE
            .scope(Cell::new(false), async {
                let result = tool.execute(input, cancel).await;
                (result, CONTACT_SHARE.with(Cell::get))
            })
            .await;
        self.contact_share |= contact_share;
        result
    }
}
