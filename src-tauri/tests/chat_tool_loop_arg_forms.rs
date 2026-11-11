//! Values the model writes in the wrong form are taken as the tool's schema means them, before the
//! guard, the dialog, the tool and the stored call (spec 087 US4, contracts/value-forms.md).

#[allow(dead_code)]
#[path = "common/tool_loop_fixture.rs"]
mod tool_loop_fixture;
use tool_loop_fixture::*;

use std::sync::{Arc, Mutex};
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
use holzi_lib::storage::chat_messages::{self as msg_store, MessageRole};
use holzi_lib::storage::query;

const MARK: &str = "x_mail_mark_messages";

/// A mail tool with a yes/no and a number argument; `got` records what it ran with.
struct MarkTool {
    risk: RiskClass,
    got: Arc<Mutex<Option<Value>>>,
}

#[async_trait]
impl Tool for MarkTool {
    fn name(&self) -> &str {
        MARK
    }
    fn description(&self) -> &str {
        "Marks messages"
    }
    fn source(&self) -> &'static str {
        "haextension"
    }
    fn input_schema(&self) -> Value {
        json!({ "type": "object", "properties": {
            "ids": { "type": "array", "items": { "type": "string" } },
            "read": { "type": "boolean" },
            "limit": { "type": "integer" }
        } })
    }
    fn risk_class(&self) -> RiskClass {
        self.risk
    }
    fn origin(&self) -> Option<ToolOrigin> {
        Some(ToolOrigin {
            extension_id: "6f1c1f6e-0000-4000-8000-000000000001".to_owned(),
            extension_name: "Haex Mail".to_owned(),
            tool_title_de: "Markieren".to_owned(),
            tool_title_en: "Mark".to_owned(),
            dev: false,
            example_en: None,
        })
    }
    async fn execute(&self, input: Value, _: CancellationToken) -> ToolResult {
        *self.got.lock().unwrap() = Some(input);
        ToolResult::ok("{}")
    }
}

fn adapter(input: Value) -> StubAdapter {
    StubAdapter::new(vec![
        vec![Ok(StreamChunk::ToolCalls(vec![LlmToolCall {
            id: "call-1".to_owned(),
            name: MARK.to_owned(),
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

fn request() -> ChatRequest {
    let mut request = base_request();
    request.messages = vec![LlmMessage {
        role: ChatRole::User,
        content: "Markier die Mail msg-7 als gelesen.".to_owned(),
        attachments: Vec::new(),
    }];
    request.tools = vec![ToolSpec {
        name: MARK.to_owned(),
        description: String::new(),
        input_schema: json!({ "type": "object" }),
    }];
    request
}

const SENT: fn() -> Value = || json!({ "ids": ["msg-7"], "read": "True", "limit": "5" });
const MEANT: fn() -> Value = || json!({ "ids": ["msg-7"], "read": true, "limit": 5 });

#[tokio::test]
async fn the_tool_and_the_stored_call_get_the_value_as_meant() {
    let db = open_db();
    let (thread_id, user_message_id) = (Uuid::new_v4(), Uuid::new_v4());
    seed_thread(&db, thread_id, user_message_id);
    set_permission_mode(&db, "auto");
    let got = Arc::new(Mutex::new(None));
    let chat_state = ChatState::new();
    chat_state
        .tool_registry
        .lock()
        .unwrap()
        .register(Arc::new(MarkTool {
            risk: RiskClass::Change,
            got: got.clone(),
        }));
    let session = session_with(adapter(SENT())).await;
    let (rows, _) = tokio::time::timeout(
        Duration::from_secs(10),
        run_scripted_turn_with(
            &db,
            &chat_state,
            &session,
            thread_id,
            user_message_id,
            Uuid::new_v4(),
            request(),
        ),
    )
    .await
    .expect("no dialog in auto mode for a change");
    assert_eq!(got.lock().unwrap().clone(), Some(MEANT()));
    let call = rows
        .iter()
        .find(|m| m.role == MessageRole::ToolCall)
        .expect("a tool_call row");
    let stored: Value = serde_json::from_str(call.tool_input.as_deref().unwrap()).unwrap();
    assert_eq!(stored, MEANT());
}

#[tokio::test]
async fn the_dialog_shows_the_value_that_will_be_used() {
    let db = open_db();
    let (thread_id, user_message_id) = (Uuid::new_v4(), Uuid::new_v4());
    seed_thread(&db, thread_id, user_message_id);
    set_permission_mode(&db, "auto");
    let got = Arc::new(Mutex::new(None));
    let chat_state = Arc::new(ChatState::new());
    chat_state
        .tool_registry
        .lock()
        .unwrap()
        .register(Arc::new(MarkTool {
            risk: RiskClass::Risky,
            got: got.clone(),
        }));
    let session = session_with(adapter(SENT())).await;
    let request = request();
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
    assert_eq!(name, "tool-permission-request");
    assert_eq!(payload["toolInput"], MEANT());
    respond(
        &chat_state,
        extract_request_id(&payload),
        ApprovalDecision::Allow,
    );
    handle.await.unwrap();
    assert_eq!(got.lock().unwrap().clone(), Some(MEANT()));
    let rows = query::read(&db, |r| msg_store::list_messages(r, thread_id)).unwrap();
    assert!(rows.iter().any(|m| m.role == MessageRole::ToolCall));
}
