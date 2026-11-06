//! Helpers for the action bridge integration tests (`../action_bridge.rs`, spec 032): an action
//! definition, a registered `ActionTool`, and the events the bridge emits, so a test can play the
//! webview's part and answer them.
//!
//! Included per binary via `#[path = "common/action_fixture.rs"] mod action_fixture;`.

use std::sync::Arc;

use serde_json::Value;
use tokio::sync::mpsc;
use uuid::Uuid;

use holzi_lib::chat::session::ChatState;
use holzi_lib::chat::tools::action_bridge::ActionReply;
use holzi_lib::chat::tools::action_tool::{ActionTool, AgentActionDef};

/// An action definition as the frontend would push it: `effect` is `read`, `write` or
/// `destructive`.
pub fn action_def(tool_name: &str, action_id: &str, effect: &str) -> AgentActionDef {
    serde_json::from_value(serde_json::json!({
        "toolName": tool_name,
        "actionId": action_id,
        "description": "test action",
        "inputSchema": { "type": "object" },
        "effect": effect,
        "core": true,
        "scope": "wm.read",
        "titles": { "de": "Test", "en": "Test" },
    }))
    .expect("a valid action definition")
}

/// Registers one action as a tool of the chat state, backed by its bridge.
pub fn register_action(chat_state: &ChatState, def: AgentActionDef) {
    chat_state
        .tool_registry
        .lock()
        .unwrap()
        .register(Arc::new(ActionTool::new(
            def,
            chat_state.action_bridge.clone(),
        )));
}

/// Makes the bridge emit into a channel instead of a webview.
pub fn attach_action_events(chat_state: &ChatState) -> mpsc::UnboundedReceiver<(String, Value)> {
    let (sender, receiver) = mpsc::unbounded_channel();
    chat_state
        .action_bridge
        .set_emitter(Arc::new(move |event: &str, payload: Value| {
            let _ = sender.send((event.to_owned(), payload));
        }));
    receiver
}

/// The request id of an `action-call-request` payload.
pub fn call_id(payload: &Value) -> Uuid {
    payload["requestId"]
        .as_str()
        .expect("requestId is a string")
        .parse()
        .expect("requestId is a uuid")
}

/// Answers a call the way `respond_action_call` would.
pub fn answer(chat_state: &ChatState, payload: &Value, reply: ActionReply) -> bool {
    chat_state.action_bridge.resolve(call_id(payload), reply)
}
