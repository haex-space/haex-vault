//! Retry-behavior coverage for `run_turn` (`chat/commands.rs`) — a
//! transient stream failure, the shared per-turn retry budget, budget
//! exhaustion and non-retried terminal errors (spec.md User Story 4). Split
//! out of `chat_tool_loop.rs`; see `tests/common/tool_loop_fixture.rs` for
//! the shared `StubAdapter`/`ScriptedTool`/`run_scripted_turn` fixture.

#[allow(dead_code)]
#[path = "common/tool_loop_fixture.rs"]
mod tool_loop_fixture;
use tool_loop_fixture::*;

use std::sync::Arc;

use haex_lib::adapters::types::ToolCall as LlmToolCall;
use haex_lib::adapters::{StreamChunk, StreamError};
use haex_lib::chat::session::ChatState;
use haex_lib::chat::tools::RiskClass;
use haex_lib::chat::turn::MAX_RETRY_ATTEMPTS;
use haex_lib::storage::chat_messages::FinishReason;
use uuid::Uuid;

/// T035 (US4): a transient failure retries automatically and only the
/// final, successful answer is ever persisted — no trace of the failed
/// attempt (spec.md Acceptance Scenario 1). `tokio::time::pause` makes the
/// retry backoff instant instead of a real 500ms wait.
#[tokio::test]
async fn a_transient_failure_retries_and_only_the_final_answer_persists() {
    tokio::time::pause();
    let db = open_db();
    let thread_id = Uuid::new_v4();
    let user_message_id = Uuid::new_v4();
    let assistant_message_id = Uuid::new_v4();
    seed_thread(&db, thread_id, user_message_id);

    let chat_state = ChatState::new();
    let adapter = StubAdapter::new(vec![
        vec![Err(StreamError::Transient("hiccup".to_string()))],
        vec![
            Ok(StreamChunk::Delta {
                content: "hi there".to_string(),
                reasoning: None,
            }),
            Ok(StreamChunk::Done {
                finish_reason: Some("end_turn".to_string()),
                prompt_tokens: Some(1),
                completion_tokens: Some(1),
                ttft_ms: Some(1),
                total_ms: 1,
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

    assert_eq!(rows.len(), 2, "{rows:#?}");
    let final_row = rows
        .iter()
        .find(|m| m.id == assistant_message_id)
        .expect("terminal assistant row");
    assert_eq!(final_row.finish_reason, Some(FinishReason::Complete));
    assert_eq!(final_row.content, "hi there");

    let retry_events: Vec<_> = events.iter().filter(|(n, _)| n == "chat-retry").collect();
    assert_eq!(retry_events.len(), 1, "{events:#?}");
    assert_eq!(retry_events[0].1["attempt"], 1);
}

/// A retry consumed while recovering from one tool round must reduce the
/// budget available to later rounds in the same turn.
#[tokio::test]
async fn retries_are_shared_across_tool_rounds() {
    tokio::time::pause();
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

    let adapter = StubAdapter::new(vec![
        vec![Err(StreamError::Transient("first hiccup".to_string()))],
        vec![Ok(StreamChunk::ToolCalls(vec![LlmToolCall {
            id: "call-1".to_string(),
            name: "echo".to_string(),
            input: serde_json::json!({}),
        }]))],
        vec![Err(StreamError::Transient("second hiccup".to_string()))],
        vec![Err(StreamError::Transient("third hiccup".to_string()))],
        vec![Err(StreamError::Transient("fourth hiccup".to_string()))],
        vec![Ok(StreamChunk::Done {
            finish_reason: Some("end_turn".to_string()),
            prompt_tokens: Some(1),
            completion_tokens: Some(1),
            ttft_ms: Some(1),
            total_ms: 1,
        })],
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

    let final_row = rows
        .iter()
        .find(|message| message.id == assistant_message_id)
        .expect("terminal assistant row");
    assert_eq!(final_row.finish_reason, Some(FinishReason::Error));
    let retry_events: Vec<_> = events
        .iter()
        .filter(|(name, _)| name == "chat-retry")
        .collect();
    assert_eq!(retry_events.len(), MAX_RETRY_ATTEMPTS, "{events:#?}");
}

/// T036 (US4): a transient failure that keeps recurring past the retry
/// budget ends the turn with `FinishReason::Error`, distinguishable from
/// `Cancelled` and `ToolLimitReached` (spec.md Acceptance Scenario 2).
#[tokio::test]
async fn a_transient_failure_past_the_retry_limit_ends_with_error() {
    tokio::time::pause();
    let db = open_db();
    let thread_id = Uuid::new_v4();
    let user_message_id = Uuid::new_v4();
    let assistant_message_id = Uuid::new_v4();
    seed_thread(&db, thread_id, user_message_id);

    let chat_state = ChatState::new();
    // The original attempt plus every retry all fail transiently,
    // exhausting the bounded budget.
    let steps = (0..=MAX_RETRY_ATTEMPTS)
        .map(|_| vec![Err(StreamError::Transient("still down".to_string()))])
        .collect();
    let adapter = StubAdapter::new(steps);
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
        .expect("terminal assistant row");
    assert_eq!(final_row.finish_reason, Some(FinishReason::Error));
    assert_ne!(final_row.finish_reason, Some(FinishReason::Cancelled));
    assert_ne!(
        final_row.finish_reason,
        Some(FinishReason::ToolLimitReached)
    );

    let retry_events: Vec<_> = events.iter().filter(|(n, _)| n == "chat-retry").collect();
    assert_eq!(retry_events.len(), MAX_RETRY_ATTEMPTS, "{events:#?}");
}

/// A terminal (non-transient) `StreamError` must never be retried, even
/// with retry budget remaining — it would just reproduce deterministically.
#[tokio::test]
async fn a_terminal_stream_error_is_not_retried() {
    let db = open_db();
    let thread_id = Uuid::new_v4();
    let user_message_id = Uuid::new_v4();
    let assistant_message_id = Uuid::new_v4();
    seed_thread(&db, thread_id, user_message_id);

    let chat_state = ChatState::new();
    let adapter = StubAdapter::new(vec![vec![Err(StreamError::Model(
        "invalid request".to_string(),
    ))]]);
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
        .expect("terminal assistant row");
    assert_eq!(final_row.finish_reason, Some(FinishReason::Error));
    assert!(!events.iter().any(|(n, _)| n == "chat-retry"));
}
