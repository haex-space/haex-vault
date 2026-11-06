//! The action bridge inside the tool loop (spec 032 US1, ADR-0006): a model calls an action as a
//! tool, the call goes out as `action-call-request`, the test answers it the way the webview's
//! `respond_action_call` does, and the persisted rows and the tool result show what the model saw.
//! The bridge itself (timeout, cancellation, ordering) is covered by
//! `src/chat/tools/action_bridge_tests.rs`; this file covers it in the loop.

#[allow(dead_code)]
#[path = "common/tool_loop_fixture.rs"]
mod tool_loop_fixture;
use tool_loop_fixture::*;

#[allow(dead_code)]
#[path = "common/action_fixture.rs"]
mod action_fixture;
use action_fixture::*;

use std::time::Duration;

use holzi_lib::adapters::types::ChatRequest;
use holzi_lib::adapters::types::ToolCall as LlmToolCall;
use holzi_lib::adapters::StreamChunk;
use holzi_lib::chat::session::ChatState;
use holzi_lib::chat::tools::action_bridge::{
    ActionBridge, ActionOutcomeWire, ActionReply, ACTION_FAILED_MESSAGE, EVENT_ACTION_CALL_REQUEST,
};
use holzi_lib::chat::tools::offer::core_offer;
use holzi_lib::storage::chat_messages::{self as msg_store, ChatMessage, MessageRole};
use holzi_lib::storage::query;
use serde_json::{json, Value};
use std::sync::Arc;
use uuid::Uuid;

/// Builds a stub model chunk requesting one named tool with a fixed input payload.
fn tool_call(id: &str, name: &str) -> StreamChunk {
    StreamChunk::ToolCalls(vec![LlmToolCall {
        id: id.to_string(),
        name: name.to_string(),
        input: json!({ "x": 1 }),
    }])
}

/// Supplies the stub model's final text and completion metadata after the tool round.
fn final_answer() -> Vec<Result<StreamChunk, holzi_lib::adapters::StreamError>> {
    vec![
        Ok(StreamChunk::Delta {
            content: "final answer".to_string(),
            reasoning: None,
        }),
        Ok(StreamChunk::Done {
            finish_reason: Some("end_turn".to_string()),
            prompt_tokens: Some(10),
            completion_tokens: Some(5),
            ttft_ms: Some(3),
            total_ms: 20,
        }),
    ]
}

struct Turn {
    chat_state: Arc<ChatState>,
    db: haex_crdt::Database,
    thread_id: Uuid,
    handle: tokio::task::JoinHandle<()>,
}

/// Starts a turn in which the stub model calls `calls` (one round), then answers.
async fn start_turn(chat_state: ChatState, calls: Vec<StreamChunk>) -> Turn {
    let steps = vec![calls.into_iter().map(Ok).collect(), final_answer()];
    start_turn_with_steps(chat_state, steps).await.0
}

/// Starts a turn over scripted `steps` and also returns the requests the model received.
async fn start_turn_with_steps(
    chat_state: ChatState,
    steps: Vec<Vec<Result<StreamChunk, holzi_lib::adapters::StreamError>>>,
) -> (Turn, Arc<std::sync::Mutex<Vec<ChatRequest>>>) {
    let db = open_db();
    let thread_id = Uuid::new_v4();
    let user_message_id = Uuid::new_v4();
    seed_thread(&db, thread_id, user_message_id);
    // `Change` and `Safe` run under Auto, which keeps the approval dialog out of these tests.
    set_permission_mode(&db, "auto");
    let chat_state = Arc::new(chat_state);
    let (adapter, requests) = StubAdapter::recording(steps);
    let session = session_with(adapter).await;
    let mut request = base_request();
    request.tools = core_offer(
        &chat_state
            .tool_registry
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()),
    );
    let stream = session.adapter.stream_chat(request.clone()).await.unwrap();
    let (handle, _events) = spawn_turn(
        db.clone(),
        Arc::clone(&chat_state),
        session,
        thread_id,
        user_message_id,
        Uuid::new_v4(),
        request,
        stream,
    );
    let turn = Turn {
        chat_state,
        db,
        thread_id,
        handle,
    };
    (turn, requests)
}

/// Reads the persisted messages for this turn's thread, failing on a storage error.
fn rows(turn: &Turn) -> Vec<ChatMessage> {
    query::read(&turn.db, |r| msg_store::list_messages(r, turn.thread_id)).unwrap()
}

/// Selects the tool-result messages persisted in this turn's thread.
fn tool_results(turn: &Turn) -> Vec<ChatMessage> {
    rows(turn)
        .into_iter()
        .filter(|m| m.role == MessageRole::ToolResult)
        .collect()
}

