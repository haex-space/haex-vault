//! Spec 021 T035, the contract of the rights (research R3): one case per rights group that exists,
//! and the guardrails never listed and never callable, whatever an agent was granted. Later
//! deliveries add their groups (extensions, files, passwords, commands) as cases here.

#[path = "common/mcp_fixture.rs"]
mod mcp_fixture;

use holzi_lib::mcp_server::grants::{OperateLevel, Rights, OPERATE_SCOPES};
use holzi_lib::mcp_server::hello::CloseReason;
use mcp_fixture::*;

/// Every scope of "holzi bedienen" at the highest level.
fn everything() -> Rights {
    Rights {
        operate: OPERATE_SCOPES
            .iter()
            .map(|scope| ((*scope).to_owned(), OperateLevel::Write))
            .collect(),
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn group_operate_lists_exactly_the_covered_actions() {
    let mut fx = fixture();
    let server = fx.server().await;
    let (agent, _helper) = fx.registered("contract", everything()).await;
    // Until the call prompts exist an agent only reads, whatever its level (shipping note).
    assert_eq!(tool_names(&agent).await, ["settings_get", "wm_state_get"]);
    server.stop(CloseReason::Disabled).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn guardrails_are_never_listed_and_never_callable() {
    let mut fx = fixture();
    let server = fx.server().await;
    let (agent, _helper) = fx.registered("contract", everything()).await;
    assert!(!tool_names(&agent)
        .await
        .contains(&"settings_agents_mcp_enable".to_owned()));
    let result = call(&agent, "settings_agents_mcp_enable").await;
    assert_eq!(error_code(&result).as_deref(), Some("not_allowed"));
    assert!(fx
        .window_calls()
        .iter()
        .all(|(action, _)| action != "settings.agents.mcp.enable"));
    server.stop(CloseReason::Disabled).await;
}
