//! The tool round asks the user when a call needs a choice (spec 046, US2, FR-005–FR-012): an
//! option or the user's own words run the call again with that value, a declined question ends as
//! `declined_by_user`, Manual mode asks for approval only once, and a cancelled turn leaves no rows.
//! Uses the shared `tests/common/tool_loop_fixture.rs` (`StubAdapter`, `spawn_turn`).

#[allow(dead_code)]
#[path = "common/tool_loop_fixture.rs"]
mod tool_loop_fixture;
use tool_loop_fixture::*;

use std::sync::Arc;

use async_trait::async_trait;
use haex_lib::adapters::types::ToolCall as LlmToolCall;
use haex_lib::adapters::types::ToolSpec;
use haex_lib::adapters::StreamChunk;
use haex_lib::chat::choices::ChoiceAnswer;
use haex_lib::chat::commands::abort_turn;
use haex_lib::chat::session::ChatState;
use haex_lib::chat::tools::ask_user::AskUserTool;
use haex_lib::chat::tools::{
    ApprovalDecision, ChoiceOption, ChoiceRequest, RiskClass, Tool, ToolResult,
};
use haex_lib::storage::chat_messages::{self as msg_store, ChatMessage, FinishReason, MessageRole};
use haex_lib::storage::query;
use serde_json::{json, Value};
use tokio::sync::mpsc::UnboundedReceiver;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

/// Opens an "app": `haex` and `again` fit several and ask to choose, anything else opens.
struct OpenApp;

#[async_trait]
impl Tool for OpenApp {
    fn name(&self) -> &str {
        "open_app"
    }

    fn description(&self) -> &str {
        "test-only app opener"
    }

    fn source(&self) -> &'static str {
        "cli"
    }

    fn input_schema(&self) -> Value {
        json!({ "type": "object" })
    }

    fn risk_class(&self) -> RiskClass {
        RiskClass::Change
    }

    async fn execute(&self, input: Value, _cancel: CancellationToken) -> ToolResult {
        let app = input["appId"].as_str().unwrap_or_default().to_owned();
        if app == "haex" || app == "again" {
            return ToolResult::needs_choice(
                json!({ "error": { "code": "needs_choice" } }).to_string(),
                ChoiceRequest {
                    question: None,
                    field: Some("appId".into()),
                    value: app,
                    options: vec![
                        ChoiceOption {
                            value: "extension.mail".into(),
                            label: "haex-mail".into(),
                            unavailable: None,
                        },
                        ChoiceOption {
                            value: "extension.notes".into(),
                            label: "haex-notes".into(),
                            unavailable: None,
                        },
                    ],
                },
            );
        }
        ToolResult::ok(json!({ "opened": app }).to_string())
    }
}

struct Turn {
    db: haex_crdt::Database,
    chat_state: Arc<ChatState>,
    thread_id: Uuid,
    handle: tokio::task::JoinHandle<()>,
    events: UnboundedReceiver<(String, Value)>,
}

/// A turn whose model calls `open_app` with `appId: "haex"` once, then ends.
async fn start(mode: &str) -> Turn {
    start_with(mode, "open_app", json!({ "appId": "haex" })).await
}

/// A turn whose model calls `tool` with `input` once, then ends.
async fn start_with(mode: &str, tool: &str, input: Value) -> Turn {
    let db = open_db();
    let thread_id = Uuid::new_v4();
    let user_message_id = Uuid::new_v4();
    seed_thread(&db, thread_id, user_message_id);
    set_permission_mode(&db, mode);

    let chat_state = Arc::new(ChatState::new());
    chat_state
        .tool_registry
        .lock()
        .unwrap()
        .register(Arc::new(OpenApp));
    chat_state
        .tool_registry
        .lock()
        .unwrap()
        .register(Arc::new(AskUserTool));

    let adapter = StubAdapter::new(vec![
        vec![Ok(StreamChunk::ToolCalls(vec![LlmToolCall {
            id: "call-1".to_string(),
            name: tool.to_string(),
            input,
        }]))],
        vec![Ok(StreamChunk::Done {
            finish_reason: Some("end_turn".to_string()),
            prompt_tokens: Some(1),
            completion_tokens: Some(1),
            ttft_ms: Some(1),
            total_ms: 1,
        })],
    ]);
    let session = session_with(adapter).await;
    let mut request = base_request();
    // `ask_user` is an action-source tool: the round runs it only when the request offered it.
    request.tools.push(ToolSpec {
        name: "ask_user".to_string(),
        description: String::new(),
        input_schema: json!({ "type": "object" }),
    });
    let stream = session.adapter.stream_chat(request.clone()).await.unwrap();
    let (handle, events) = spawn_turn(
        db.clone(),
        chat_state.clone(),
        session,
        thread_id,
        user_message_id,
        Uuid::new_v4(),
        request,
        stream,
    );
    Turn {
        db,
        chat_state,
        thread_id,
        handle,
        events,
    }
}

