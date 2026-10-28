//! Spec 065 T006 (research R8): a native catalog action (ADR 0011) called by an external agent runs
//! in its Rust executor, not in the window, as the agent and in the agent's approval mode; an action
//! that asks the user itself skips the call prompt in Manuell and Auto and is refused in Plan.

#[path = "common/mcp_fixture.rs"]
mod mcp_fixture;

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use mcp_fixture::*;
use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;

use holzi_lib::caller::Caller;
use holzi_lib::chat::tools::action_tool::AgentActionDef;
use holzi_lib::chat::tools::native_action::{NativeCall, NativeError, NativeExecutor, NativeReply};
use holzi_lib::chat::tools::permission::PermissionMode;
use holzi_lib::mcp_server::grants::{OperateLevel, Rights};
use holzi_lib::mcp_server::hello::CloseReason;
use holzi_lib::mcp_server::registry::AgentMode;

/// Keeps every call it got.
#[derive(Default)]
struct Recorder {
    calls: Mutex<Vec<(String, NativeCall)>>,
}

#[async_trait]
impl NativeExecutor for Recorder {
    fn action_ids(&self) -> &'static [&'static str] {
        &["native.look", "native.change"]
    }

    fn confirming_action_ids(&self) -> &'static [&'static str] {
        &["native.change"]
    }

    async fn run(
        &self,
        action_id: &str,
        _input: Value,
        call: &NativeCall,
        _cancel: &CancellationToken,
    ) -> Result<NativeReply, NativeError> {
        self.calls
            .lock()
            .expect("calls")
            .push((action_id.to_owned(), call.clone()));
        Ok(json!({ "native": action_id }).into())
    }
}

fn native(id: &str, scope: &str, effect: &str, confirms: bool) -> AgentActionDef {
    serde_json::from_value(json!({
        "toolName": id.replace('.', "_"),
        "actionId": id,
        "description": "A native action.",
        "inputSchema": { "type": "object" },
        "effect": effect,
        "scope": scope,
        "runner": "native",
        "confirmsItself": confirms,
    }))
    .expect("definition")
}

fn setup(fx: &Fixture) -> Arc<Recorder> {
    let recorder = Arc::new(Recorder::default());
    fx.native.add(recorder.clone());
    let mut catalog = test_catalog();
    catalog.push(native("native.look", "settings.read", "read", false));
    catalog.push(native("native.change", "settings.device", "write", true));
    fx.catalog.replace(catalog);
    recorder
}

fn rights() -> Rights {
    Rights {
        operate: [
            ("settings.read".to_owned(), OperateLevel::Read),
            ("settings.device".to_owned(), OperateLevel::Write),
        ]
        .into_iter()
        .collect(),
        browser: None,
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_native_action_runs_in_rust_as_the_agent_in_its_mode() {
    let mut fx = fixture();
    let recorder = setup(&fx);
    let server = fx.server().await;
    let (agent, _helper) = fx.registered_as("native", rights(), AgentMode::Auto).await;
    let id = fx.agent_id("native").await;

    let result = call(agent.peer(), "native_look").await;
    assert_eq!(error_code(&result), None);
    assert_eq!(
        result.structured_content,
        Some(json!({ "native": "native.look" }))
    );
    assert!(fx.window_calls().is_empty(), "not through the window");
    let calls = recorder.calls.lock().expect("calls").clone();
    assert_eq!(
        calls,
        [(
            "native.look".to_owned(),
            NativeCall {
                caller: Caller::ExternalAgent { id: id.to_string() },
                mode: PermissionMode::Auto,
            }
        )]
    );
    server.stop(CloseReason::Disabled).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_action_that_confirms_itself_skips_the_call_prompt_but_not_plan() {
    let mut fx = fixture();
    let recorder = setup(&fx);
    let server = fx.server().await;
    let (manual, _helper) = fx
        .registered_as("manual", rights(), AgentMode::Manual)
        .await;
    assert_eq!(
        error_code(&call(manual.peer(), "native_change").await),
        None
    );
    fx.no_call_prompt().await;

    let (plan, _helper) = fx.registered_as("plan", rights(), AgentMode::Plan).await;
    assert_eq!(
        error_code(&call(plan.peer(), "native_change").await).as_deref(),
        Some("plan_mode")
    );
    let ran: Vec<String> = recorder
        .calls
        .lock()
        .expect("calls")
        .iter()
        .map(|(id, _)| id.clone())
        .collect();
    assert_eq!(ran, ["native.change"], "Plan never reached the executor");
    server.stop(CloseReason::Disabled).await;
}
