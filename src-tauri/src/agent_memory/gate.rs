//! When a write of an agent asks the user (spec 097 FR-018, FR-019, research R7, contracts "When a
//! write asks"): one rule for the local agent and the Abo-Agents. A write without foreign content in
//! the conversation runs without a question in every chat mode but "Plan", which refuses it; after
//! foreign content it asks.

use std::collections::HashSet;

use crate::chat::tools::permission::PermissionMode;
use crate::mcp_server::grants::MemoryLevel;
use crate::storage::chat_messages::{ChatMessage, MessageRole};

tokio::task_local! {
    /// The page shown before an approved write, or `None` if it was a new page.
    pub(crate) static APPROVED_PAGE: Option<super::store::Page>;
    pub(crate) static CATALOG_LEVEL: MemoryLevel;
}

#[cfg(test)]
#[path = "gate_tests.rs"]
mod tests;

/// The verdict on a write of the scope `memory.write`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    Allow,
    Ask,
    Deny,
}

/// The tools of the scope `memory.write` (`memory.add`, `memory.write`, `memory.remove`).
pub const WRITE_TOOLS: [&str; 3] = ["memory_add", "memory_write", "memory_remove"];

/// Whether `tool_name` is a write of the memory, whose question this gate decides.
pub fn is_write_tool(tool_name: &str) -> bool {
    WRITE_TOOLS.contains(&tool_name)
}

/// Whether the model may see a built-in tool and the rows it produced.
pub(crate) fn visible_at(level: MemoryLevel, tool_name: &str) -> bool {
    match tool_name {
        "memory_open" | "memory_search" => !level.is_off(),
        name if is_write_tool(name) => level == MemoryLevel::ReadWrite,
        _ => true,
    }
}

/// Identifies both sides of an excluded historical tool call.
pub(crate) fn excluded_calls<'a>(
    history: impl IntoIterator<Item = &'a ChatMessage>,
    level: MemoryLevel,
) -> HashSet<uuid::Uuid> {
    history
        .into_iter()
        .filter(|row| {
            row.role == MessageRole::ToolCall
                && row.tool_source.as_deref() == Some("action")
                && row.tool_name.as_deref().is_some_and(|name| {
                    !visible_at(level, name)
                        || (name == crate::chat::tools::find_actions::FIND_ACTIONS_TOOL_NAME
                            && level != MemoryLevel::ReadWrite)
                })
        })
        .map(|row| row.id)
        .collect()
}

pub(crate) fn excluded_row(row: &ChatMessage, calls: &HashSet<uuid::Uuid>) -> bool {
    calls.contains(&row.id)
        || (row.role == MessageRole::ToolResult
            && row.parent_id.is_some_and(|parent| calls.contains(&parent)))
}

/// `foreign`: the conversation's `memory_foreign_at` is set.
pub fn decide(mode: PermissionMode, foreign: bool) -> Decision {
    match (mode, foreign) {
        (PermissionMode::Plan, _) => Decision::Deny,
        (_, true) => Decision::Ask,
        (_, false) => Decision::Allow,
    }
}

impl From<Decision> for crate::chat::tools::permission::Decision {
    fn from(decision: Decision) -> Self {
        match decision {
            Decision::Allow => Self::Allow,
            Decision::Ask => Self::Ask,
            Decision::Deny => Self::Deny,
        }
    }
}
