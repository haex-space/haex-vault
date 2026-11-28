//! Spec 068 T012: a turn runs with the model of its own conversation. Cloud models, a delegate and
//! the local engine in memory load or run beside turns of other conversations; only an engine load
//! is alone. Uses the scripted adapter and tool of `tests/common/tool_loop_fixture.rs` with manual
//! approval, so a turn parks on a `tool-permission-request` until the test answers it.

#[allow(dead_code)]
#[path = "common/tool_loop_fixture.rs"]
mod tool_loop_fixture;
use tool_loop_fixture::*;

use std::sync::Arc;
use std::time::Duration;

use haex_crdt::Database;
use haex_lib::adapters::types::ToolCall as LlmToolCall;
use haex_lib::adapters::StreamChunk;
use haex_lib::chat::session::{ActiveSession, ChatState};
use haex_lib::chat::tools::{ApprovalDecision, RiskClass};
use haex_lib::storage::chat_messages::{self as msg_store, ChatMessage, FinishReason, MessageRole};
use haex_lib::storage::providers::ProviderKind;
use haex_lib::storage::query;
use serde_json::Value;
use tokio::sync::mpsc::UnboundedReceiver;
use tokio::task::JoinHandle;
use uuid::Uuid;

const CLOUD_A: &str = "11111111-1111-1111-1111-111111111111:model-a";
const CLOUD_B: &str = "22222222-2222-2222-2222-222222222222:model-b";
const CLOUD_C: &str = "33333333-3333-3333-3333-333333333333:model-c";
const DELEGATE: &str = "44444444-4444-4444-4444-444444444444:delegate";

type Events = UnboundedReceiver<(String, Value)>;

struct Turn {
    thread_id: Uuid,
    assistant_message_id: Uuid,
    handle: Option<JoinHandle<()>>,
    events: Events,
}

impl Turn {
    async fn join(&mut self) {
        if let Some(handle) = self.handle.take() {
            handle.await.unwrap();
        }
    }
}

/// A session of `kind` on `model_id` with `adapter`; a cloud session carries no provider row here.
fn session_of(model_id: &str, kind: ProviderKind, adapter: StubAdapter) -> ActiveSession {
    ActiveSession {
        model_id: model_id.to_string(),
        provider_id: None,
        provider_kind: kind,
        adapter: Arc::new(adapter),
        tokenizer_repo: String::new(),
        context_window: None,
        engine: matches!(kind, ProviderKind::Local).then_some("llama.cpp"),
    }
}

fn done() -> Vec<Result<StreamChunk, haex_lib::adapters::StreamError>> {
    vec![Ok(StreamChunk::Done {
        finish_reason: Some("end_turn".to_string()),
        prompt_tokens: Some(1),
        completion_tokens: Some(1),
        ttft_ms: Some(1),
        total_ms: 1,
    })]
}

/// An adapter that calls the `echo` tool once (which manual mode asks about) and then answers.
fn echo_then_answer() -> StubAdapter {
    StubAdapter::new(vec![
        vec![Ok(StreamChunk::ToolCalls(vec![LlmToolCall {
            id: "call-1".to_string(),
            name: "echo".to_string(),
            input: serde_json::json!({}),
        }]))],
        done(),
    ])
}

