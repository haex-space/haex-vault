//! The guard of `run_turn` (spec 082 US4, T022): a made-up id and an unknown tool are turned away
//! before the permission dialog, in every mode; a call the user asked for still meets the dialog as
//! before (FR-013). Kept apart from `chat_tool_loop_permissions.rs`, which is past the 500-line rule.

#[allow(dead_code)]
#[path = "common/tool_loop_fixture.rs"]
mod tool_loop_fixture;
use tool_loop_fixture::*;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use holzi_lib::adapters::types::{ChatMessage as LlmMessage, ChatRole, ToolCall as LlmToolCall};
use holzi_lib::adapters::types::{ChatRequest, ToolSpec};
use holzi_lib::adapters::StreamChunk;
use holzi_lib::chat::session::ChatState;
use holzi_lib::chat::tools::{ApprovalDecision, RiskClass, Tool, ToolOrigin, ToolResult};
use holzi_lib::storage::chat_messages::{ChatMessage, MessageRole};

const DELETE: &str = "x_mail_delete_messages";

/// haex-mail's deletion, as far as the turn sees it; `ran` records whether it executed.
struct DeleteTool {
    ran: Arc<AtomicBool>,
}

#[async_trait]
impl Tool for DeleteTool {
    fn name(&self) -> &str {
        DELETE
    }
    fn description(&self) -> &str {
        "Deletes messages for good"
    }
    fn source(&self) -> &'static str {
        "haextension"
    }
    fn input_schema(&self) -> Value {
        json!({ "type": "object", "properties": { "ids": { "type": "array" } } })
    }
    fn risk_class(&self) -> RiskClass {
        RiskClass::Risky
    }
    fn origin(&self) -> Option<ToolOrigin> {
        Some(ToolOrigin {
            extension_id: "6f1c1f6e-0000-4000-8000-000000000001".to_owned(),
            extension_name: "Haex Mail".to_owned(),
            tool_title_de: "Löschen".to_owned(),
            tool_title_en: "Delete".to_owned(),
            dev: false,
            example_en: None,
        })
    }
    async fn execute(&self, _: Value, _: CancellationToken) -> ToolResult {
        self.ran.store(true, Ordering::SeqCst);
        ToolResult::ok("{}")
    }
}

fn one_call(name: &str, input: Value) -> StubAdapter {
    StubAdapter::new(vec![
        vec![Ok(StreamChunk::ToolCalls(vec![LlmToolCall {
            id: "call-1".to_owned(),
            name: name.to_owned(),
            input,
        }]))],
        vec![Ok(StreamChunk::Done {
            finish_reason: Some("end_turn".to_owned()),
            prompt_tokens: None,
            completion_tokens: None,
            ttft_ms: None,
            total_ms: 1,
        })],
    ])
}

/// The same call twice in a row, then the end: the model repeats what the guard turned away.
fn call_twice(name: &str, input: Value) -> StubAdapter {
    let call = |id: &str| {
        vec![Ok(StreamChunk::ToolCalls(vec![LlmToolCall {
            id: id.to_owned(),
            name: name.to_owned(),
            input: input.clone(),
        }]))]
    };
    StubAdapter::new(vec![
        call("call-1"),
        call("call-2"),
        vec![Ok(StreamChunk::Done {
            finish_reason: Some("end_turn".to_owned()),
            prompt_tokens: None,
            completion_tokens: None,
            ttft_ms: None,
            total_ms: 1,
        })],
    ])
}

fn spec(name: &str) -> ToolSpec {
    ToolSpec {
        name: name.to_owned(),
        description: String::new(),
        input_schema: json!({ "type": "object" }),
    }
}

/// A request whose user said `said` and whose step offered `offered`.
fn request(said: &str, offered: &[&str]) -> ChatRequest {
    let mut request = base_request();
    request.messages = vec![LlmMessage {
        role: ChatRole::User,
        content: said.to_owned(),
        attachments: Vec::new(),
    }];
    request.tools = offered.iter().map(|name| spec(name)).collect();
    request
}

/// Runs a turn that cannot wait on the user: a dialog would hang it, so the timeout fails the test.
async fn turn_without_dialog(
    mode: &str,
    adapter: StubAdapter,
    request: ChatRequest,
    ran: &Arc<AtomicBool>,
) -> (Vec<ChatMessage>, Vec<(String, Value)>) {
    let db = open_db();
    let (thread_id, user_message_id) = (Uuid::new_v4(), Uuid::new_v4());
    seed_thread(&db, thread_id, user_message_id);
    set_permission_mode(&db, mode);
    let chat_state = ChatState::new();
    chat_state
        .tool_registry
        .lock()
        .unwrap()
        .register(Arc::new(DeleteTool { ran: ran.clone() }));
    let session = session_with(adapter).await;
    let turn = run_scripted_turn_with(
        &db,
        &chat_state,
        &session,
        thread_id,
        user_message_id,
        Uuid::new_v4(),
        request,
    );
    tokio::time::timeout(Duration::from_secs(10), turn)
        .await
        .unwrap_or_else(|_| panic!("{mode}: the turn waited on a dialog"))
}

fn tool_result(rows: &[ChatMessage]) -> &ChatMessage {
    rows.iter()
        .find(|m| m.role == MessageRole::ToolResult)
        .expect("a tool_result row")
}