/// Waits up to ten seconds for an action request and returns its payload.
/// Panics if the channel closes, the wait expires, or a different event arrives.
async fn next_call(events: &mut tokio::sync::mpsc::UnboundedReceiver<(String, Value)>) -> Value {
    let (event, payload) = tokio::time::timeout(Duration::from_secs(10), events.recv())
        .await
        .expect("an action call within 10 s")
        .expect("the channel is open");
    assert_eq!(event, EVENT_ACTION_CALL_REQUEST);
    payload
}

#[tokio::test]
async fn an_action_call_round_trips_and_persists_with_the_source_action() {
    let chat_state = ChatState::new();
    register_action(
        &chat_state,
        action_def("wm_state_get", "wm.state.get", "read"),
    );
    let mut events = attach_action_events(&chat_state);
    let mut turn = start_turn(chat_state, vec![tool_call("call-1", "wm_state_get")]).await;

    let payload = next_call(&mut events).await;
    assert_eq!(payload["actionId"], "wm.state.get");
    assert_eq!(payload["input"], json!({ "x": 1 }));
    assert!(answer(
        &turn.chat_state,
        &payload,
        ActionReply::Ok {
            result: json!({ "windows": [] })
        }
    ));
    (&mut turn.handle).await.unwrap();

    let all = rows(&turn);
    let call = all
        .iter()
        .find(|m| m.role == MessageRole::ToolCall)
        .expect("a tool_call row");
    assert_eq!(call.tool_name.as_deref(), Some("wm_state_get"));
    assert_eq!(call.tool_source.as_deref(), Some("action"));
    let result = &tool_results(&turn)[0];
    assert_eq!(result.tool_is_error, Some(false));
    assert!(result.content.contains("windows"), "{}", result.content);
}

#[tokio::test]
async fn an_action_not_in_the_current_offer_is_not_executed() {
    let chat_state = ChatState::new();
    let mut hidden = action_def("hidden_action", "hidden.action", "read");
    hidden.core = false;
    register_action(&chat_state, hidden);
    let mut events = attach_action_events(&chat_state);
    let mut turn = start_turn(chat_state, vec![tool_call("call-1", "hidden_action")]).await;
    (&mut turn.handle).await.unwrap();

    assert!(
        events.try_recv().is_err(),
        "an unoffered action must not reach the frontend bridge"
    );
    let result = &tool_results(&turn)[0];
    assert!(result.content.contains("unknown tool: hidden_action"));
}

#[tokio::test]
async fn a_runner_error_reaches_the_model_with_code_field_and_message() {
    let chat_state = ChatState::new();
    register_action(&chat_state, action_def("chat_x", "chat.x", "write"));
    let mut events = attach_action_events(&chat_state);
    let mut turn = start_turn(chat_state, vec![tool_call("call-1", "chat_x")]).await;

    let payload = next_call(&mut events).await;
    answer(
        &turn.chat_state,
        &payload,
        ActionReply::Err {
            code: "invalid_input".into(),
            field: Some("text".into()),
            message: "text must be a string".into(),
            next: Some("Correct `text` as the message says and call again.".into()),
        },
    );
    (&mut turn.handle).await.unwrap();

    let result = &tool_results(&turn)[0];
    assert_eq!(result.tool_is_error, Some(true));
    let parsed: Value = serde_json::from_str(&result.content).expect("the content is JSON");
    assert_eq!(parsed["error"]["code"], "invalid_input");
    assert_eq!(parsed["error"]["field"], "text");
    assert_eq!(parsed["error"]["message"], "text must be a string");
    // The runner's next step reaches the model beside the message (spec 082 FR-014).
    assert_eq!(
        parsed["error"]["next"],
        "Correct `text` as the message says and call again."
    );
}

#[tokio::test]
async fn a_failed_handler_shows_the_model_a_fixed_text_not_its_message() {
    let chat_state = ChatState::new();
    register_action(&chat_state, action_def("chat_x", "chat.x", "write"));
    let mut events = attach_action_events(&chat_state);
    let mut turn = start_turn(chat_state, vec![tool_call("call-1", "chat_x")]).await;

    let payload = next_call(&mut events).await;
    // The same conversion `respond_action_call` applies to what the webview sends.
    let wire = ActionOutcomeWire {
        ok: false,
        result: None,
        code: Some("failed".into()),
        field: None,
        message: Some("cannot read /home/someone/vault.db".into()),
        next: None,
        options: None,
    };
    answer(&turn.chat_state, &payload, wire.into());
    (&mut turn.handle).await.unwrap();

    let result = &tool_results(&turn)[0];
    assert!(
        result.content.contains(ACTION_FAILED_MESSAGE),
        "{}",
        result.content
    );
    assert!(!result.content.contains("vault.db"), "{}", result.content);
}

