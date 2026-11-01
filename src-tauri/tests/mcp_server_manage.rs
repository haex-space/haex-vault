//! Spec 021 US7 (T056–T058): what the user changes in "KI → Agenten" reaches a connected agent at
//! once. A revocation ends the connection and its waiting prompt, the credential stops working and
//! the next connection asks again; a registration without a connection for 90 days expires and the
//! dialog starts from the old rights; new rights apply from the next call.

#[path = "common/mcp_fixture.rs"]
mod mcp_fixture;

use std::time::{Duration, Instant};

use haex_crdt::rusqlite::params;
use mcp_fixture::*;

use holzi_lib::mcp_server::connection::now_millis;
use holzi_lib::mcp_server::grants::{OperateLevel, Rights};
use holzi_lib::mcp_server::hello::CloseReason;
use holzi_lib::mcp_server::helper::exit;
use holzi_lib::mcp_server::manage::EXPIRES_AFTER_MILLIS;
use holzi_lib::mcp_server::manage_commands::{revoke, set_mode, set_rights, unblock};
use holzi_lib::mcp_server::registry::{self, AgentMode, AgentStatus, Transport};

fn rights(scopes: &[(&str, OperateLevel)]) -> Rights {
    Rights {
        operate: scopes
            .iter()
            .map(|(scope, level)| ((*scope).to_owned(), *level))
            .collect(),
        browser: None,
        ..Default::default()
    }
}

/// FR-027, SC-004: the connection ends within a second, the waiting prompt with it, the credential
/// is gone and the next connection asks for a new registration.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn revoking_ends_the_connection_and_the_next_one_asks_again() {
    let mut fx = fixture();
    let server = fx.server().await;
    let all = rights(&[("settings.read", OperateLevel::Write)]);
    let (agent, helper) = fx.registered_as("revoked", all, AgentMode::Manual).await;
    let id = fx.agent_id("revoked").await;
    let pending = tokio::spawn({
        let agent = agent.peer().clone();
        async move { call(&agent, "settings_get").await }
    });
    let (prompt, _) = fx.next_call().await;

    let started = Instant::now();
    revoke(&fx.db, fx.device, Some(server.core()), id)
        .await
        .expect("revoke");
    let code = tokio::time::timeout(Duration::from_secs(1), helper)
        .await
        .expect("the helper ends within a second")
        .expect("helper");
    assert!(started.elapsed() < Duration::from_secs(1));
    assert_eq!(code, exit::CLOSED);
    assert!(!fx
        .calls
        .answer(&prompt, holzi_lib::mcp_server::prompts::CallDecision::Allow));
    let _ = tokio::time::timeout(WAIT, pending).await;
    assert!(fx.window_calls().is_empty(), "the waiting call never ran");
    assert!(
        !fx.credential_file("revoked").exists(),
        "the helper forgets the credential"
    );
    let device = fx.device;
    let left = fx
        .db
        .read(move |q| registry::get(q, device, id))
        .await
        .expect("read");
    assert_eq!(left, None);

    let (_again, _helper) = fx.agent("revoked").await.expect("connected");
    fx.next_question().await;
    server.stop(CloseReason::Disabled).await;
}

