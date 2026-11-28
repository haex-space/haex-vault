//! Spec 068 PR B (T006): turns of different conversations run at the same time, a model load waits
//! for all of them, and stopping one turn cancels only its own generation, approvals and questions.
//! Uses the scripted adapter and tool of `tests/common/tool_loop_fixture.rs`, with manual approval so
//! a turn parks on a `tool-permission-request` until the test answers it.

#[allow(dead_code)]
#[path = "common/tool_loop_fixture.rs"]
mod tool_loop_fixture;
use tool_loop_fixture::*;

use std::sync::Arc;
use std::time::Duration;

use haex_crdt::Database;
use haex_lib::adapters::types::ToolCall as LlmToolCall;
use haex_lib::adapters::StreamChunk;
use haex_lib::chat::session::ChatState;
use haex_lib::chat::tools::{ApprovalDecision, RiskClass};
use haex_lib::storage::chat_messages::{self as msg_store, FinishReason, MessageRole};
use haex_lib::storage::query;
use serde_json::Value;
use tokio::sync::mpsc::UnboundedReceiver;
use tokio::task::JoinHandle;
use uuid::Uuid;

type Events = UnboundedReceiver<(String, Value)>;

struct Turn {
    thread_id: Uuid,
    assistant_message_id: Uuid,
    handle: Option<JoinHandle<()>>,
    events: Events,
}

impl Turn {
    fn ids(&self) -> (Uuid, Uuid) {
        (self.thread_id, self.assistant_message_id)
    }

    fn is_finished(&self) -> bool {
        self.handle
            .as_ref()
            .is_none_or(|handle| handle.is_finished())
    }

    /// Waits for the turn's task to end.
    async fn join(&mut self) {
        if let Some(handle) = self.handle.take() {
            handle.await.unwrap();
        }
    }
}