#[tokio::test]
async fn no_answer_ends_as_action_timeout() {
    let mut chat_state = ChatState::new();
    chat_state.action_bridge = ActionBridge::with_timeout(Duration::from_millis(60));
    register_action(
        &chat_state,
        action_def("wm_state_get", "wm.state.get", "read"),
    );
    let _events = attach_action_events(&chat_state);
    let mut turn = start_turn(chat_state, vec![tool_call("call-1", "wm_state_get")]).await;
    (&mut turn.handle).await.unwrap();

    let result = &tool_results(&turn)[0];
    assert_eq!(result.tool_is_error, Some(true));
    let error: Value = serde_json::from_str(&result.content).expect("a JSON error");
    assert_eq!(error["error"]["code"], "action_timeout");
    assert!(error["error"]["next"].is_string(), "{}", result.content);
}

#[tokio::test]
async fn without_an_emitter_the_call_is_action_unavailable() {
    let chat_state = ChatState::new();
    register_action(
        &chat_state,
        action_def("wm_state_get", "wm.state.get", "read"),
    );
    let mut turn = start_turn(chat_state, vec![tool_call("call-1", "wm_state_get")]).await;
    (&mut turn.handle).await.unwrap();

    let result = &tool_results(&turn)[0];
    assert_eq!(result.tool_is_error, Some(true));
    let error: Value = serde_json::from_str(&result.content).expect("a JSON error");
    assert_eq!(error["error"]["code"], "action_unavailable");
    assert!(error["error"]["next"].is_string(), "{}", result.content);
}

#[tokio::test]
async fn closing_the_vault_while_a_call_is_open_cancels_it() {
    let chat_state = ChatState::new();
    register_action(
        &chat_state,
        action_def("wm_state_get", "wm.state.get", "read"),
    );
    let mut events = attach_action_events(&chat_state);
    let mut turn = start_turn(chat_state, vec![tool_call("call-1", "wm_state_get")]).await;

    let payload = next_call(&mut events).await;
    turn.chat_state.reset_for_close();
    (&mut turn.handle).await.unwrap();

    let result = &tool_results(&turn)[0];
    assert_eq!(result.content, "tool_call_cancelled");
    // A late answer to the closed call does nothing.
    assert!(!answer(
        &turn.chat_state,
        &payload,
        ActionReply::Ok {
            result: Value::Null
        }
    ));
}

#[tokio::test]
async fn the_calls_of_one_round_run_one_after_the_other() {
    let chat_state = ChatState::new();
    register_action(&chat_state, action_def("wm_a", "wm.a", "read"));
    register_action(&chat_state, action_def("wm_b", "wm.b", "read"));
    let mut events = attach_action_events(&chat_state);
    let mut turn = start_turn(
        chat_state,
        vec![StreamChunk::ToolCalls(vec![
            LlmToolCall {
                id: "call-a".into(),
                name: "wm_a".into(),
                input: json!({}),
            },
            LlmToolCall {
                id: "call-b".into(),
                name: "wm_b".into(),
                input: json!({}),
            },
        ])],
    )
    .await;

    let first = next_call(&mut events).await;
    // The second call must wait for the first answer.
    assert!(
        tokio::time::timeout(Duration::from_millis(150), events.recv())
            .await
            .is_err(),
        "the second action went out before the first was answered"
    );
    answer(
        &turn.chat_state,
        &first,
        ActionReply::Ok {
            result: Value::Null,
        },
    );
    let second = next_call(&mut events).await;
    assert_ne!(first["actionId"], second["actionId"]);
    answer(
        &turn.chat_state,
        &second,
        ActionReply::Ok {
            result: Value::Null,
        },
    );
    (&mut turn.handle).await.unwrap();
    assert_eq!(tool_results(&turn).len(), 2);
}

fn tool_names(request: &ChatRequest) -> Vec<&str> {
    request
        .tools
        .iter()
        .map(|tool| tool.name.as_str())
        .collect()
}

fn hidden_action(
    tool_name: &str,
    action_id: &str,
) -> holzi_lib::chat::tools::action_tool::AgentActionDef {
    let mut def = action_def(tool_name, action_id, "read");
    def.core = false;
    def.description = format!("hidden action {action_id}");
    def
}

