//! Tool-loop core coverage for `run_turn` (`chat/commands.rs`) — the
//! ordered persistence chain, tool-level errors and the round limit
//! (spec.md Acceptance Scenarios 1-3, User Story 1). Split out of
//! `chat_tool_loop.rs`; see `tests/common/tool_loop_fixture.rs` for the
//! shared `StubAdapter`/`ScriptedTool`/`run_scripted_turn` fixture and
//! `chat_tool_loop_permissions.rs` / `chat_tool_loop_retry.rs` for the
//! permission-gating and retry cases.

#[allow(dead_code)]
#[path = "common/tool_loop_fixture.rs"]
mod tool_loop_fixture;
use tool_loop_fixture::*;

use std::sync::Arc;

use holzi_lib::adapters::types::{ChatRole, ToolCall as LlmToolCall};
use holzi_lib::adapters::StreamChunk;
use holzi_lib::chat::session::ChatState;
use holzi_lib::chat::tools::RiskClass;
use holzi_lib::chat::turn::MAX_TOOL_ROUNDS;
use holzi_lib::storage::chat_messages::{FinishReason, MessageRole};
use holzi_lib::storage::preferences::{self, PrefScope};
use uuid::Uuid;

#[tokio::test]
async fn tool_call_then_final_answer_persists_the_full_ordered_chain() {
    let db = open_db();
    let thread_id = Uuid::new_v4();
    let user_message_id = Uuid::new_v4();
    let assistant_message_id = Uuid::new_v4();
    seed_thread(&db, thread_id, user_message_id);
    // US1 fixtures predate the permission gate (Phase 4) and are not
    // testing it — `auto` lets the `Safe` scripted tool execute
    // immediately, same as before the gate existed.
    set_permission_mode(&db, "auto");

    let chat_state = ChatState::new();
    chat_state
        .tool_registry
        .lock()
        .unwrap()
        .register(Arc::new(ScriptedTool {
            name: "echo",
            risk_class: RiskClass::Safe,
            fails: false,
        }));

    let adapter = StubAdapter::new(vec![
        vec![Ok(StreamChunk::ToolCalls(vec![LlmToolCall {
            id: "call-1".to_string(),
            name: "echo".to_string(),
            input: serde_json::json!({ "x": 1 }),
        }]))],
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
        ],
    ]);
    let session = session_with(adapter).await;

    let (rows, events) = run_scripted_turn(
        &db,
        &chat_state,
        &session,
        thread_id,
        user_message_id,
        assistant_message_id,
    )
    .await;

    // user -> tool_call -> tool_result -> assistant(final), in that order.
    assert_eq!(rows.len(), 4, "{rows:#?}");
    assert_eq!(rows[0].id, user_message_id);
    assert_eq!(rows[1].role, MessageRole::ToolCall);
    assert_eq!(rows[1].parent_id, Some(user_message_id));
    assert_eq!(rows[1].tool_name.as_deref(), Some("echo"));
    assert_eq!(rows[1].tool_call_id.as_deref(), Some("call-1"));
    assert_eq!(rows[2].role, MessageRole::ToolResult);
    assert_eq!(rows[2].parent_id, Some(rows[1].id));
    assert_eq!(rows[2].tool_call_id.as_deref(), Some("call-1"));
    assert_eq!(rows[2].tool_is_error, Some(false));
    assert_eq!(rows[3].role, MessageRole::Assistant);
    assert_eq!(rows[3].id, assistant_message_id);
    assert_eq!(rows[3].parent_id, Some(rows[2].id));
    assert_eq!(rows[3].content, "final answer");
    assert_eq!(rows[3].finish_reason, Some(FinishReason::Complete));

    let event_names: Vec<&str> = events.iter().map(|(n, _)| n.as_str()).collect();
    assert!(event_names.contains(&"chat-tool-call"));
    assert!(event_names.contains(&"chat-tool-result"));
    assert!(event_names.contains(&"chat-message-complete"));
    let (last_name, last_payload) = events.last().expect("at least one event");
    assert_eq!(last_name, "chat-turn-complete");
    assert_eq!(last_payload["finishReason"], "complete");
    assert_eq!(
        last_payload["assistantMessageId"],
        assistant_message_id.to_string()
    );
}

