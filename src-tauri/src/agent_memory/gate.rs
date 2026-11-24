//! When a write of an agent asks the user (spec 097 FR-018, FR-019, research R7, contracts "When a
//! write asks"): one rule for the local agent and the Abo-Agents. A write without foreign content in
//! the conversation runs without a question in every chat mode but "Plan", which refuses it; after
//! foreign content it asks.

use crate::chat::tools::permission::PermissionMode;

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