/// Starts a turn of a new conversation with `session`.
async fn start_turn(db: &Database, chat_state: &Arc<ChatState>, session: ActiveSession) -> Turn {
    let thread_id = Uuid::new_v4();
    let user_message_id = Uuid::new_v4();
    let assistant_message_id = Uuid::new_v4();
    seed_thread(db, thread_id, user_message_id);
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

fn rows(db: &Database, turn: &Turn) -> Vec<ChatMessage> {
    query::read(db, |r| msg_store::list_messages(r, turn.thread_id)).unwrap()
}

/// The terminal assistant row of a turn.
fn final_row(db: &Database, turn: &Turn) -> ChatMessage {
    rows(db, turn)
        .into_iter()
        .find(|m| m.id == turn.assistant_message_id)
        .expect("terminal assistant row")
}

#[tokio::test]
async fn a_cloud_pick_loads_beside_a_running_turn() {
    let (db, chat_state) = setup();
    let mut running = start_turn(
        &db,
        &chat_state,
        session_of(CLOUD_A, ProviderKind::ApiKey, echo_then_answer()),
    )
    .await;
    let request = next_request(&mut running).await;

    let load = chat_state
        .acquire_for_load(CLOUD_B)
        .expect("a provider's model loads while another conversation answers");
    assert!(!load.is_exclusive(), "beside the turn, not alone");
    // The turn goes on: another turn can start, and the first one finishes with its own model.
    let mut other = start_turn(
        &db,
        &chat_state,
        session_of(CLOUD_B, ProviderKind::ApiKey, echo_then_answer()),
    )
    .await;
    let other_request = next_request(&mut other).await;
    drop(load);

    respond(&chat_state, request, ApprovalDecision::Allow);
    respond(&chat_state, other_request, ApprovalDecision::Allow);
    running.join().await;
    other.join().await;
    assert_eq!(
        final_row(&db, &running).finish_reason,
        Some(FinishReason::Complete)
    );
    assert_eq!(
        final_row(&db, &other).finish_reason,
        Some(FinishReason::Complete)
    );
}

#[tokio::test]
async fn a_delegate_pick_loads_beside_a_running_turn() {
    let (db, chat_state) = setup();
    let mut running = start_turn(
        &db,
        &chat_state,
        session_of(CLOUD_A, ProviderKind::ApiKey, echo_then_answer()),
    )
    .await;
    let request = next_request(&mut running).await;

    let load = chat_state
        .acquire_for_load(DELEGATE)
        .expect("a delegate's model loads while another conversation answers");
    assert!(!load.is_exclusive());
    drop(load);

    respond(&chat_state, request, ApprovalDecision::Allow);
    running.join().await;
}

#[tokio::test]
async fn an_engine_load_is_alone_and_refused_while_a_turn_runs() {
    let (db, chat_state) = setup();
    let mut running = start_turn(
        &db,
        &chat_state,
        session_of(CLOUD_A, ProviderKind::ApiKey, echo_then_answer()),
    )
    .await;
    let request = next_request(&mut running).await;

    // A local model that is not in memory has to be loaded: the engine is one for every turn.
    assert!(
        chat_state.acquire_for_load("qwen3-4b").is_err(),
        "an engine load cannot start under a running turn"
    );

    respond(&chat_state, request, ApprovalDecision::Allow);
    running.join().await;
    let load = chat_state
        .acquire_for_load("qwen3-4b")
        .expect("the engine loads when no turn runs");
    assert!(load.is_exclusive());
    // While it loads, no turn and no other load starts.
    assert!(chat_state.acquire_for_load(CLOUD_B).is_err());
    assert!(chat_state
        .begin_turn(
            Uuid::new_v4(),
            Some(Uuid::new_v4()),
            tokio_util::sync::CancellationToken::new(),
        )
        .is_err());
}

#[tokio::test]
async fn a_local_model_in_memory_is_taken_up_beside_a_running_turn() {
    let (db, chat_state) = setup();
    chat_state.replace_local_model(Some(session_of(
        "qwen3-4b",
        ProviderKind::Local,
        StubAdapter::new(vec![]),
    )));
    let mut running = start_turn(
        &db,
        &chat_state,
        session_of(CLOUD_A, ProviderKind::ApiKey, echo_then_answer()),
    )
    .await;
    let request = next_request(&mut running).await;

    // Going back to a conversation with the local model only swaps what the chat answers with.
    let load = chat_state
        .acquire_for_load("qwen3-4b")
        .expect("the engine is in memory");
    assert!(!load.is_exclusive());
    drop(load);

    respond(&chat_state, request, ApprovalDecision::Allow);
    running.join().await;
}

#[tokio::test]
async fn an_exclusive_operation_refuses_a_cloud_load() {
    let chat_state = ChatState::new();
    let _alone = chat_state.acquire_operation().unwrap();
    assert!(chat_state.acquire_for_load(CLOUD_A).is_err());
}

#[tokio::test]
async fn two_turns_on_two_cloud_models_run_at_once_and_keep_their_models() {
    let (db, chat_state) = setup();
    let mut first = start_turn(
        &db,
        &chat_state,
        session_of(CLOUD_A, ProviderKind::ApiKey, echo_then_answer()),
    )
    .await;
    let mut second = start_turn(
        &db,
        &chat_state,
        session_of(CLOUD_B, ProviderKind::ApiKey, echo_then_answer()),
    )
    .await;
    let request_a = next_request(&mut first).await;
    let request_b = next_request(&mut second).await;
    assert_eq!(
        chat_state.turns.running().len(),
        2,
        "both turns run at once"
    );

    // Mid-run the person picks a third cloud model in a conversation of its own: the open model of
    // the chat changes, the two turns keep theirs, their tools and their approvals.
    let load = chat_state.acquire_for_load(CLOUD_C).unwrap();
    *chat_state.session.lock().unwrap() = Some(session_of(
        CLOUD_C,
        ProviderKind::ApiKey,
        StubAdapter::new(vec![]),
    ));
    drop(load);
    assert!(
        chat_state
            .pending_tool_approvals
            .lock()
            .unwrap()
            .contains_key(&request_a),
        "the pick leaves the open approvals alone"
    );

    respond(&chat_state, request_b, ApprovalDecision::Allow);
    respond(&chat_state, request_a, ApprovalDecision::Allow);
    first.join().await;
    second.join().await;

    for (turn, model) in [(&first, CLOUD_A), (&second, CLOUD_B)] {
        let row = final_row(&db, turn);
        assert_eq!(row.finish_reason, Some(FinishReason::Complete));
        assert_eq!(row.model_id.as_deref(), Some(model), "the turn's own model");
        assert!(
            rows(&db, turn)
                .iter()
                .all(|m| m.model_id.is_none() || m.model_id.as_deref() == Some(model)),
            "no row of the turn names another model"
        );
        assert!(
            rows(&db, turn)
                .iter()
                .any(|m| m.role == MessageRole::ToolResult),
            "the turn ran its tool"
        );
    }
}

#[tokio::test]
async fn a_delegate_turn_and_a_cloud_turn_run_at_the_same_time() {
    let (db, chat_state) = setup();
    let mut cloud = start_turn(
        &db,
        &chat_state,
        session_of(CLOUD_A, ProviderKind::ApiKey, echo_then_answer()),
    )
    .await;
    let request = next_request(&mut cloud).await;
    // The delegate answers with text while the cloud turn waits for its approval.
    let mut delegate = start_turn(
        &db,
        &chat_state,
        session_of(
            DELEGATE,
            ProviderKind::CliDelegate,
            StubAdapter::new(vec![done()]),
        ),
    )
    .await;
    delegate.join().await;
    assert_eq!(
        final_row(&db, &delegate).finish_reason,
        Some(FinishReason::Complete)
    );
    assert_eq!(
        final_row(&db, &delegate).model_id.as_deref(),
        Some(DELEGATE)
    );

    respond(&chat_state, request, ApprovalDecision::Allow);
    cloud.join().await;
    assert_eq!(
        final_row(&db, &cloud).finish_reason,
        Some(FinishReason::Complete)
    );
    assert_eq!(final_row(&db, &cloud).model_id.as_deref(), Some(CLOUD_A));
}