/// FR-007: after 90 days without a connection the credential stops working, and the dialog starts
/// from the rights and approval mode the agent had.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_registration_without_a_connection_for_ninety_days_expires() {
    let mut fx = fixture();
    let server = fx.server().await;
    let before = rights(&[
        ("wm.read", OperateLevel::Read),
        ("settings.device", OperateLevel::Write),
    ]);
    let (first, helper) = fx
        .registered_as("sleepy", before.clone(), AgentMode::Auto)
        .await;
    drop(first.cancel().await);
    let _ = helper.await;
    let id = fx.agent_id("sleepy").await;
    let device = fx.device;
    let long_ago = now_millis() - EXPIRES_AFTER_MILLIS - 1000;
    fx.db
        .write(move |tx| {
            tx.execute(
                "UPDATE haex_mcp_agents SET last_seen_at = ?2 WHERE id = ?1",
                params![id.to_string(), long_ago],
            )?;
            Ok(())
        })
        .await
        .expect("age the registration");

    let (again, _helper) = fx.agent("sleepy").await.expect("connected");
    let (question_id, question) = fx.next_question().await;
    assert_eq!(question.rights, before, "the old rights prefilled");
    assert_eq!(question.mode, AgentMode::Auto, "the old mode prefilled");
    assert!(
        tool_names(&again).await.is_empty(),
        "the old credential is refused"
    );
    let expired = fx
        .db
        .read(move |q| registry::get(q, device, id))
        .await
        .expect("read")
        .expect("the row stays");
    assert_eq!(expired.status, AgentStatus::Expired);

    fx.register_as(&question_id, before, AgentMode::Auto);
    wait_for_tools(&again).await;
    let renewed = fx
        .db
        .read(move |q| registry::list(q, device))
        .await
        .expect("read");
    assert_eq!(renewed.len(), 1, "the same row, renewed");
    assert_eq!(renewed[0].id, id);
    assert_eq!(renewed[0].status, AgentStatus::Registered);
    server.stop(CloseReason::Disabled).await;
}

/// FR-017, US2 scenario 4: new rights apply from the next call, and the agent sees new tools.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn changed_rights_apply_to_a_connected_agent() {
    let mut fx = fixture();
    let server = fx.server().await;
    let (agent, _helper) = fx
        .registered("changing", rights(&[("wm.read", OperateLevel::Read)]))
        .await;
    let id = fx.agent_id("changing").await;
    assert_eq!(tool_names(&agent).await, ["wm_state_get"]);
    assert_eq!(
        error_code(&call(agent.peer(), "settings_get").await).as_deref(),
        Some("not_allowed")
    );

    let wider = rights(&[
        ("wm.read", OperateLevel::Read),
        ("settings.read", OperateLevel::Read),
    ]);
    set_rights(&fx.db, fx.device, Some(server.core()), id, wider)
        .await
        .expect("rights");
    assert_eq!(tool_names(&agent).await, ["settings_get", "wm_state_get"]);
    assert_eq!(error_code(&call(agent.peer(), "settings_get").await), None);

    // Plan from now on: reading still runs.
    set_mode(&fx.db, fx.device, Some(server.core()), id, AgentMode::Plan)
        .await
        .expect("mode");
    assert_eq!(error_code(&call(agent.peer(), "wm_state_get").await), None);

    // Guardrails can never be granted.
    let guarded = rights(&[("guardrails", OperateLevel::Write)]);
    assert!(
        set_rights(&fx.db, fx.device, Some(server.core()), id, guarded)
            .await
            .is_err()
    );
    server.stop(CloseReason::Disabled).await;
}

/// FR-026: lifting a remembered block in the current vault session permits registration again.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn unblocking_clears_the_session_decline_and_allows_registration_again() {
    let mut fx = fixture();
    let server = fx.server().await;
    let (first, helper) = fx.agent("blocked").await.expect("pending agent");
    let (request, _) = fx.next_question().await;
    fx.deny(&request, true);
    assert_eq!(helper.await.expect("helper"), exit::CLOSED);
    drop(first);

    let id = fx.agent_id("blocked").await;
    let core = server.core();
    assert!(core.declined("blocked", Transport::Ipc));
    unblock(&fx.db, fx.device, Some(core), id)
        .await
        .expect("unblock");
    assert!(!core.declined("blocked", Transport::Ipc));

    let (_again, helper) = fx.agent("blocked").await.expect("connected again");
    let (request, _) = fx.next_question().await;
    fx.register(&request, Rights::default());
    server.stop(CloseReason::Disabled).await;
    let _ = tokio::time::timeout(WAIT, helper).await;
}
