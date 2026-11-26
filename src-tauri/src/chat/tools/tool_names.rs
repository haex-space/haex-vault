//! The names of the tools a hosted model can load with `find_actions` (operator 2026-11-25, spec 097
//! results.md "Tool names in the prompt"): the first offer is a selection, and a model that does
//! not know what else exists may answer that it has no tool instead of searching. Only the names,
//! not the tools' schemas, so the prompt stays short; only for turns that do not route, since spec
//! 087 measured a hosted model and the small local ones reacting differently to sentences about
//! searching.

use super::action_tool::ACTION_SOURCE;
use super::ask_user::ASK_USER_TOOL_NAME;
use super::extension_offer::EXTENSION_SOURCE;
use super::offer::FIND_ACTIONS_TOOL_NAME;
use super::profile_gate::permits;
use super::{Tool, ToolRegistry};
use crate::agent_memory::gate::visible_at;
use crate::chat::turn::images::NEED_IMAGES;
use crate::mcp_server::grants::{MemoryLevel, Rights};

#[cfg(test)]
#[path = "tool_names_tests.rs"]
mod tests;

/// What the line says before the names.
pub const HEAD: &str = "HaexVault has these tools besides the offered ones; find_actions loads one when you search for its name:";

/// The line naming every tool of `registry` that a search can add and `keep` lets the turn use: the
/// actions that are not core and the extensions' tools, in the registry's order, so the line stays
/// the same for a conversation. `None` when there is none.
pub fn tool_names_line(
    registry: &ToolRegistry,
    keep: impl Fn(&dyn Tool) -> bool,
) -> Option<String> {
    let names: Vec<&str> = registry
        .iter()
        .map(|tool| tool.as_ref())
        .filter(|tool| searchable(*tool) && keep(*tool))
        .map(|tool| tool.name())
        .collect();
    (!names.is_empty()).then(|| format!("{HEAD} {}.", names.join(", ")))
}

/// The chat's extension overview followed by searchable names allowed by the profile, memory
/// level and model capabilities.
pub fn with_names(
    overview: Option<String>,
    registry: &ToolRegistry,
    rights: &Rights,
    level: MemoryLevel,
    accepts_images: bool,
) -> Option<String> {
    let names = tool_names_line(registry, |tool| {
        let name = tool.name();
        permits(rights, tool).is_ok()
            && visible_at(level, name)
            && (accepts_images || !NEED_IMAGES.contains(&name))
    });
    [overview, names]
        .into_iter()
        .flatten()
        .reduce(|overview, names| format!("{overview}\n\n{names}"))
}

fn searchable(tool: &dyn Tool) -> bool {
    match tool.source() {
        EXTENSION_SOURCE => tool.match_entry().is_some(),
        ACTION_SOURCE => {
            tool.name() != FIND_ACTIONS_TOOL_NAME
                && tool.name() != ASK_USER_TOOL_NAME
                && tool.action_definition().is_some_and(|def| !def.core)
        }
        _ => false,
    }
}
