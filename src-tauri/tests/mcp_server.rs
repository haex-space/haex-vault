//! Spec 021, the MCP server against a real vault, a real socket and the real `--mcp-agent` helper:
//! registering (US1, T020, T021), rights on every call (US2, T036) and keeping strangers out
//! (US8, T041, T043).

#[path = "common/mcp_fixture.rs"]
mod mcp_fixture;

use std::time::Duration;

use serde_json::json;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use holzi_lib::mcp_server::grants::{default_rights, OperateLevel, Rights};
use holzi_lib::mcp_server::hello::CloseReason;
use holzi_lib::mcp_server::helper::exit;
use holzi_lib::mcp_server::session::INSTRUCTIONS_WAITING;
use mcp_fixture::*;

// ---------------------------------------------------------------------------
// User Story 1: connect and register
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_unknown_agent_waits_without_tools_and_gets_them_after_the_registration() {
    let mut fx = fixture();
    let server = fx.server().await;
    let (agent, _helper) = fx.agent("claude-code").await.expect("connected");

    let info = agent.peer_info().expect("initialize answered");
    assert_eq!(
        info.server_info.as_ref().map(|s| s.name.as_str()),
        Some("holzi")
    );
    assert_eq!(info.instructions.as_deref(), Some(INSTRUCTIONS_WAITING));
    assert!(
        tool_names(&agent).await.is_empty(),
        "no tool before the registration"
    );

    let (id, question) = fx.next_question().await;
    assert_eq!(question.slug, "claude-code");
    assert_eq!(
        question.rights,
        default_rights(),
        "reading is preset (FR-009)"
    );
    fx.register(&id, default_rights());
    wait_for_tools(&agent).await;

    // The preset reads everywhere, so only the reading actions show.
    assert_eq!(tool_names(&agent).await, ["settings_get", "wm_state_get"]);
    let kept = std::fs::read_to_string(fx.credential_file("claude-code")).expect("credential");
    assert_eq!(kept.trim().len(), 43);
    server.stop(CloseReason::Disabled).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_registered_agent_comes_back_without_a_dialog() {
    let mut fx = fixture();
    let server = fx.server().await;
    let (first, helper) = fx.registered("codex", default_rights()).await;
    drop(first.cancel().await);
    let _ = helper.await;

    let (again, _helper) = fx.agent("codex").await.expect("connected again");
    fx.no_question().await;
    assert_eq!(tool_names(&again).await, ["settings_get", "wm_state_get"]);
    server.stop(CloseReason::Disabled).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_remembered_denial_blocks_and_a_plain_one_lasts_for_the_session() {
    let mut fx = fixture();
    let server = fx.server().await;

    let (_agent, helper) = fx.agent("blocked-agent").await.expect("connected");
    let (id, _) = fx.next_question().await;
    fx.deny(&id, true);
    assert_eq!(helper.await.expect("helper"), exit::CLOSED);
    let Err(refused) = fx.agent("blocked-agent").await else {
        panic!("refused");
    };
    assert_eq!(refused.await.expect("helper"), exit::REJECTED);
    fx.no_question().await;

    let (_agent, helper) = fx.agent("declined-agent").await.expect("connected");
    let (id, _) = fx.next_question().await;
    fx.deny(&id, false);
    assert_eq!(helper.await.expect("helper"), exit::CLOSED);
    let Err(refused) = fx.agent("declined-agent").await else {
        panic!("refused");
    };
    assert_eq!(refused.await.expect("helper"), exit::REJECTED);
    fx.no_question().await;
    server.stop(CloseReason::Disabled).await;

    // A new vault session forgets the plain denial but not the remembered one.
    let server = fx.server().await;
    // Kept until the question arrives: a dropped agent ends its session before it is asked.
    let (_agent, _helper) = fx.agent("declined-agent").await.expect("connected");
    fx.next_question().await;
    assert!(fx.agent("blocked-agent").await.is_err());
    server.stop(CloseReason::Disabled).await;
}

// ---------------------------------------------------------------------------
// User Story 2: only what the rights cover
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_call_outside_the_rights_is_refused_and_never_runs() {
    let mut fx = fixture();
    let server = fx.server().await;
    let rights = Rights {
        operate: [("wm.read".to_owned(), OperateLevel::Read)]
            .into_iter()
            .collect(),
        browser: None,
    };
    let (agent, _helper) = fx.registered("claude-code", rights).await;
    assert_eq!(tool_names(&agent).await, ["wm_state_get"]);

    for forbidden in [
        "settings_get",
        "settings_appearance_setColorScheme",
        "no_such_tool",
    ] {
        let result = call(&agent, forbidden).await;
        assert_eq!(
            error_code(&result).as_deref(),
            Some("not_allowed"),
            "{forbidden}"
        );
    }
    assert!(fx.window_calls().is_empty(), "nothing ran");

    let result = call(&agent, "wm_state_get").await;
    assert_eq!(error_code(&result), None);
    assert_eq!(
        result.structured_content,
        Some(json!({ "ran": "wm.state.get" }))
    );
    let calls = fx.window_calls();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].0, "wm.state.get");
    assert_eq!(
        calls[0].1["kind"], "externalAgent",
        "the window learns who calls"
    );
    assert!(calls[0].1["agentId"].is_string());
    server.stop(CloseReason::Disabled).await;
}