fn registry_with_search(chat_state: &ChatState) {
    register_action(
        chat_state,
        action_def("wm_state_get", "wm.state.get", "read"),
    );
    register_action(chat_state, hidden_action("shelf_one", "shelf.one"));
    register_action(chat_state, hidden_action("drawer_two", "drawer.two"));
    let defs = chat_state.tool_registry.lock().unwrap().action_defs();
    chat_state.tool_registry.lock().unwrap().register(Arc::new(
        holzi_lib::chat::tools::find_actions::FindActionsTool::new(defs),
    ));
}

fn search_call(id: &str, query: &str) -> StreamChunk {
    StreamChunk::ToolCalls(vec![LlmToolCall {
        id: id.to_string(),
        name: "find_actions".to_string(),
        input: json!({ "query": query }),
    }])
}

#[tokio::test]
async fn the_first_request_offers_the_same_core_whatever_is_registered() {
    let chat_state = ChatState::new();
    registry_with_search(&chat_state);
    for index in 0..50 {
        register_action(
            &chat_state,
            hidden_action(&format!("extra_{index}"), &format!("extra.item{index}")),
        );
    }
    let (mut turn, requests) = start_turn_with_steps(chat_state, vec![final_answer()]).await;
    (&mut turn.handle).await.unwrap();
    let first = requests.lock().unwrap()[0].clone();
    let mut names = tool_names(&first);
    names.sort_unstable();
    // `run_command` is not an action: it stays offered, outside the action limit.
    assert_eq!(names, ["find_actions", "run_command", "wm_state_get"]);
}

#[tokio::test]
async fn what_find_actions_finds_is_offered_next_and_a_tool_the_turn_called_stays() {
    let chat_state = ChatState::new();
    registry_with_search(&chat_state);
    let mut events = attach_action_events(&chat_state);
    let steps = vec![
        vec![Ok(search_call("call-1", "shelf"))],
        vec![Ok(tool_call("call-2", "shelf_one"))],
        vec![Ok(search_call("call-3", "drawer"))],
        final_answer(),
    ];
    let (mut turn, requests) = start_turn_with_steps(chat_state, steps).await;

    // The found tool runs: the frontend sees its call.
    let payload = next_call(&mut events).await;
    assert_eq!(payload["actionId"], "shelf.one");
    answer(
        &turn.chat_state,
        &payload,
        ActionReply::Ok { result: json!({}) },
    );
    (&mut turn.handle).await.unwrap();

    let requests = requests.lock().unwrap();
    assert_eq!(requests.len(), 4);
    assert!(!tool_names(&requests[0]).contains(&"shelf_one"));
    assert!(tool_names(&requests[1]).contains(&"shelf_one"));
    assert!(!tool_names(&requests[1]).contains(&"drawer_two"));
    assert!(tool_names(&requests[3]).contains(&"drawer_two"));
    // The turn works with what `shelf_one` returned: a later search does not take it away (#840).
    assert!(tool_names(&requests[3]).contains(&"shelf_one"));
}

#[tokio::test]
async fn a_later_search_replaces_what_an_earlier_one_found_and_the_turn_never_called() {
    let chat_state = ChatState::new();
    registry_with_search(&chat_state);
    let steps = vec![
        vec![Ok(search_call("call-1", "shelf"))],
        vec![Ok(search_call("call-2", "drawer"))],
        final_answer(),
    ];
    let (mut turn, requests) = start_turn_with_steps(chat_state, steps).await;
    (&mut turn.handle).await.unwrap();

    let requests = requests.lock().unwrap();
    assert_eq!(requests.len(), 3);
    assert!(tool_names(&requests[1]).contains(&"shelf_one"));
    assert!(tool_names(&requests[2]).contains(&"drawer_two"));
    assert!(!tool_names(&requests[2]).contains(&"shelf_one"));
}

#[tokio::test]
async fn a_search_in_the_same_round_as_another_tool_still_extends_the_offer() {
    let chat_state = ChatState::new();
    registry_with_search(&chat_state);
    let mut events = attach_action_events(&chat_state);
    let round = StreamChunk::ToolCalls(vec![
        LlmToolCall {
            id: "call-1".into(),
            name: "find_actions".into(),
            input: json!({ "query": "shelf" }),
        },
        LlmToolCall {
            id: "call-2".into(),
            name: "wm_state_get".into(),
            input: json!({}),
        },
    ]);
    let steps = vec![vec![Ok(round)], final_answer()];
    let (mut turn, requests) = start_turn_with_steps(chat_state, steps).await;
    let payload = next_call(&mut events).await;
    answer(
        &turn.chat_state,
        &payload,
        ActionReply::Ok {
            result: json!({"windows": []}),
        },
    );
    (&mut turn.handle).await.unwrap();
    assert!(tool_names(&requests.lock().unwrap()[1]).contains(&"shelf_one"));
}
