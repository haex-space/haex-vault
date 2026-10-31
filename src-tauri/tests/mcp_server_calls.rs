//! Calls of a registered agent (spec 021 US3, T045, T047, T052): the approval mode decides whether
//! a call runs, asks the window or is refused; every call lands in the agent log, refused ones too;
//! and one agent runs at most four calls at once.

#[path = "common/mcp_fixture.rs"]
mod mcp_fixture;

use std::time::Duration;

use mcp_fixture::{call, call_with, error_code, fixture, fixture_with};
use serde_json::json;

use holzi_lib::mcp_server::agent_log::Outcome;
use holzi_lib::mcp_server::grants::{OperateLevel, Rights};
use holzi_lib::mcp_server::hello::CloseReason;
use holzi_lib::mcp_server::prompts::{CallDecision, CallEffect};
use holzi_lib::mcp_server::registry::AgentMode;

/// Reading and changing in the scopes of the test catalog.
fn all_rights() -> Rights {
    Rights {
        operate: ["wm.read", "settings.read", "settings.device", "chat.write"]
            .into_iter()
            .map(|scope| (scope.to_owned(), OperateLevel::Write))
            .collect(),
        browser: None,
        ..Default::default()
    }
}

fn ran(fx: &mcp_fixture::Fixture) -> Vec<String> {
    fx.window_calls()
        .into_iter()
        .map(|(action, _)| action)
        .collect()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn manual_asks_before_every_call_and_runs_only_what_is_allowed() {
    let mut fx = fixture();
    let server = fx.server().await;
    let (agent, _helper) = fx
        .registered_as("claude-code", all_rights(), AgentMode::Manual)
        .await;

    let pending = tokio::spawn({
        let agent = agent.peer().clone();
        async move {
            call_with(
                &agent,
                "settings_appearance_setColorScheme",
                json!({ "x": "dark" }),
            )
            .await
        }
    });
    let (id, question) = fx.next_call().await;
    assert_eq!(question.action_id, "settings.appearance.setColorScheme");
    assert_eq!(question.effect, CallEffect::Write);
    assert_eq!(question.client_name, "rmcp");
    assert_eq!(question.input, json!({ "x": "dark" }));
    assert!(ran(&fx).is_empty(), "nothing runs before the answer");
    fx.answer_call(&id, CallDecision::Allow);
    let result = pending.await.expect("call");
    assert_eq!(error_code(&result), None);
    assert_eq!(ran(&fx), ["settings.appearance.setColorScheme"]);

    let pending = tokio::spawn({
        let agent = agent.peer().clone();
        async move { call(&agent, "wm_state_get").await }
    });
    let (id, _) = fx.next_call().await;
    fx.answer_call(&id, CallDecision::Deny);
    let result = pending.await.expect("call");
    assert_eq!(error_code(&result).as_deref(), Some("user_declined"));
    assert_eq!(ran(&fx).len(), 1, "a declined call never runs");

    let log = fx.log_of("claude-code").await;
    let outcomes: Vec<(String, Outcome, bool)> = log
        .iter()
        .map(|entry| (entry.tool.clone(), entry.outcome, entry.asked))
        .collect();
    assert_eq!(
        outcomes,
        [
            ("wm_state_get".to_owned(), Outcome::DeniedUser, true),
            (
                "settings_appearance_setColorScheme".to_owned(),
                Outcome::Ok,
                true
            ),
        ]
    );
    assert_eq!(log[1].effect, "write");
    assert_eq!(log[1].input.as_deref(), Some(r#"{"x":"dark"}"#));
    server.stop(CloseReason::Disabled).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn auto_runs_reading_and_changing_and_asks_before_destroying() {
    let mut fx = fixture();
    let server = fx.server().await;
    let (agent, _helper) = fx
        .registered_as("codex", all_rights(), AgentMode::Auto)
        .await;

    for tool in ["wm_state_get", "settings_appearance_setColorScheme"] {
        assert_eq!(error_code(&call(&agent, tool).await), None, "{tool}");
    }
    fx.no_call_prompt().await;

    let pending = tokio::spawn({
        let agent = agent.peer().clone();
        async move { call(&agent, "chat_thread_delete").await }
    });
    let (id, question) = fx.next_call().await;
    assert_eq!(question.effect, CallEffect::Destructive);
    fx.answer_call(&id, CallDecision::Allow);
    assert_eq!(error_code(&pending.await.expect("call")), None);
    assert_eq!(
        ran(&fx),
        [
            "wm.state.get",
            "settings.appearance.setColorScheme",
            "chat.thread.delete"
        ]
    );
    server.stop(CloseReason::Disabled).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn plan_runs_reading_and_refuses_the_rest_without_asking() {
    let mut fx = fixture();
    let server = fx.server().await;
    let (agent, _helper) = fx
        .registered_as("planner", all_rights(), AgentMode::Plan)
        .await;

    assert_eq!(error_code(&call(&agent, "settings_get").await), None);
    for tool in ["settings_appearance_setColorScheme", "chat_thread_delete"] {
        assert_eq!(
            error_code(&call(&agent, tool).await).as_deref(),
            Some("plan_mode"),
            "{tool}"
        );
    }
    fx.no_call_prompt().await;
    assert_eq!(ran(&fx), ["settings.get"]);
    let outcomes: Vec<Outcome> = fx
        .log_of("planner")
        .await
        .iter()
        .map(|entry| entry.outcome)
        .collect();
    assert_eq!(
        outcomes,
        [Outcome::DeniedPlan, Outcome::DeniedPlan, Outcome::Ok]
    );
    server.stop(CloseReason::Disabled).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn refused_calls_are_logged_too() {
    let mut fx = fixture();
    let server = fx.server().await;
    let rights = Rights {
        operate: [("wm.read".to_owned(), OperateLevel::Read)]
            .into_iter()
            .collect(),
        browser: None,
        ..Default::default()
    };
    let (agent, _helper) = fx.registered("reader", rights).await;
    for tool in ["settings_appearance_setColorScheme", "no_such_tool"] {
        assert_eq!(
            error_code(&call(&agent, tool).await).as_deref(),
            Some("not_allowed")
        );
    }
    let log = fx.log_of("reader").await;
    assert_eq!(log.len(), 2);
    assert!(log
        .iter()
        .all(|entry| entry.outcome == Outcome::DeniedRights));
    assert_eq!(log[0].tool, "no_such_tool");
    assert_eq!(log[0].effect, "", "an unknown tool has no effect");
    assert_eq!(log[1].effect, "write");
    server.stop(CloseReason::Disabled).await;
}

/// FR-023: a prompt nobody answers ends without running the call.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_unanswered_prompt_ends_with_no_answer() {
    let mut fx = fixture_with(Duration::from_millis(300));
    let server = fx.server().await;
    let (agent, _helper) = fx
        .registered_as("slow", all_rights(), AgentMode::Manual)
        .await;
    let pending = tokio::spawn({
        let agent = agent.peer().clone();
        async move { call(&agent, "settings_get").await }
    });
    fx.next_call().await;
    let result = pending.await.expect("call");
    assert_eq!(error_code(&result).as_deref(), Some("no_answer"));
    assert!(ran(&fx).is_empty());
    let log = fx.log_of("slow").await;
    assert_eq!(log[0].outcome, Outcome::NoAnswer);
    assert!(log[0].asked);
    server.stop(CloseReason::Disabled).await;
}

/// Closing the vault ends a waiting prompt; the call never runs.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn stopping_the_server_ends_a_waiting_prompt() {
    let mut fx = fixture();
    let server = fx.server().await;
    let (agent, _helper) = fx
        .registered_as("waiting", all_rights(), AgentMode::Manual)
        .await;
    let pending = tokio::spawn({
        let agent = agent.peer().clone();
        async move {
            agent
                .call_tool(rmcp::model::CallToolRequestParams::new("settings_get"))
                .await
        }
    });
    let (id, _) = fx.next_call().await;
    server.stop(CloseReason::VaultClosed).await;
    // The agent may see the result or only the closed connection; either way nothing ran.
    let _ = tokio::time::timeout(mcp_fixture::WAIT, pending).await;
    assert!(
        !fx.calls.answer(&id, CallDecision::Allow),
        "the prompt is gone"
    );
    assert!(ran(&fx).is_empty());
}

/// FR-029, research R11: more than four running calls of one agent wait, then give up.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_fifth_concurrent_call_gets_too_many_calls() {
    let mut fx = fixture();
    fx.limits.call_wait = Duration::from_millis(300);
    let server = fx.server().await;
    let (agent, _helper) = fx
        .registered_as("busy", all_rights(), AgentMode::Manual)
        .await;

    let mut pending = Vec::new();
    let mut prompts = Vec::new();
    for _ in 0..4 {
        let agent = agent.peer().clone();
        pending.push(tokio::spawn(
            async move { call(&agent, "settings_get").await },
        ));
        prompts.push(fx.next_call().await.0);
    }
    let fifth = call(&agent, "wm_state_get").await;
    assert_eq!(error_code(&fifth).as_deref(), Some("too_many_calls"));
    fx.no_call_prompt().await;

    for id in &prompts {
        fx.answer_call(id, CallDecision::Allow);
    }
    for call in pending {
        assert_eq!(error_code(&call.await.expect("call")), None);
    }
    // A slot is free again.
    let again = tokio::spawn({
        let agent = agent.peer().clone();
        async move { call(&agent, "wm_state_get").await }
    });
    let (id, _) = fx.next_call().await;
    fx.answer_call(&id, CallDecision::Allow);
    assert_eq!(error_code(&again.await.expect("call")), None);
    assert_eq!(fx.log_of("busy").await[5].outcome, Outcome::TooMany);
    server.stop(CloseReason::Disabled).await;
}