#[tokio::test]
async fn a_made_up_id_is_turned_away_without_a_dialog_in_every_mode() {
    for mode in ["manual", "auto", "plan"] {
        let ran = Arc::new(AtomicBool::new(false));
        let (rows, events) = turn_without_dialog(
            mode,
            one_call(DELETE, json!({ "ids": ["12345"] })),
            request("Lösch die Mail von Bernd endgültig.", &[DELETE]),
            &ran,
        )
        .await;
        assert!(
            !events.iter().any(|(n, _)| n == "tool-permission-request"),
            "{mode}: no dialog"
        );
        assert!(!ran.load(Ordering::SeqCst), "{mode}: the tool must not run");
        let result = tool_result(&rows);
        assert_eq!(result.tool_is_error, Some(true), "{mode}");
        assert!(
            result.content.contains("turned away"),
            "{mode}: {}",
            result.content
        );
        assert!(
            result.content.contains("12345"),
            "{mode}: {}",
            result.content
        );
    }
}

/// The refusal repeats the made-up id; that must not make the id known: the same call in the next
/// round is turned away again, without a dialog and without running.
#[tokio::test]
async fn a_refused_id_does_not_become_known_by_its_refusal() {
    for mode in ["manual", "auto"] {
        let ran = Arc::new(AtomicBool::new(false));
        let (rows, events) = turn_without_dialog(
            mode,
            call_twice(DELETE, json!({ "ids": ["12345"] })),
            request("Lösch die Mail von Bernd endgültig.", &[DELETE]),
            &ran,
        )
        .await;
        assert!(
            !events.iter().any(|(n, _)| n == "tool-permission-request"),
            "{mode}: no dialog in any round"
        );
        assert!(!ran.load(Ordering::SeqCst), "{mode}: the tool must not run");
        let refused: Vec<_> = rows
            .iter()
            .filter(|m| m.role == MessageRole::ToolResult)
            .collect();
        assert_eq!(refused.len(), 2, "{mode}: one answer per round");
        assert!(
            refused
                .iter()
                .all(|m| m.tool_is_error == Some(true) && m.content.contains("turned away")),
            "{mode}: {refused:?}"
        );
    }
}

#[tokio::test]
async fn an_unknown_tool_is_answered_with_the_closest_offered_names() {
    let ran = Arc::new(AtomicBool::new(false));
    let (rows, events) = turn_without_dialog(
        "auto",
        one_call("x_mail_list_messages", json!({})),
        request(
            "Was schreibt die Hausverwaltung?",
            &[
                "wm_apps_list",
                "x_mail_find_messages",
                "x_mail_list_mailboxes",
                DELETE,
            ],
        ),
        &ran,
    )
    .await;
    assert!(!events.iter().any(|(n, _)| n == "tool-permission-request"));
    let result = tool_result(&rows);
    assert_eq!(result.tool_is_error, Some(true));
    assert!(
        result
            .content
            .starts_with("unknown tool: x_mail_list_messages"),
        "{}",
        result.content
    );
    assert!(
        result.content.contains("x_mail_find_messages")
            && result.content.contains("x_mail_list_mailboxes"),
        "{}",
        result.content
    );
}

/// FR-013: an id the user named passes the guard, and the dialog decides as before.
#[tokio::test]
async fn an_asked_for_risky_call_still_meets_the_dialog() {
    for mode in ["manual", "auto"] {
        let db = open_db();
        let (thread_id, user_message_id) = (Uuid::new_v4(), Uuid::new_v4());
        seed_thread(&db, thread_id, user_message_id);
        set_permission_mode(&db, mode);
        let ran = Arc::new(AtomicBool::new(false));
        let chat_state = Arc::new(ChatState::new());
        chat_state
            .tool_registry
            .lock()
            .unwrap()
            .register(Arc::new(DeleteTool { ran: ran.clone() }));
        let session = session_with(one_call(DELETE, json!({ "ids": ["msg-7"] }))).await;
        let request = request("Lösch die Nachricht msg-7 endgültig.", &[DELETE]);
        let stream = session.adapter.stream_chat(request.clone()).await.unwrap();
        let (handle, mut events) = spawn_turn(
            db.clone(),
            chat_state.clone(),
            session,
            thread_id,
            user_message_id,
            Uuid::new_v4(),
            request,
            stream,
        );
        let (name, payload) = events.recv().await.expect("an event");
        assert_eq!(name, "tool-permission-request", "{mode}");
        assert_eq!(payload["toolName"], DELETE);
        respond(
            &chat_state,
            extract_request_id(&payload),
            ApprovalDecision::Allow,
        );
        handle.await.unwrap();
        assert!(ran.load(Ordering::SeqCst), "{mode}: the agreed call runs");
    }
    // Plan mode blocks a risky call as before, without a dialog.
    let ran = Arc::new(AtomicBool::new(false));
    let (rows, _) = turn_without_dialog(
        "plan",
        one_call(DELETE, json!({ "ids": ["msg-7"] })),
        request("Lösch die Nachricht msg-7 endgültig.", &[DELETE]),
        &ran,
    )
    .await;
    assert_eq!(tool_result(&rows).content, "blocked_by_plan_mode");
    assert!(!ran.load(Ordering::SeqCst));
}
