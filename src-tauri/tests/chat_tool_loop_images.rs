//! Images in tool results (spec 044 FR-035a, research R2): a model that accepts images gets them as
//! blocks next to the result, any other model a note in their place; the history keeps only the
//! text and a note, never the image.

#[allow(dead_code)]
#[path = "common/tool_loop_fixture.rs"]
mod tool_loop_fixture;
use tool_loop_fixture::*;

use std::sync::Arc;

use async_trait::async_trait;
use holzi_lib::adapters::types::{AttachmentKind, ChatRole, ToolCall as LlmToolCall};
use holzi_lib::adapters::StreamChunk;
use holzi_lib::chat::session::ChatState;
use holzi_lib::chat::tools::{RiskClass, Tool, ToolImage, ToolResult};
use holzi_lib::model_capabilities::ModelCapabilities;
use holzi_lib::storage::chat_messages::MessageRole;
use serde_json::Value;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

/// A tool that answers with a text and one image.
struct Photo;

#[async_trait]
impl Tool for Photo {
    fn name(&self) -> &str {
        "echo"
    }

    fn description(&self) -> &str {
        "shows a photo"
    }

    fn source(&self) -> &'static str {
        "cli"
    }

    fn input_schema(&self) -> Value {
        serde_json::json!({ "type": "object" })
    }

    fn risk_class(&self) -> RiskClass {
        RiskClass::Safe
    }

    async fn execute(&self, _input: Value, _cancel: CancellationToken) -> ToolResult {
        ToolResult::ok("a photo").with_images(vec![ToolImage {
            name: "foto.jpg".to_string(),
            media_type: "image/jpeg".to_string(),
            bytes: vec![1, 2, 3],
        }])
    }
}

/// Runs one tool call and a final answer; returns the persisted tool result and the message the
/// adapter got for it in the second request.
async fn turn(
    accepts: Option<Vec<AttachmentKind>>,
) -> (String, holzi_lib::adapters::types::ChatMessage) {
    let db = open_db();
    let thread_id = Uuid::new_v4();
    let user_message_id = Uuid::new_v4();
    seed_thread(&db, thread_id, user_message_id);
    set_permission_mode(&db, "auto");
    let chat_state = ChatState::new();
    chat_state
        .tool_registry
        .lock()
        .unwrap()
        .register(Arc::new(Photo));
    let (adapter, requests) = StubAdapter::recording(vec![
        vec![Ok(StreamChunk::ToolCalls(vec![LlmToolCall {
            id: "call-1".to_string(),
            name: "echo".to_string(),
            input: serde_json::json!({}),
        }]))],
        vec![Ok(StreamChunk::Done {
            finish_reason: Some("end_turn".to_string()),
            prompt_tokens: None,
            completion_tokens: None,
            ttft_ms: None,
            total_ms: 1,
        })],
    ]);
    let session = session_with(adapter).await;
    let mut request = base_request();
    request.capabilities = Some(ModelCapabilities {
        accepted_attachment_kinds: accepts,
        ..Default::default()
    });
    let (rows, _) = run_scripted_turn_with(
        &db,
        &chat_state,
        &session,
        thread_id,
        user_message_id,
        Uuid::new_v4(),
        request,
    )
    .await;
    let kept = rows
        .iter()
        .find(|row| row.role == MessageRole::ToolResult)
        .expect("a tool result")
        .content
        .clone();
    let sent = requests.lock().unwrap()[1]
        .messages
        .iter()
        .find(|message| matches!(message.role, ChatRole::ToolResult { .. }))
        .expect("the tool result in the second request")
        .clone();
    (kept, sent)
}

fn content(message: &holzi_lib::adapters::types::ChatMessage) -> &str {
    match &message.role {
        ChatRole::ToolResult { content, .. } => content,
        _ => unreachable!(),
    }
}

#[tokio::test]
async fn a_model_that_accepts_images_gets_the_image_and_the_history_a_note() {
    let (kept, sent) = turn(Some(vec![AttachmentKind::Image, AttachmentKind::Text])).await;
    assert_eq!(content(&sent), "a photo");
    assert_eq!(sent.attachments.len(), 1);
    assert_eq!(sent.attachments[0].kind, AttachmentKind::Image);
    assert_eq!(sent.attachments[0].bytes, vec![1, 2, 3]);
    assert_eq!(kept, "a photo\n[Image foto.jpg: shown to the model.]");
}

#[tokio::test]
async fn any_other_model_gets_a_note_in_place_of_the_image() {
    for accepts in [Some(vec![AttachmentKind::Text]), None] {
        let (kept, sent) = turn(accepts.clone()).await;
        assert!(sent.attachments.is_empty(), "{accepts:?}");
        assert!(
            content(&sent).contains("does not accept images"),
            "{accepts:?}"
        );
        assert_eq!(kept, content(&sent), "{accepts:?}");
    }
}