// ---------------------------------------------------------------------------
// User Story 8: strangers get nothing
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_forged_credential_counts_as_unknown_and_gets_no_tools() {
    let mut fx = fixture();
    let server = fx.server().await;
    let file = fx.credential_file("codex");
    std::fs::create_dir_all(file.parent().expect("dir")).expect("dir");
    std::fs::write(&file, "forged-credential").expect("forged");
    let (agent, _helper) = fx.agent("codex").await.expect("connected");
    fx.next_question().await;
    assert!(tool_names(&agent).await.is_empty());
    assert_eq!(
        error_code(&call(&agent, "wm_state_get").await).as_deref(),
        Some("not_allowed")
    );
    assert!(fx.window_calls().is_empty());
    server.stop(CloseReason::Disabled).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_hundred_connects_of_one_agent_show_one_dialog() {
    let mut fx = fixture();
    let server = fx.server().await;
    let (_agent, _helper) = fx.agent("looping").await.expect("connected");
    fx.next_question().await;
    for _ in 0..100 {
        let (reply, _stream) = fx.greet_raw("looping", None).await;
        assert_eq!(
            reply,
            json!({ "status": "rejected", "reason": "registration_busy" })
        );
    }
    fx.no_question().await;
    server.stop(CloseReason::Disabled).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_message_longer_than_the_limit_ends_the_connection() {
    let fx = fixture();
    let server = fx.server().await;
    let (reply, mut stream) = fx.greet_raw("flooding", None).await;
    assert_eq!(reply["status"], "pending");
    let chunk = vec![b'x'; 64 * 1024];
    for _ in 0..20 {
        if stream.write_all(&chunk).await.is_err() {
            break;
        }
    }
    let mut rest = Vec::new();
    let ended = tokio::time::timeout(Duration::from_secs(5), stream.read_to_end(&mut rest)).await;
    assert!(ended.is_ok(), "the server closed the connection");
    server.stop(CloseReason::Disabled).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn stopping_the_server_ends_the_agents_and_removes_the_socket() {
    let mut fx = fixture();
    let server = fx.server().await;
    let (_agent, helper) = fx.registered("claude-code", default_rights()).await;
    server.stop(CloseReason::VaultClosed).await;
    assert_eq!(
        tokio::time::timeout(WAIT, helper)
            .await
            .expect("the helper ends")
            .expect("helper"),
        exit::CLOSED
    );
    assert!(
        fx.agent("claude-code").await.is_err(),
        "nothing listens any more"
    );
}
