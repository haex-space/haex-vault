use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;

use super::action_tool::AgentActionDef;
use super::native_action::{
    NativeActionTool, NativeCall, NativeError, NativeExecutor, NativeExecutors, NativeReply,
};
use super::{RiskClass, Tool, ToolImage};

/// Answers `files.list` with its input, `files.copy` with an image, fails `files.stat` with a field
/// and never ends `files.read`.
struct Echo;

#[async_trait]
impl NativeExecutor for Echo {
    fn action_ids(&self) -> &'static [&'static str] {
        &["files.list", "files.stat", "files.read", "files.copy"]
    }

    fn confirming_action_ids(&self) -> &'static [&'static str] {
        &["files.list"]
    }

    async fn run(
        &self,
        action_id: &str,
        input: Value,
        _call: &NativeCall,
        _cancel: &CancellationToken,
    ) -> Result<NativeReply, NativeError> {
        match action_id {
            "files.list" => Ok(json!({ "got": input }).into()),
            "files.stat" => Err(NativeError::invalid_input(Some("path"), "no such path")),
            "files.copy" => Ok(NativeReply {
                result: json!({ "note": "an image" }),
                images: vec![ToolImage {
                    name: "a.jpg".to_owned(),
                    media_type: "image/jpeg".to_owned(),
                    bytes: vec![1, 2, 3],
                }],
            }),
            _ => std::future::pending().await,
        }
    }
}

fn def(action_id: &str, effect: &str) -> AgentActionDef {
    serde_json::from_value(json!({
        "toolName": action_id.replace('.', "_"),
        "actionId": action_id,
        "description": "d",
        "inputSchema": { "type": "object" },
        "effect": effect,
        "runner": "native",
    }))
    .expect("a valid definition")
}

#[test]
fn source_risk_and_definition_are_those_of_an_action() {
    for (effect, risk) in [
        ("read", RiskClass::Safe),
        ("write", RiskClass::Change),
        ("destructive", RiskClass::Risky),
    ] {
        let tool = NativeActionTool::new(
            def("files.list", effect),
            Arc::new(Echo),
            NativeExecutors::default(),
        );
        assert_eq!(tool.risk_class(), risk, "{effect}");
        assert_eq!(tool.source(), "action");
        assert_eq!(tool.name(), "files_list");
        assert_eq!(
            tool.action_definition().map(|d| d.action_id.as_str()),
            Some("files.list")
        );
    }
}

#[tokio::test]
async fn a_result_goes_to_the_model_as_json() {
    let tool = NativeActionTool::new(
        def("files.list", "read"),
        Arc::new(Echo),
        NativeExecutors::default(),
    );
    let result = tool
        .execute(json!({ "path": "/a" }), CancellationToken::new())
        .await;
    assert!(!result.is_error);
    assert_eq!(
        serde_json::from_str::<Value>(&result.content).expect("json"),
        json!({ "got": { "path": "/a" } })
    );
}

#[tokio::test]
async fn an_error_carries_code_field_and_message() {
    let tool = NativeActionTool::new(
        def("files.stat", "read"),
        Arc::new(Echo),
        NativeExecutors::default(),
    );
    let result = tool.execute(json!({}), CancellationToken::new()).await;
    assert!(result.is_error);
    assert_eq!(
        serde_json::from_str::<Value>(&result.content).expect("json"),
        json!({ "error": { "code": "invalid_input", "field": "path", "message": "no such path" } })
    );
}

#[tokio::test]
async fn a_cancelled_turn_ends_the_call() {
    let tool = NativeActionTool::new(
        def("files.read", "read"),
        Arc::new(Echo),
        NativeExecutors::default(),
    );
    let cancel = CancellationToken::new();
    cancel.cancel();
    let result = tool.execute(json!({}), cancel).await;
    assert!(result.is_error);
    assert_eq!(result.content, "tool_call_cancelled");
}

#[test]
fn executors_are_found_by_each_of_their_ids() {
    let executors = NativeExecutors::default();
    assert!(executors.get("files.list").is_none());
    executors.add(Arc::new(Echo));
    assert!(executors.get("files.stat").is_some());
    let mut ids = executors.ids();
    ids.sort_unstable();
    assert_eq!(
        ids,
        ["files.copy", "files.list", "files.read", "files.stat"]
    );
}

#[tokio::test]
async fn images_of_a_reply_reach_the_tool_result() {
    let tool = NativeActionTool::new(
        def("files.copy", "write"),
        Arc::new(Echo),
        NativeExecutors::default(),
    );
    let result = tool.execute(json!({}), CancellationToken::new()).await;
    assert_eq!(result.content, r#"{"note":"an image"}"#);
    assert_eq!(result.images.len(), 1);
    assert_eq!(result.images[0].name, "a.jpg");
}

#[test]
fn an_action_confirms_itself_only_when_definition_and_executor_agree() {
    let mut definition = def("files.list", "destructive");
    let tool = NativeActionTool::new(
        definition.clone(),
        Arc::new(Echo),
        NativeExecutors::default(),
    );
    assert!(!tool.confirms_itself());
    definition.confirms_itself = true;
    let tool = NativeActionTool::new(definition, Arc::new(Echo), NativeExecutors::default());
    assert!(tool.confirms_itself());
    assert_eq!(tool.risk_class(), RiskClass::Risky);

    let mut definition = def("files.stat", "destructive");
    definition.confirms_itself = true;
    let tool = NativeActionTool::new(definition, Arc::new(Echo), NativeExecutors::default());
    assert!(!tool.confirms_itself());
}

/// Keeps the call it got.
#[derive(Default)]
struct Recorder {
    seen: std::sync::Mutex<Option<NativeCall>>,
}

#[async_trait]
impl NativeExecutor for Recorder {
    fn action_ids(&self) -> &'static [&'static str] {
        &["files.list"]
    }

    async fn run(
        &self,
        _action_id: &str,
        _input: Value,
        call: &NativeCall,
        _cancel: &CancellationToken,
    ) -> Result<NativeReply, NativeError> {
        *self.seen.lock().expect("seen") = Some(call.clone());
        Ok(json!({}).into())
    }
}

/// Spec 065 T005: the chat calls as the built-in agent in the chat's approval mode, never with a
/// caller from the input.
#[tokio::test]
async fn the_chat_calls_as_the_builtin_agent_in_its_mode() {
    use crate::caller::Caller;
    use crate::chat::tools::permission::PermissionMode;

    let recorder = Arc::new(Recorder::default());
    let executors = NativeExecutors::default();
    let tool = NativeActionTool::new(
        def("files.list", "read"),
        recorder.clone(),
        executors.clone(),
    );
    tool.execute(
        json!({ "caller": "externalAgent" }),
        CancellationToken::new(),
    )
    .await;
    assert_eq!(
        recorder.seen.lock().expect("seen").clone(),
        Some(NativeCall::builtin(PermissionMode::Manual)),
        "Manuell when no mode source is set"
    );

    executors.set_mode_source(Arc::new(|| Box::pin(async { PermissionMode::Auto })));
    tool.execute(json!({}), CancellationToken::new()).await;
    let seen = recorder.seen.lock().expect("seen").clone().expect("called");
    assert_eq!(seen.caller, Caller::BuiltinAgent);
    assert_eq!(seen.mode, PermissionMode::Auto);
}
