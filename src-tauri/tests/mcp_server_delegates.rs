//! Spec 021 US6 (T083): delegates in the chat. "Rechte festlegen" registers Claude Code or Codex
//! through the registration dialog; then each start gets holzi's server with a pass that lives only
//! in the app, no credential file, and its call prompts name the chat thread. Without a
//! registration, or once its start ended, a pass is refused and no dialog opens.

#[path = "common/mcp_fixture.rs"]
mod mcp_fixture;

use std::sync::Arc;

use haex_crdt::rusqlite::params;
use serde_json::json;
use uuid::Uuid;

use holzi_lib::adapters::cli_delegate::holzi_mcp::HolziAccess;
use holzi_lib::adapters::cli_delegate::DelegateVendor;
use holzi_lib::mcp_server::connection::now_millis;
use holzi_lib::mcp_server::delegates::{
    self, delegate_id, DelegatePass, DelegateRegistration, DelegateStatus,
};
use holzi_lib::mcp_server::grants::{OperateLevel, Rights};
use holzi_lib::mcp_server::hello::CloseReason;
use holzi_lib::mcp_server::helper::exit;
use holzi_lib::mcp_server::manage::EXPIRES_AFTER_MILLIS;
use holzi_lib::mcp_server::prompts::CallDecision;
use holzi_lib::mcp_server::registry::{AgentKind, AgentMode};
use holzi_lib::mcp_server::server::Running;
use mcp_fixture::*;

const THREAD: Uuid = Uuid::from_u128(0x7e57_0021);
const SLUG: &str = "claude-code-delegate";
const WRITE_TOOL: &str = "settings_appearance_setColorScheme";

fn settings_rights() -> Rights {
    Rights {
        operate: [("settings.device".to_owned(), OperateLevel::Write)]
            .into_iter()
            .collect(),
        ..Default::default()
    }
}

/// The helper's exit code for a start holzi turns away.
async fn refused(fx: &Fixture, args: &[String]) -> i32 {
    match fx.delegate(args).await {
        Ok(_) => panic!("the delegate was let in"),
        Err(helper) => helper.await.expect("the helper"),
    }
}

/// "Rechte festlegen" for Claude Code, answered with `rights` and `mode`.
async fn set_rights(
    fx: &mut Fixture,
    server: &Running,
    rights: Rights,
    mode: AgentMode,
) -> (DelegateRegistration, Rights) {
    let core = Arc::clone(server.core());
    let asking =
        tokio::spawn(
            async move { delegates::register(&core, AgentKind::ClaudeCodeDelegate).await },
        );
    let (id, question) = fx.next_question().await;
    assert!(question.delegate, "the dialog knows it is a delegate");
    assert_eq!(question.slug, SLUG);
    assert_eq!(question.client_name, "Claude Code");
    fx.register_as(&id, rights, mode);
    (asking.await.expect("registered"), question.rights)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_registered_delegate_comes_in_by_its_pass_and_asks_in_its_chat() {
    let mut fx = fixture();
    let server = fx.server().await;
    assert_eq!(
        delegates::status(server.core(), AgentKind::ClaudeCodeDelegate).await,
        DelegateStatus::Unregistered
    );
    let (registered, _) = set_rights(&mut fx, &server, settings_rights(), AgentMode::Manual).await;
    assert_eq!(registered, DelegateRegistration::Registered);
    assert_eq!(
        fx.agent_id(SLUG).await,
        delegate_id(fx.device, AgentKind::ClaudeCodeDelegate),
        "one row per kind and device"
    );

    let holzi = fx
        .delegates
        .server_for(DelegateVendor::Claude, Some(THREAD))
        .await
        .expect("a registered delegate gets holzi");
    assert_eq!(holzi.args[1], SLUG);
    let (agent, _helper) = fx.delegate(&holzi.args).await.expect("connected");
    wait_for_tools(&agent).await;
    fx.no_question().await;
    assert!(
        !fx.credential_file(SLUG).exists(),
        "a delegate keeps no file"
    );

    let asking = tokio::spawn({
        let peer = agent.peer().clone();
        async move { call_with(&peer, WRITE_TOOL, json!({ "x": "dark" })).await }
    });
    let (id, question) = fx.next_call().await;
    assert_eq!(question.delegate_thread_id, Some(THREAD.to_string()));
    fx.answer_call(&id, CallDecision::Allow);
    assert_eq!(error_code(&asking.await.expect("the call")), None);

    // The start ended: its pass is gone with it.
    let args = holzi.args.clone();
    drop(holzi);
    assert_eq!(refused(&fx, &args).await, exit::REJECTED);
    fx.no_question().await;
    server.stop(CloseReason::Disabled).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn without_a_registration_there_is_no_server_and_no_dialog() {
    let mut fx = fixture();
    let server = fx.server().await;
    assert!(fx
        .delegates
        .server_for(DelegateVendor::Codex, Some(THREAD))
        .await
        .is_none());

    // A pass of an unregistered delegate opens nothing either.
    let (credential, _guard) = fx
        .delegates
        .issue(DelegatePass {
            kind: AgentKind::CodexDelegate,
            thread: None,
        })
        .expect("a pass");
    let args = [
        "--mcp-agent",
        "codex-delegate",
        "--delegate-token",
        credential.as_str(),
    ]
    .map(str::to_owned);
    assert_eq!(refused(&fx, &args).await, exit::REJECTED);
    fx.no_question().await;

    // With the access off, even a registered delegate gets no server.
    set_rights(&mut fx, &server, settings_rights(), AgentMode::Auto).await;
    server.stop(CloseReason::Disabled).await;
    assert!(fx
        .delegates
        .server_for(DelegateVendor::Claude, None)
        .await
        .is_none());
}

/// FR-007: a delegate not started for 90 days expires; "Rechte festlegen" starts from its rights.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_expired_delegate_registers_again_from_its_last_rights() {
    let mut fx = fixture();
    let server = fx.server().await;
    set_rights(&mut fx, &server, settings_rights(), AgentMode::Plan).await;
    let (device, id) = (
        fx.device,
        delegate_id(fx.device, AgentKind::ClaudeCodeDelegate),
    );
    let long_ago = now_millis() - EXPIRES_AFTER_MILLIS - 1000;
    fx.db
        .write(move |tx| {
            tx.execute(
                "UPDATE haex_mcp_agents SET last_seen_at = ?3 WHERE id = ?1 AND vault_device_uuid = ?2",
                params![id.to_string(), device.to_string(), long_ago],
            )?;
            Ok(())
        })
        .await
        .expect("age the registration");
    assert!(fx
        .delegates
        .server_for(DelegateVendor::Claude, None)
        .await
        .is_none());
    assert_eq!(
        delegates::status(server.core(), AgentKind::ClaudeCodeDelegate).await,
        DelegateStatus::Expired
    );

    let (registered, preset) =
        set_rights(&mut fx, &server, Rights::default(), AgentMode::Auto).await;
    assert_eq!(registered, DelegateRegistration::Registered);
    assert_eq!(
        preset,
        settings_rights(),
        "the dialog starts from the last rights"
    );
    assert!(fx
        .delegates
        .server_for(DelegateVendor::Claude, None)
        .await
        .is_some());
    server.stop(CloseReason::Disabled).await;
}