/// A turn that calls the `echo` tool once (which manual mode asks about) and then answers.
async fn start_turn(db: &Database, chat_state: &Arc<ChatState>) -> Turn {
    let thread_id = Uuid::new_v4();
    let user_message_id = Uuid::new_v4();
    let assistant_message_id = Uuid::new_v4();
    seed_thread(db, thread_id, user_message_id);
    let adapter = StubAdapter::new(vec![
        vec![Ok(StreamChunk::ToolCalls(vec![LlmToolCall {
            id: "call-1".to_string(),
            name: "echo".to_string(),
            input: serde_json::json!({}),
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
    let request = base_request();
    let stream = session.adapter.stream_chat(request.clone()).await.unwrap();
    let (handle, events) = spawn_turn(
        db.clone(),
        chat_state.clone(),
        session,
        thread_id,
        user_message_id,
        assistant_message_id,
        request,
        stream,
    );
    Turn {
        thread_id,
        assistant_message_id,
        handle: Some(handle),
        events,
    }
}

fn setup() -> (Database, Arc<ChatState>) {
    let db = open_db();
    set_permission_mode(&db, "manual");
    let chat_state = Arc::new(ChatState::new());
    chat_state
        .tool_registry
        .lock()
        .unwrap()
        .register(Arc::new(ScriptedTool {
            name: "echo",
            risk_class: RiskClass::Safe,
            fails: false,
        }));
    (db, chat_state)
}

/// The id of the next `tool-permission-request` of a turn.
async fn next_request(turn: &mut Turn) -> Uuid {
    let (name, payload) = tokio::time::timeout(Duration::from_secs(10), turn.events.recv())
        .await
        .expect("the turn emits its request in time")
        .expect("an event must arrive");
    assert_eq!(name, "tool-permission-request");
    extract_request_id(&payload)
}

/// The finish reason of the terminal assistant row of a turn, by its conversation and message ids.
fn final_reason(db: &Database, ids: (Uuid, Uuid)) -> Option<FinishReason> {
    let (thread_id, assistant_message_id) = ids;
    query::read(db, |r| msg_store::list_messages(r, thread_id))
        .unwrap()
        .into_iter()
        .find(|m| m.id == assistant_message_id)
        .expect("terminal assistant row")
        .finish_reason
}

#[tokio::test]
async fn two_turns_in_different_conversations_wait_for_their_approvals_at_the_same_time() {
    let (db, chat_state) = setup();
    let mut first = start_turn(&db, &chat_state).await;
    let mut second = start_turn(&db, &chat_state).await;

    // Neither request is answered yet, so both turns are running at once.
    let request_a = next_request(&mut first).await;
    let request_b = next_request(&mut second).await;
    assert_ne!(request_a, request_b);
    assert_eq!(chat_state.turns.running().len(), 2);

    respond(&chat_state, request_b, ApprovalDecision::Allow);
    respond(&chat_state, request_a, ApprovalDecision::Allow);
    first.join().await;
    second.join().await;

    assert_eq!(final_reason(&db, first.ids()), Some(FinishReason::Complete));
    assert_eq!(
        final_reason(&db, second.ids()),
        Some(FinishReason::Complete)
    );
    assert!(!chat_state.turns.any_running(), "both slots are free again");
}

#[tokio::test]
async fn a_model_load_is_refused_until_every_turn_has_ended() {
    let (db, chat_state) = setup();
    let mut first = start_turn(&db, &chat_state).await;
    let mut second = start_turn(&db, &chat_state).await;
    let request_a = next_request(&mut first).await;
    let request_b = next_request(&mut second).await;

    assert!(
        chat_state.acquire_operation().is_err(),
        "a load or vault transition cannot start under running turns"
    );

    respond(&chat_state, request_a, ApprovalDecision::Allow);
    first.join().await;
    assert!(
        chat_state.acquire_operation().is_err(),
        "the other turn still runs"
    );

    respond(&chat_state, request_b, ApprovalDecision::Allow);
    second.join().await;
    assert!(chat_state.acquire_operation().is_ok());
}

#[tokio::test]
async fn aborting_one_turn_cancels_only_it_and_the_other_completes() {
    let (db, chat_state) = setup();
    let mut stopped = start_turn(&db, &chat_state).await;
    let mut kept = start_turn(&db, &chat_state).await;
    let request_stopped = next_request(&mut stopped).await;
    let request_kept = next_request(&mut kept).await;

    assert!(chat_state.abort_turn_by_id(stopped.assistant_message_id));
    stopped.join().await;

    // The stopped turn ended as cancelled and left no tool rows behind.
    assert_eq!(
        final_reason(&db, stopped.ids()),
        Some(FinishReason::Cancelled)
    );
    let rows = query::read(&db, |r| msg_store::list_messages(r, stopped.thread_id)).unwrap();
    assert!(rows
        .iter()
        .all(|m| m.role != MessageRole::ToolCall && m.role != MessageRole::ToolResult));
    // Its approval is gone; a late answer to it is harmless.
    assert!(!chat_state
        .pending_tool_approvals
        .lock()
        .unwrap()
        .contains_key(&request_stopped));

    // The other turn's approval is still open and answering it lets that turn finish.
    assert!(
        chat_state
            .pending_tool_approvals
            .lock()
            .unwrap()
            .contains_key(&request_kept),
        "the other turn's approval is untouched"
    );
    assert!(!kept.is_finished(), "the other turn is still waiting");
    respond(&chat_state, request_kept, ApprovalDecision::Allow);
    kept.join().await;
    assert_eq!(final_reason(&db, kept.ids()), Some(FinishReason::Complete));
    let kept_rows = query::read(&db, |r| msg_store::list_messages(r, kept.thread_id)).unwrap();
    assert!(
        kept_rows.iter().any(|m| m.role == MessageRole::ToolResult),
        "the kept turn ran its tool"
    );
}

#[tokio::test]
async fn stopping_by_conversation_finds_that_conversations_turn() {
    let (db, chat_state) = setup();
    let mut first = start_turn(&db, &chat_state).await;
    let mut second = start_turn(&db, &chat_state).await;
    let _ = next_request(&mut first).await;
    let request_b = next_request(&mut second).await;

    assert!(chat_state.abort_turn_of_thread(first.thread_id));
    first.join().await;
    assert_eq!(
        final_reason(&db, first.ids()),
        Some(FinishReason::Cancelled)
    );
    assert!(!second.is_finished());

    respond(&chat_state, request_b, ApprovalDecision::Allow);
    second.join().await;
    assert_eq!(
        final_reason(&db, second.ids()),
        Some(FinishReason::Complete)
    );
}

#[tokio::test]
async fn a_second_turn_in_the_same_conversation_is_refused_while_the_first_runs() {
    let (db, chat_state) = setup();
    let mut running = start_turn(&db, &chat_state).await;
    let request = next_request(&mut running).await;

    assert!(
        chat_state
            .begin_turn(
                Uuid::new_v4(),
                Some(running.thread_id),
                tokio_util::sync::CancellationToken::new(),
            )
            .is_err(),
        "one turn per conversation"
    );
    assert!(!running.is_finished(), "the running turn is untouched");

    respond(&chat_state, request, ApprovalDecision::Allow);
    running.join().await;
}

#[tokio::test]
async fn stopping_every_turn_ends_them_all() {
    let (db, chat_state) = setup();
    let mut first = start_turn(&db, &chat_state).await;
    let mut second = start_turn(&db, &chat_state).await;
    let _ = next_request(&mut first).await;
    let _ = next_request(&mut second).await;

    haex_lib::chat::commands::abort_turn(&chat_state).unwrap();
    first.join().await;
    second.join().await;

    assert_eq!(
        final_reason(&db, first.ids()),
        Some(FinishReason::Cancelled)
    );
    assert_eq!(
        final_reason(&db, second.ids()),
        Some(FinishReason::Cancelled)
    );
    assert!(chat_state.acquire_operation().is_ok());
}
