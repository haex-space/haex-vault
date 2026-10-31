//! Spec 021 US5 (T073): holzi's command tool for external agents. `run_command` is listed only with
//! the group "Befehle ausführen" and only where the device runs commands; it counts as destructive,
//! so Auto asks before every command and Plan refuses it. The prompt and the log show the command
//! line without the credentials in it (research R10).

#[path = "common/mcp_fixture.rs"]
mod mcp_fixture;

use std::sync::Arc;

use serde_json::{json, Value};

use holzi_lib::chat::tools::cli::CliTool;
use holzi_lib::mcp_server::agent_log::Outcome;
use holzi_lib::mcp_server::grants::{default_rights, Rights};
use holzi_lib::mcp_server::hello::CloseReason;
use holzi_lib::mcp_server::prompts::{CallDecision, CallEffect};
use holzi_lib::mcp_server::registry::AgentMode;
use mcp_fixture::*;

const TOOL: &str = "run_command";

fn commands() -> Rights {
    Rights {
        commands: true,
        ..default_rights()
    }
}

fn with_command_tool() -> Fixture {
    let mut fx = fixture();
    fx.command_tool = Some(Arc::new(CliTool::default()));
    fx
}

fn command(line: &str) -> Value {
    json!({ "command": line })
}

/// A command that leaves `marker` behind when it runs, in `sh` and in `cmd`.
fn marking(fx: &Fixture) -> (Value, std::path::PathBuf) {
    let marker = fx.dir.path().join("ran");
    (
        command(&format!("echo ran > \"{}\"", marker.display())),
        marker,
    )
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_command_tool_needs_the_group_and_a_device_that_runs_commands() {
    // The device runs commands, the agent lacks the group; then the agent has the group on a
    // device that runs none (Android, iOS).
    for (mut fx, rights) in [
        (with_command_tool(), default_rights()),
        (fixture(), commands()),
    ] {
        let server = fx.server().await;
        let (agent, _helper) = fx.registered("claude-code", rights).await;
        assert!(!tool_names(&agent).await.contains(&TOOL.to_owned()));
        let (input, marker) = marking(&fx);
        let refused = call_with(agent.peer(), TOOL, input).await;
        assert_eq!(error_code(&refused).as_deref(), Some("not_allowed"));
        fx.no_call_prompt().await;
        assert!(!marker.exists(), "nothing ran");
        server.stop(CloseReason::Disabled).await;
    }
}

/// FR-012: listed as destructive; Auto asks before every command, and the answer decides.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn every_command_asks_in_auto_and_runs_only_after_allow() {
    let mut fx = with_command_tool();
    let server = fx.server().await;
    let (agent, _helper) = fx
        .registered_as("auto-agent", commands(), AgentMode::Auto)
        .await;
    let tools = agent.list_all_tools().await.expect("tools/list");
    let tool = tools
        .iter()
        .find(|tool| tool.name == TOOL)
        .expect("run_command is offered");
    let hints = tool.annotations.as_ref().expect("annotations");
    assert_eq!(hints.destructive_hint, Some(true));
    assert_eq!(hints.read_only_hint, Some(false));

    let asking = tokio::spawn({
        let peer = agent.peer().clone();
        async move { call_with(&peer, TOOL, command("echo hello --token=s3cret")).await }
    });
    let (id, question) = fx.next_call().await;
    assert_eq!(question.action_id, TOOL);
    assert_eq!(question.effect, CallEffect::Destructive);
    assert_eq!(question.titles.en, "Run a command");
    assert_eq!(question.input, command("echo hello --token=•••"));
    fx.answer_call(&id, CallDecision::Allow);
    let allowed = asking.await.expect("the call");
    assert_eq!(error_code(&allowed), None);
    let output = allowed.structured_content.expect("output")["output"]
        .as_str()
        .map(|output| output.trim().to_owned());
    assert_eq!(output.as_deref(), Some("hello --token=s3cret"));

    let (input, marker) = marking(&fx);
    let declining = tokio::spawn({
        let peer = agent.peer().clone();
        async move { call_with(&peer, TOOL, input).await }
    });
    let (id, _) = fx.next_call().await;
    fx.answer_call(&id, CallDecision::Deny);
    let declined = declining.await.expect("the call");
    assert_eq!(error_code(&declined).as_deref(), Some("user_declined"));
    assert!(!marker.exists(), "nothing ran");

    let failing = tokio::spawn({
        let peer = agent.peer().clone();
        async move { call_with(&peer, TOOL, command("exit 3")).await }
    });
    let (id, _) = fx.next_call().await;
    fx.answer_call(&id, CallDecision::Allow);
    let failed = failing.await.expect("the call");
    assert_eq!(error_code(&failed).as_deref(), Some("command_failed"));

    let log = fx.log_of("auto-agent").await;
    let outcomes: Vec<_> = log.iter().map(|entry| entry.outcome).collect();
    assert_eq!(
        outcomes,
        [Outcome::Error, Outcome::DeniedUser, Outcome::Ok],
        "newest first"
    );
    assert!(log
        .iter()
        .all(|entry| entry.asked && entry.effect == "destructive"));
    let shown = log[2].input.as_deref().expect("the inputs");
    assert!(!shown.contains("s3cret"), "{shown}");
    server.stop(CloseReason::Disabled).await;
}

/// FR-012: Plan refuses every command without asking.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn plan_refuses_every_command_without_asking() {
    let mut fx = with_command_tool();
    let server = fx.server().await;
    let (agent, _helper) = fx
        .registered_as("plan-agent", commands(), AgentMode::Plan)
        .await;
    assert!(tool_names(&agent).await.contains(&TOOL.to_owned()));
    let (input, marker) = marking(&fx);
    let refused = call_with(agent.peer(), TOOL, input).await;
    assert_eq!(error_code(&refused).as_deref(), Some("plan_mode"));
    fx.no_call_prompt().await;
    assert!(!marker.exists(), "nothing ran");
    assert_eq!(
        fx.log_of("plan-agent").await[0].outcome,
        Outcome::DeniedPlan
    );
    server.stop(CloseReason::Disabled).await;
}