#[tokio::test]
async fn a_tool_error_does_not_end_the_turn() {
    let db = open_db();
    let thread_id = Uuid::new_v4();
    let user_message_id = Uuid::new_v4();
    let assistant_message_id = Uuid::new_v4();
    seed_thread(&db, thread_id, user_message_id);
    // US1 fixtures predate the permission gate (Phase 4) and are not
    // testing it — `auto` lets the `Safe` scripted tool execute
    // immediately, same as before the gate existed.
    set_permission_mode(&db, "auto");

    let chat_state = ChatState::new();
    chat_state
        .tool_registry
        .lock()
        .unwrap()
        .register(Arc::new(ScriptedTool {
            name: "echo",
            risk_class: RiskClass::Safe,
            fails: true,
        }));

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

    let (rows, _events) = run_scripted_turn(
        &db,
        &chat_state,
        &session,
        thread_id,
        user_message_id,
        assistant_message_id,
    )
    .await;

    assert_eq!(rows.len(), 4, "{rows:#?}");
    assert_eq!(rows[2].role, MessageRole::ToolResult);
    assert_eq!(rows[2].tool_is_error, Some(true));
    assert_eq!(rows[2].content, "scripted failure");
    // The turn must still reach a normal completion, not FinishReason::Error.
    assert_eq!(rows[3].role, MessageRole::Assistant);
    assert_eq!(rows[3].finish_reason, Some(FinishReason::Complete));
}

#[tokio::test]
async fn exceeding_the_round_limit_stops_with_tool_limit_reached() {
    let db = open_db();
    let thread_id = Uuid::new_v4();
    let user_message_id = Uuid::new_v4();
    let assistant_message_id = Uuid::new_v4();
    seed_thread(&db, thread_id, user_message_id);
    // US1 fixtures predate the permission gate (Phase 4) and are not
    // testing it — `auto` lets the `Safe` scripted tool execute
    // immediately, same as before the gate existed.
    set_permission_mode(&db, "auto");

    let chat_state = ChatState::new();
    chat_state
        .tool_registry
        .lock()
        .unwrap()
        .register(Arc::new(ScriptedTool {
            name: "echo",
            risk_class: RiskClass::Safe,
            fails: false,
        }));

    let adapter = StubAdapter::always_calls_tool("echo");
    let session = session_with(adapter).await;

    let (rows, events) = run_scripted_turn(
        &db,
        &chat_state,
        &session,
        thread_id,
        user_message_id,
        assistant_message_id,
    )
    .await;

    let final_row = rows
        .iter()
        .find(|m| m.id == assistant_message_id)
        .expect("terminal assistant row must be persisted");
    assert_eq!(
        final_row.finish_reason,
        Some(FinishReason::ToolLimitReached)
    );

    // Exactly MAX_TOOL_ROUNDS tool_call rows — no further step ran once
    // the cap was reached.
    let tool_call_rows = rows
        .iter()
        .filter(|m| m.role == MessageRole::ToolCall)
        .count();
    assert_eq!(tool_call_rows, MAX_TOOL_ROUNDS);

    let (last_name, last_payload) = events.last().expect("at least one event");
    assert_eq!(last_name, "chat-turn-complete");
    assert_eq!(last_payload["finishReason"], "tool_limit_reached");
}

