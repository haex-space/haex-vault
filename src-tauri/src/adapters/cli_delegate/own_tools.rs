//! Which of an Abo-Agent's own tools bring foreign content into the conversation (spec 097 FR-019,
//! research R7, `specs/097-agent-memory/analysis/codex-spike.md`). Like `agent_memory::foreign`,
//! the lists name what is own and treat everything else as foreign, so a tool new to haex-vault
//! makes a write ask once too often rather than once too rarely. haex-vault's own tools are judged
//! where their calls arrive (`mcp_server::dispatch`), not here.

use serde_json::Value;

use super::haex_mcp::SERVER_NAME;

/// Claude Code's tools that work on the run folder, which holds only the user's attachments, or on
/// the conversation itself. `Task` and `Agent` start a sub-agent, whose own tools are read too.
const CLAUDE_OWN: [&str; 8] = [
    "Read",
    "Glob",
    "Grep",
    "Edit",
    "Write",
    "TodoWrite",
    "Task",
    "Agent",
];

/// Codex's item types that are the conversation itself or work on the run folder. A sub-agent's
/// own items arrive as items of their own.
const CODEX_OWN: [&str; 14] = [
    "userMessage",
    "hookPrompt",
    "agentMessage",
    "plan",
    "reasoning",
    "contextCompaction",
    "fileChange",
    "imageView",
    "imageGeneration",
    "sleep",
    "enteredReviewMode",
    "exitedReviewMode",
    "collabAgentToolCall",
    "subAgentActivity",
];

/// Whether a `stream-json` line of Claude Code calls a tool that brings foreign content: a
/// `tool_use` block of an assistant message, also of a sub-agent, whose name is neither one of
/// [`CLAUDE_OWN`] nor a tool of haex-vault's server.
pub(super) fn claude_line_is_foreign(line: &str) -> bool {
    if !line.contains("\"tool_use\"") {
        return false;
    }
    let Ok(value) = serde_json::from_str::<Value>(line) else {
        return false;
    };
    if value["type"].as_str() != Some("assistant") {
        return false;
    }
    let haex = format!("mcp__{SERVER_NAME}__");
    value
        .pointer("/message/content")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|block| block["type"].as_str() == Some("tool_use"))
        .any(|block| {
            let name = block["name"].as_str().unwrap_or_default();
            !CLAUDE_OWN.contains(&name) && !name.starts_with(&haex)
        })
}

/// Whether a notification of Codex's app-server names an item that brings foreign content: a web
/// search, a command (whatever its exit code), a tool of another MCP server, and any item type
/// that is not one of [`CODEX_OWN`]. Judged at `item/started`, before any of its output, and again
/// at `item/completed`, for an item that reports only its end.
pub(super) fn codex_item_is_foreign(method: &str, params: &Value) -> bool {
    if method != "item/started" && method != "item/completed" {
        return false;
    }
    let item = &params["item"];
    match item["type"].as_str() {
        Some("mcpToolCall") => item["server"].as_str() != Some(SERVER_NAME),
        Some(kind) => !CODEX_OWN.contains(&kind),
        None => true,
    }
}
