use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;

use super::register_agent_actions;
use crate::chat::session::ChatState;
use crate::chat::tools::action_tool::AgentActionDef;
use crate::chat::tools::native_action::{NativeError, NativeExecutor, NativeReply};

struct Files;

#[async_trait]
impl NativeExecutor for Files {
    fn action_ids(&self) -> &'static [&'static str] {
        &["files.list"]
    }

    async fn run(
        &self,
        _action_id: &str,
        _input: Value,
        _cancel: &CancellationToken,
    ) -> Result<NativeReply, NativeError> {
        Ok(json!({ "native": true }).into())
    }
}

fn def(action_id: &str, native: bool) -> AgentActionDef {
    let mut def = json!({
        "toolName": action_id.replace('.', "_"),
        "actionId": action_id,
        "description": "d",
        "inputSchema": { "type": "object" },
        "effect": "read",
    });
    if native {
        def["runner"] = json!("native");
    }
    serde_json::from_value(def).expect("a valid definition")
}

fn action_tools(chat: &ChatState) -> Vec<String> {
    let registry = chat.tool_registry.lock().expect("registry");
    let mut names: Vec<String> = registry
        .iter()
        .filter(|tool| tool.source() == "action")
        .map(|tool| tool.name().to_owned())
        .collect();
    names.sort();
    names
}

#[tokio::test]
async fn a_native_action_runs_its_executor_without_the_window() {
    let chat = ChatState::new();
    chat.native_actions.add(Arc::new(Files));
    register_agent_actions(
        &chat,
        vec![def("files.list", true), def("wm.state.get", false)],
    )
    .expect("registered");
    let tool = chat
        .tool_registry
        .lock()
        .expect("registry")
        .get("files_list")
        .expect("the tool");
    // The bridge has no emitter here: a window action would answer `action_unavailable`.
    let result = tool.execute(json!({}), CancellationToken::new()).await;
    assert_eq!(result.content, r#"{"native":true}"#);
}

#[test]
fn a_native_action_without_executor_is_refused_and_changes_nothing() {
    let chat = ChatState::new();
    register_agent_actions(&chat, vec![def("wm.state.get", false)]).expect("registered");
    let before = action_tools(&chat);
    let error =
        register_agent_actions(&chat, vec![def("files.list", true)]).expect_err("no executor");
    assert!(error.to_string().contains("files.list"), "{error}");
    assert_eq!(action_tools(&chat), before);
}

#[test]
fn an_executor_without_definition_is_refused() {
    let chat = ChatState::new();
    chat.native_actions.add(Arc::new(Files));
    let error = register_agent_actions(&chat, vec![def("wm.state.get", false)])
        .expect_err("files.list has no definition");
    assert!(error.to_string().contains("files.list"), "{error}");
    // Pushed as a window action it is no definition of the executor either.
    assert!(register_agent_actions(&chat, vec![def("files.list", false)]).is_err());
}
