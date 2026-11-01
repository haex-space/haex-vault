//! holzi's command tool for external agents (spec 021 US5, FR-012): offered only with the group and
//! only where the device runs commands, and its answers in the shape of the contract.

use std::sync::Arc;

use serde_json::json;

use super::agent_log::Outcome;
use super::command_tool::{check, offer, result};
use super::grants::{default_rights, Rights};
use crate::chat::tools::cli::CliTool;
use crate::chat::tools::{Tool, ToolResult};

fn commands() -> Rights {
    Rights {
        commands: true,
        ..default_rights()
    }
}

#[test]
fn the_tool_needs_the_group_and_a_device_that_runs_commands() {
    let tool: Arc<dyn Tool> = Arc::new(CliTool::default());
    assert!(offer(&commands(), Some(&tool)).is_some());
    assert!(offer(&default_rights(), Some(&tool)).is_none());
    assert!(offer(&commands(), None).is_none());
    assert!(check(&commands(), Some(&tool), "run_command").is_some());
    assert!(check(&commands(), Some(&tool), "run_commands").is_none());
    assert!(check(&default_rights(), Some(&tool), "run_command").is_none());
}

#[test]
fn the_answer_carries_the_output_and_a_stop_is_unavailable() {
    let (ok, outcome) = result(ToolResult::ok("hello\n"));
    assert_eq!(outcome, Outcome::Ok);
    assert_eq!(ok.structured_content, Some(json!({ "output": "hello\n" })));

    let (failed, outcome) = result(ToolResult::error("command exited with status 3"));
    assert_eq!(outcome, Outcome::Error);
    assert_eq!(
        failed.structured_content,
        Some(json!({ "code": "command_failed", "message": "command exited with status 3" }))
    );

    let (stopped, outcome) = result(ToolResult::error("tool_call_cancelled"));
    assert_eq!(outcome, Outcome::Unavailable);
    assert_eq!(
        stopped.structured_content.expect("body")["code"],
        "unavailable"
    );
}
