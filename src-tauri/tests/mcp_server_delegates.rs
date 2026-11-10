//! Spec 021 US6 and spec 083 PR J (T096 to T100): delegates in the chat. Claude Code and Codex get
//! holzi's server with a pass that lives only in the app, no credential file, and its call prompts
//! name the chat thread. There is no registration: what a delegate may do is the agent profile of
//! the model its start runs with, like for any model, and a model the active profile does not offer
//! gets no server. Once its start ended, a pass is refused.

#[path = "common/mcp_fixture.rs"]
mod mcp_fixture;

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use holzi_lib::adapters::cli_delegate::holzi_mcp::HolziAccess;
use holzi_lib::adapters::cli_delegate::DelegateVendor;
use holzi_lib::agent_profiles::{store, Subject};
use holzi_lib::chat::tools::native_action::{NativeCall, NativeError, NativeExecutor, NativeReply};
use holzi_lib::mcp_server::delegates::{self, model_delegate_id, DelegatePass};
use holzi_lib::mcp_server::grants::{default_rights, BrowserRights, OperateLevel, Rights};
use holzi_lib::mcp_server::hello::CloseReason;
use holzi_lib::mcp_server::helper::exit;
use holzi_lib::mcp_server::prompts::CallDecision;
use holzi_lib::mcp_server::registry::AgentKind;
use holzi_lib::mcp_server::session::INSTRUCTIONS_REGISTERED;
use mcp_fixture::*;

const THREAD: Uuid = Uuid::from_u128(0x7e57_0021);
const SLUG: &str = "claude-code-delegate";
const WRITE_TOOL: &str = "settings_appearance_setColorScheme";
/// The id of a model of the Abo-Agent's provider, as the chat names it.
const MODEL: &str = "6f3c0a52-0a47-4c0a-9a61-5d2d1a5b7e01:claude-sonnet";
const OTHER_MODEL: &str = "6f3c0a52-0a47-4c0a-9a61-5d2d1a5b7e01:claude-opus";

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