impl Turn {
    /// The next event called `name`; every event before it is returned too, oldest first.
    async fn until(&mut self, name: &str) -> (Value, Vec<String>) {
        let mut before = Vec::new();
        loop {
            let (event, payload) = self.events.recv().await.expect("an event");
            if event == name {
                return (payload, before);
            }
            before.push(event);
        }
    }

    fn answer(&self, request: &Value, answer: ChoiceAnswer) {
        self.chat_state
            .pending_choices
            .resolve(extract_request_id(request), answer)
            .expect("an open question");
    }

    /// Waits for the end of the turn and returns its rows and the events it still emitted.
    async fn finish(mut self) -> (Vec<ChatMessage>, Vec<String>) {
        self.handle.await.unwrap();
        let mut rest = Vec::new();
        while let Ok((event, _)) = self.events.try_recv() {
            rest.push(event);
        }
        let rows = query::read(&self.db, |r| msg_store::list_messages(r, self.thread_id)).unwrap();
        (rows, rest)
    }
}

fn tool_result(rows: &[ChatMessage]) -> &ChatMessage {
    rows.iter()
        .find(|m| m.role == MessageRole::ToolResult)
        .expect("a tool_result row")
}

#[tokio::test]
async fn an_option_runs_the_call_again_with_that_value() {
    let mut turn = start("auto").await;
    let (request, _) = turn.until("chat-choice-request").await;
    assert_eq!(request["toolName"], "open_app");
    assert_eq!(request["threadId"], turn.thread_id.to_string());
    assert_eq!(request["field"], "appId");
    assert_eq!(request["value"], "haex");
    assert_eq!(request["question"], Value::Null);
    assert_eq!(request["options"][1]["label"], "haex-notes");

    turn.answer(
        &request,
        ChoiceAnswer::Option {
            value: "extension.notes".into(),
        },
    );
    let (rows, rest) = turn.finish().await;
    assert!(!rest.iter().any(|e| e == "chat-choice-request"));
    let result = tool_result(&rows);
    assert_eq!(result.tool_is_error, Some(false));
    let content: Value = serde_json::from_str(&result.content).unwrap();
    assert_eq!(
        content,
        json!({
            "result": { "opened": "extension.notes" },
            "choice": { "value": "haex", "answer": "extension.notes" },
        })
    );
}

#[tokio::test]
async fn own_words_that_are_ambiguous_again_lead_to_a_second_question() {
    let mut turn = start("auto").await;
    let (first, _) = turn.until("chat-choice-request").await;
    turn.answer(
        &first,
        ChoiceAnswer::Text {
            text: "again".into(),
        },
    );
    let (second, _) = turn.until("chat-choice-request").await;
    assert_eq!(second["value"], "again");
    assert_ne!(second["requestId"], first["requestId"]);
    turn.answer(
        &second,
        ChoiceAnswer::Option {
            value: "extension.mail".into(),
        },
    );
    let (rows, _) = turn.finish().await;
    let content: Value = serde_json::from_str(&tool_result(&rows).content).unwrap();
    assert_eq!(content["result"]["opened"], "extension.mail");
    assert_eq!(content["choice"]["value"], "haex");
}