/// At the round cap the model gets one last step and is told not to call a tool but to say what it
/// found: the answer is its text, marked `ToolLimitReached`, and the instruction is not a row of the
/// thread.
#[tokio::test]
async fn at_the_round_limit_the_model_gives_a_last_answer_and_calls_no_tool() {
    let db = open_db();
    let thread_id = Uuid::new_v4();
    let user_message_id = Uuid::new_v4();
    let assistant_message_id = Uuid::new_v4();
    seed_thread(&db, thread_id, user_message_id);
    set_permission_mode(&db, "auto");

    let chat_state = ChatState::new();
    chat_state
        .tool_registry
        .lock()
        .unwrap()
        .register(Arc::new(ScriptedTool {
            name: "echo",
            risk_class: RiskClass::Safe,
            fails: false,
        }));

    let mut steps = Vec::new();
    for _ in 0..MAX_TOOL_ROUNDS {
        steps.push(vec![Ok(StreamChunk::ToolCalls(vec![LlmToolCall {
            id: Uuid::new_v4().to_string(),
            name: "echo".to_string(),
            input: serde_json::json!({}),
        }]))]);
    }
    steps.push(vec![
        Ok(StreamChunk::Delta {
            content: "Found two pages; the form is still open.".to_string(),
            reasoning: None,
        }),
        Ok(StreamChunk::Done {
            finish_reason: Some("end_turn".to_string()),
            prompt_tokens: Some(10),
            completion_tokens: Some(5),
            ttft_ms: Some(3),
            total_ms: 20,
        }),
    ]);
    let (adapter, requests) = StubAdapter::recording(steps);
    let session = session_with(adapter).await;

    let (rows, events) = run_scripted_turn(
        &db,
        &chat_state,
        &session,
        thread_id,
        user_message_id,
        assistant_message_id,
    )
    .await;

    let final_row = rows
        .iter()
        .find(|m| m.id == assistant_message_id)
        .expect("terminal assistant row must be persisted");
    assert_eq!(
        final_row.finish_reason,
        Some(FinishReason::ToolLimitReached)
    );
    assert_eq!(
        final_row.content,
        "Found two pages; the form is still open."
    );
    assert_eq!(
        rows.iter().filter(|m| m.role == MessageRole::User).count(),
        1,
        "the wrap-up instruction is only in the request"
    );

    let requests = requests.lock().unwrap();
    assert_eq!(requests.len(), MAX_TOOL_ROUNDS + 1);
    let last = requests.last().unwrap();
    // The tool list stays (the history holds tool blocks, which Anthropic only takes with tools);
    // the model is told not to use them.
    assert!(
        !last.tools.is_empty(),
        "the history's tool blocks need the tool list"
    );
    assert_eq!(last.messages.last().unwrap().role, ChatRole::User);
    assert!(requests[MAX_TOOL_ROUNDS - 1]
        .tools
        .iter()
        .any(|t| t.name == "echo"));

    let (name, payload) = events.last().expect("at least one event");
    assert_eq!(name, "chat-turn-complete");
    assert_eq!(payload["finishReason"], "tool_limit_reached");
}

/// The vault setting `chat.maxToolRounds` replaces the default cap, and a change applies to the
/// next round.
#[tokio::test]
async fn the_stored_round_limit_replaces_the_default() {
    let db = open_db();
    let thread_id = Uuid::new_v4();
    let user_message_id = Uuid::new_v4();
    let assistant_message_id = Uuid::new_v4();
    seed_thread(&db, thread_id, user_message_id);
    set_permission_mode(&db, "auto");
    db.write(|tx| preferences::insert_or_update(tx, PrefScope::Vault, "chat.maxToolRounds", "3"))
        .unwrap();

    let chat_state = ChatState::new();
    chat_state
        .tool_registry
        .lock()
        .unwrap()
        .register(Arc::new(ScriptedTool {
            name: "echo",
            risk_class: RiskClass::Safe,
            fails: false,
        }));
    let session = session_with(StubAdapter::always_calls_tool("echo")).await;

    let (rows, _) = run_scripted_turn(
        &db,
        &chat_state,
        &session,
        thread_id,
        user_message_id,
        assistant_message_id,
    )
    .await;

    let tool_call_rows = rows
        .iter()
        .filter(|m| m.role == MessageRole::ToolCall)
        .count();
    assert_eq!(tool_call_rows, 3);
    let final_row = rows
        .iter()
        .find(|m| m.id == assistant_message_id)
        .expect("terminal assistant row must be persisted");
    assert_eq!(
        final_row.finish_reason,
        Some(FinishReason::ToolLimitReached)
    );
}