/// Makes `model` a member of a new active profile that gives `rights`.
async fn active_profile_with(fx: &Fixture, model: &str, rights: Rights) -> Uuid {
    let profile = fx.profile(rights).await;
    let model = Subject::Model(model.to_owned());
    fx.db
        .write(move |tx| store::add_member(tx, &model, profile))
        .await
        .expect("the model joins");
    profile
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_delegate_comes_in_by_its_pass_with_the_rights_of_its_model_and_asks_in_its_chat() {
    let mut fx = fixture();
    let server = fx.server().await;
    active_profile_with(&fx, MODEL, settings_rights()).await;

    let holzi = fx
        .delegates
        .server_for(DelegateVendor::Claude, Some(THREAD), MODEL)
        .await
        .expect("a model of the active profile gets holzi");
    assert_eq!(holzi.args[1], SLUG);
    assert_eq!(
        fx.agent_id(SLUG).await,
        model_delegate_id(fx.device, AgentKind::ClaudeCodeDelegate, MODEL),
        "the start wrote a technical row for this model"
    );
    let (agent, _helper) = fx.delegate(&holzi.args).await.expect("connected");
    wait_for_tools(&agent).await;
    fx.no_question().await;
    assert!(
        !fx.credential_file(SLUG).exists(),
        "a delegate keeps no file"
    );
    assert!(
        tool_names(&agent).await.contains(&WRITE_TOOL.to_owned()),
        "the profile of its model gives the write tool"
    );

    let asking = tokio::spawn({
        let peer = agent.peer().clone();
        async move { call_with(&peer, WRITE_TOOL, json!({ "x": "dark" })).await }
    });
    let (id, question) = fx.next_call().await;
    assert_eq!(question.delegate_thread_id, Some(THREAD.to_string()));
    fx.answer_call(&id, CallDecision::Allow);
    assert_eq!(error_code(&asking.await.expect("the call")), None);

    // The start ended: its pass is gone with it, and no registration dialog opens.
    let args = holzi.args.clone();
    drop(holzi);
    assert_eq!(refused(&fx, &args).await, exit::REJECTED);
    fx.no_question().await;
    server.stop(CloseReason::Disabled).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_model_of_standard_has_the_factory_rights_and_needs_no_registration() {
    let mut fx = fixture();
    let server = fx.server().await;
    let holzi = fx
        .delegates
        .server_for(DelegateVendor::Codex, Some(THREAD), MODEL)
        .await
        .expect("a model in Standard gets holzi");
    let (agent, _helper) = fx.delegate(&holzi.args).await.expect("connected");
    // The tools of the factory rights: what a model in Standard has, whichever kind it is.
    let expected = default_rights();
    if !expected.operate.is_empty() || expected.commands {
        // The list fills a moment after the connection: ask once it has tools, not before.
        wait_for_tools(&agent).await;
    }
    assert_eq!(
        !tool_names(&agent).await.is_empty(),
        !expected.operate.is_empty() || expected.commands,
        "the factory rights decide the list"
    );
    assert!(
        !tool_names(&agent).await.contains(&WRITE_TOOL.to_owned()),
        "Standard gives no write tool of the settings"
    );
    fx.no_question().await;
    server.stop(CloseReason::Disabled).await;
}

#[test]
fn registered_agent_instructions_direct_web_searches_through_haex() {
    assert!(INSTRUCTIONS_REGISTERED.contains("use it first"));
    assert!(INSTRUCTIONS_REGISTERED.contains("browser_search"));
    assert!(INSTRUCTIONS_REGISTERED.contains("do not claim the whole website is down"));
}

struct BrowserActions;

const BROWSER_ACTION_IDS: &[&str] = &["browser.open", "browser.search"];

#[async_trait]
impl NativeExecutor for BrowserActions {
    fn action_ids(&self) -> &'static [&'static str] {
        BROWSER_ACTION_IDS
    }

    fn serves_external_agents(&self) -> bool {
        true
    }

    async fn run(
        &self,
        _action_id: &str,
        _input: Value,
        _call: &NativeCall,
        _cancel: &CancellationToken,
    ) -> Result<NativeReply, NativeError> {
        Ok(NativeReply::from(Value::Null))
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_claude_delegate_gets_haex_browser_tools_from_its_agent_profile() {
    let mut fx = fixture();
    fx.catalog.replace(
        ["browser.open", "browser.search"]
            .into_iter()
            .map(|action_id| {
                let tool_name = action_id.replace('.', "_");
                serde_json::from_value(json!({
                    "toolName": tool_name,
                    "actionId": action_id,
                    "description": format!("Run {action_id} in haex's agent browser."),
                    "inputSchema": { "type": "object" },
                    "effect": "write",
                    "core": true,
                    "runner": "native",
                    "scope": "browser"
                }))
                .expect("browser action definition")
            })
            .collect(),
    );
    fx.native.add(Arc::new(BrowserActions));
    let server = fx.server().await;
    let rights = Rights {
        browser: Some(BrowserRights::switched_on()),
        ..default_rights()
    };
    active_profile_with(&fx, MODEL, rights).await;

    let holzi = fx
        .delegates
        .server_for(DelegateVendor::Claude, Some(THREAD), MODEL)
        .await
        .expect("the profiled Claude model gets holzi's MCP server");
    let (agent, _helper) = fx.delegate(&holzi.args).await.expect("connected");
    wait_for_tools(&agent).await;
    let names = tool_names(&agent).await;
    assert!(names.contains(&"browser_search".to_owned()), "{names:?}");
    assert!(names.contains(&"browser_open".to_owned()), "{names:?}");
    fx.no_question().await;
    server.stop(CloseReason::Disabled).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn two_models_of_a_provider_have_the_profile_they_are_in_and_one_the_profile_does_not_offer_gets_none(
) {
    let fx = fixture();
    let server = fx.server().await;
    // The active profile offers MODEL only: OTHER_MODEL is not a member, so the chat would refuse
    // it and the delegate gets no server for it.
    active_profile_with(&fx, MODEL, settings_rights()).await;
    assert!(fx
        .delegates
        .server_for(DelegateVendor::Claude, Some(THREAD), MODEL)
        .await
        .is_some());
    assert!(fx
        .delegates
        .server_for(DelegateVendor::Claude, Some(THREAD), OTHER_MODEL)
        .await
        .is_none());
    server.stop(CloseReason::Disabled).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_pass_of_an_unknown_start_opens_nothing_and_with_the_access_off_there_is_no_server() {
    let mut fx = fixture();
    let server = fx.server().await;
    // A pass that was never issued for a start of this process opens nothing.
    let (credential, _guard) = fx
        .delegates
        .issue(DelegatePass {
            kind: AgentKind::CodexDelegate,
            agent: model_delegate_id(fx.device, AgentKind::CodexDelegate, MODEL),
            thread: None,
            model: MODEL.to_owned(),
        })
        .expect("a pass");
    // The technical row does not exist yet: nothing wrote it, so the pass is turned away.
    let args = [
        "--mcp-agent",
        "codex-delegate",
        "--delegate-token",
        credential.as_str(),
    ]
    .map(str::to_owned);
    assert_eq!(refused(&fx, &args).await, exit::REJECTED);
    fx.no_question().await;

    server.stop(CloseReason::Disabled).await;
    assert!(fx
        .delegates
        .server_for(DelegateVendor::Claude, None, MODEL)
        .await
        .is_none());
    assert_eq!(delegates::status(), delegates::DelegateStatus::Available);
}