#[tokio::test]
async fn a_declined_question_runs_nothing() {
    let mut turn = start("auto").await;
    let (request, _) = turn.until("chat-choice-request").await;
    turn.answer(&request, ChoiceAnswer::Cancel);
    let (rows, _) = turn.finish().await;
    let result = tool_result(&rows);
    assert_eq!(result.tool_is_error, Some(true));
    assert_eq!(result.content, "declined_by_user");
}

#[tokio::test]
async fn manual_mode_asks_for_approval_once_and_not_again_after_the_choice() {
    let mut turn = start("manual").await;
    let (approval, _) = turn.until("tool-permission-request").await;
    let approval_id = extract_request_id(&approval);
    let sender = turn
        .chat_state
        .pending_tool_approvals
        .lock()
        .unwrap()
        .remove(&approval_id)
        .expect("the approval");
    let _ = sender.send(ApprovalDecision::Allow);

    let (request, before) = turn.until("chat-choice-request").await;
    assert!(!before.iter().any(|e| e == "tool-permission-request"));
    turn.answer(
        &request,
        ChoiceAnswer::Option {
            value: "extension.mail".into(),
        },
    );
    let (rows, rest) = turn.finish().await;
    assert!(!rest.iter().any(|e| e == "tool-permission-request"));
    assert_eq!(tool_result(&rows).tool_is_error, Some(false));
}

#[tokio::test]
async fn cancelling_the_turn_while_a_question_is_open_leaves_no_tool_rows() {
    let mut turn = start("auto").await;
    let (request, _) = turn.until("chat-choice-request").await;
    abort_turn(&turn.chat_state).unwrap();
    let chat_state = turn.chat_state.clone();
    let (rows, _) = turn.finish().await;
    assert!(!rows
        .iter()
        .any(|m| m.role == MessageRole::ToolCall || m.role == MessageRole::ToolResult));
    assert!(rows
        .iter()
        .any(|m| m.finish_reason == Some(FinishReason::Cancelled)));
    chat_state
        .pending_choices
        .resolve(extract_request_id(&request), ChoiceAnswer::Cancel)
        .expect("a late answer to a cancelled question is no error");
}

async fn ask(mode: &str) -> Turn {
    start_with(
        mode,
        "ask_user",
        json!({ "question": "Which colour scheme?", "options": ["Dark", "Light"] }),
    )
    .await
}

#[tokio::test]
async fn the_agent_asks_without_an_approval_even_in_manual_mode() {
    let mut turn = ask("manual").await;
    let (request, before) = turn.until("chat-choice-request").await;
    assert!(!before.iter().any(|e| e == "tool-permission-request"));
    assert_eq!(request["question"], "Which colour scheme?");
    assert_eq!(request["field"], Value::Null);
    assert_eq!(request["options"][0]["label"], "Dark");
    turn.answer(
        &request,
        ChoiceAnswer::Option {
            value: "Dark".into(),
        },
    );
    let (rows, rest) = turn.finish().await;
    assert!(!rest.iter().any(|e| e == "tool-permission-request"));
    let result = tool_result(&rows);
    assert_eq!(result.tool_is_error, Some(false));
    assert_eq!(
        serde_json::from_str::<Value>(&result.content).unwrap(),
        json!({ "answer": "Dark" })
    );
}

#[tokio::test]
async fn own_words_answer_the_agent_as_free_text() {
    let mut turn = ask("auto").await;
    let (request, _) = turn.until("chat-choice-request").await;
    turn.answer(
        &request,
        ChoiceAnswer::Text {
            text: "the darker accent".into(),
        },
    );
    let (rows, _) = turn.finish().await;
    assert_eq!(
        serde_json::from_str::<Value>(&tool_result(&rows).content).unwrap(),
        json!({ "answer": "the darker accent", "freeText": true })
    );
}

#[tokio::test]
async fn a_declined_question_of_the_agent_is_declined_by_user() {
    let mut turn = ask("auto").await;
    let (request, _) = turn.until("chat-choice-request").await;
    turn.answer(&request, ChoiceAnswer::Cancel);
    let (rows, _) = turn.finish().await;
    assert_eq!(tool_result(&rows).content, "declined_by_user");
}
