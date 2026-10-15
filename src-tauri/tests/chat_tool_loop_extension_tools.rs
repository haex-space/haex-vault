//! An extension's tool in the turn loop (spec 018, US3): its approval prompt and its rows name the
//! extension (T033), and a tool the user confirmed with "ask" needs an approval in every mode,
//! "Plan" included (T035). Kept apart from `chat_tool_loop_permissions.rs`, which is past the
//! 500-line limit and has a split plan of its own.

#[allow(dead_code)]
#[path = "common/tool_loop_fixture.rs"]
mod tool_loop_fixture;
use tool_loop_fixture::*;

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use holzi_lib::adapters::types::ToolCall as LlmToolCall;
use holzi_lib::adapters::StreamChunk;
use holzi_lib::chat::session::ChatState;
use holzi_lib::chat::tools::{ApprovalDecision, RiskClass, Tool, ToolOrigin, ToolResult};
use holzi_lib::storage::chat_messages::{self as msg_store, MessageRole};
use holzi_lib::storage::query;

const NAME: &str = "x_mail_count_unread";

/// An extension's read tool, confirmed with `granted` or `ask`.
struct MailTool {
    always_ask: bool,
}

fn origin() -> ToolOrigin {
    ToolOrigin {
        extension_id: "6f1c1f6e-0000-4000-8000-000000000001".to_owned(),
        extension_name: "Haex Mail".to_owned(),
        tool_title_de: "Ungelesene zählen".to_owned(),
        tool_title_en: "Count unread".to_owned(),
        dev: false,
    }
}

#[async_trait]
impl Tool for MailTool {
    fn name(&self) -> &str {
        NAME
    }
    fn description(&self) -> &str {
        "Counts unread mails"
    }
    fn source(&self) -> &'static str {
        "haextension"
    }
    fn input_schema(&self) -> Value {
        json!({ "type": "object" })
    }
    fn risk_class(&self) -> RiskClass {
        RiskClass::Safe
    }
    fn origin(&self) -> Option<ToolOrigin> {
        Some(origin())
    }
    fn always_ask(&self) -> bool {
        self.always_ask
    }
    async fn execute(&self, _: Value, _: CancellationToken) -> ToolResult {
        ToolResult::ok(r#"{"extension":"Haex Mail","data":"3"}"#)
    }
}

fn adapter() -> StubAdapter {
    StubAdapter::new(vec![
        vec![Ok(StreamChunk::ToolCalls(vec![LlmToolCall {
            id: "call-1".to_owned(),
            name: NAME.to_owned(),
            input: json!({}),
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

struct Turn {
    db: haex_crdt::Database,
    chat_state: Arc<ChatState>,
    thread_id: Uuid,
    handle: tokio::task::JoinHandle<()>,
    events: tokio::sync::mpsc::UnboundedReceiver<(String, Value)>,
}

/// Starts a turn in `mode` whose model calls the mail tool once.
async fn start(mode: &str, always_ask: bool) -> Turn {
    let db = open_db();
    let (thread_id, user_message_id) = (Uuid::new_v4(), Uuid::new_v4());
    seed_thread(&db, thread_id, user_message_id);
    set_permission_mode(&db, mode);
    let chat_state = Arc::new(ChatState::new());
    chat_state
        .tool_registry
        .lock()
        .unwrap()
        .register(Arc::new(MailTool { always_ask }));
    let session = session_with(adapter()).await;
    let mut request = base_request();
    request.tools[0].name = NAME.to_owned();
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

/// T035: "ask" asks in every mode, even where a read tool would run at once ("auto") or would be
/// allowed in "plan".
#[tokio::test]
async fn a_tool_confirmed_with_ask_asks_in_every_mode() {
    for mode in ["manual", "auto", "plan"] {
        let mut turn = start(mode, true).await;
        let (name, payload) = turn.events.recv().await.expect("an event");
        assert_eq!(name, "tool-permission-request", "{mode}");
        assert_eq!(payload["toolName"], NAME);
        respond(
            &turn.chat_state,
            extract_request_id(&payload),
            ApprovalDecision::Allow,
        );
        turn.handle.await.unwrap();
    }
}

/// A granted read tool in "auto" runs without a prompt, as before.
#[tokio::test]
async fn a_granted_tool_follows_the_mode() {
    let mut turn = start("auto", false).await;
    let (name, _) = turn.events.recv().await.expect("an event");
    assert_ne!(name, "tool-permission-request");
    turn.handle.await.unwrap();
}

/// T033: the prompt, the call event and the stored call row name the extension and the tool.
#[tokio::test]
async fn the_prompt_the_event_and_the_row_carry_the_tool_origin() {
    let mut turn = start("manual", false).await;
    let (name, payload) = turn.events.recv().await.expect("an event");
    assert_eq!(name, "tool-permission-request");
    assert_eq!(payload["toolSource"], "haextension");
    assert_eq!(payload["toolOrigin"]["extensionName"], "Haex Mail");
    assert_eq!(payload["toolOrigin"]["toolTitleDe"], "Ungelesene zählen");
    respond(
        &turn.chat_state,
        extract_request_id(&payload),
        ApprovalDecision::Allow,
    );
    turn.handle.await.unwrap();

    let mut call_event = None;
    while let Ok((name, payload)) = turn.events.try_recv() {
        if name == "chat-tool-call" {
            call_event = Some(payload);
        }
    }
    let call_event = call_event.expect("a chat-tool-call event");
    assert_eq!(
        call_event["toolOrigin"],
        serde_json::to_value(origin()).unwrap()
    );

    let rows = query::read(&turn.db, |r| msg_store::list_messages(r, turn.thread_id)).unwrap();
    let call = rows
        .iter()
        .find(|m| m.role == MessageRole::ToolCall)
        .expect("tool_call row");
    let stored: ToolOrigin =
        serde_json::from_str(call.tool_origin.as_deref().expect("stored origin")).unwrap();
    assert_eq!(stored, origin());
    let result = rows
        .iter()
        .find(|m| m.role == MessageRole::ToolResult)
        .expect("tool_result row");
    assert_eq!(result.tool_origin, None);
}
