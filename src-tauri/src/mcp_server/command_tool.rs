//! holzi's command tool for external agents (spec 021 US5, FR-012, research R3): `run_command` of
//! the chat (spec 003) with the same name, description and schema, offered only with the group
//! `commands` and only where the device runs commands (`PlatformCapabilities.command_tool`). It is
//! `Risky`, so every command asks in Auto and is refused in Plan. Its command line reaches the prompt
//! and the log only through `redact.rs`, which hides the credentials in it.

use std::sync::Arc;

use rmcp::model::CallToolResult;
use serde_json::json;

use super::agent_log::Outcome;
use super::grants::Rights;
use super::prompts::CallTitles;
use super::results::{error_result, unavailable};
use crate::chat::tools::cli::CliTool;
use crate::chat::tools::{Tool, ToolResult};
use crate::vault_gate::ChildRegistry;

/// What the command tool answers when it was stopped before the command ended.
const CANCELLED: &str = "tool_call_cancelled";

/// The command tool of this device, as the chat registers it (`chat::session::host_tools`); `None`
/// where it runs no commands (spec 043 FR-026). Its shells end with the vault, like the chat's.
pub fn for_device(children: &ChildRegistry) -> Option<Arc<dyn Tool>> {
    crate::platform::capabilities()
        .command_tool
        .then(|| Arc::new(CliTool::new(children.clone())) as Arc<dyn Tool>)
}

/// The command tool, if `rights` open it; `tool` is `None` where the device runs no commands.
pub fn offer<'a>(rights: &Rights, tool: Option<&'a Arc<dyn Tool>>) -> Option<&'a Arc<dyn Tool>> {
    tool.filter(|_| rights.commands)
}

/// The command tool behind `tool_name`, if the agent may call it; checked again on every call.
pub fn check<'a>(
    rights: &Rights,
    tool: Option<&'a Arc<dyn Tool>>,
    tool_name: &str,
) -> Option<&'a Arc<dyn Tool>> {
    offer(rights, tool).filter(|tool| tool.name() == tool_name)
}

/// What the call prompt names it.
pub fn titles() -> CallTitles {
    CallTitles {
        de: "Befehl ausführen".to_owned(),
        en: "Run a command".to_owned(),
    }
}

/// The command's answer as the agent sees it: its output; a command that failed keeps its output in
/// the message, and a stop is `unavailable`.
pub fn result(result: ToolResult) -> (CallToolResult, Outcome) {
    if !result.is_error {
        let output = json!({ "output": result.content });
        return (CallToolResult::structured(output), Outcome::Ok);
    }
    if result.content == CANCELLED {
        return (unavailable(), Outcome::Unavailable);
    }
    (
        error_result("command_failed", &result.content, None),
        Outcome::Error,
    )
}
