//! Permission-gating coverage for `run_turn` (`chat/commands.rs`) — Manual
//! and Plan mode prompts, denial, an unanswered request, aborting mid-tool
//! and mid-prompt, two independent pending requests, a mode change while a
//! request is pending, and the leaked-tool-call-tag regression (spec.md
//! User Story 2/3). Also the CI-stable and real-local-model CLI-tool
//! acceptance tests, which exercise this same manual-mode approval path
//! end-to-end. Split out of `chat_tool_loop.rs`; see
//! `tests/common/tool_loop_fixture.rs` for the shared
//! `StubAdapter`/`ScriptedTool`/`spawn_turn` fixture.
//!
//! Maintainability exception (spaex 500-LoC rule): this is the last of the
//! three binaries `chat_tool_loop.rs` split into (2026-09-15 review) and
//! inherited most of the original file's cases — every one of them shares
//! the `spawn_turn`/`respond`/`extract_request_id` manual-approval
//! mechanics, unlike the core and retry binaries, which use
//! `run_scripted_turn` and stayed under 500 lines on the same split.
//!
//! Concrete split plan, if this grows further: three binaries along the
//! existing test-name grouping — `chat_tool_loop_permission_requests.rs`
//! (`manual_mode_emits_a_permission_request_for_a_safe_tool`,
//! `a_leaked_tool_call_tag_is_stripped_from_the_interim_assistant_text`,
//! `manual_mode_emits_a_permission_request_for_the_risky_cli_tool`,
//! `denying_a_request_produces_an_error_result_and_the_turn_continues`,
//! `an_unanswered_request_leaves_the_turn_waiting_indefinitely`,
//! `plan_mode_blocks_a_risky_tool_without_any_prompt`, ~430 lines),
//! `chat_tool_loop_permission_lifecycle.rs` (the `events_after` helper,
//! `aborting_during_tool_execution_kills_the_process_and_ends_the_turn`,
//! `aborting_a_pending_permission_request_cancels_the_turn_not_denies_it`,
//! `two_independent_risky_calls_each_get_their_own_pending_request`,
//! `a_mode_change_while_pending_only_affects_the_next_tool_use`, ~350
//! lines) and `chat_tool_loop_cli_acceptance.rs`
//! (`ci_e2e_loaded_model_can_request_and_process_a_cli_command`,
//! `a_real_local_model_can_request_and_process_a_cli_command` plus the
//! `#[cfg(feature = "llm-cpu")]` imports, ~300 lines).

#[allow(dead_code)]
#[path = "common/tool_loop_fixture.rs"]
mod tool_loop_fixture;
use tool_loop_fixture::*;

use std::sync::Arc;

use holzi_lib::adapters::types::{ChatRequest, ToolCall as LlmToolCall, ToolSpec};
use holzi_lib::adapters::StreamChunk;
use holzi_lib::chat::commands::abort_turn;
use holzi_lib::chat::session::ChatState;
use holzi_lib::chat::tools::{ApprovalDecision, RiskClass};
use holzi_lib::storage::chat_messages::{self as msg_store, FinishReason, MessageRole};
use holzi_lib::storage::query;
use serde_json::Value;
use uuid::Uuid;

#[cfg(feature = "llm-cpu")]
use holzi_lib::adapters::local::LocalAdapter;
#[cfg(feature = "llm-cpu")]
use holzi_lib::adapters::types::{ChatMessage as LlmMessage, ChatRole};
#[cfg(feature = "llm-cpu")]
use holzi_lib::chat::session::ActiveSession;
#[cfg(feature = "llm-cpu")]
use holzi_lib::llm::local::LocalModel;
#[cfg(feature = "llm-cpu")]
use holzi_lib::storage::providers::ProviderKind;
#[cfg(feature = "llm-cpu")]
use std::env;
#[cfg(feature = "llm-cpu")]
use std::path::PathBuf;

#[tokio::test]
async fn manual_mode_emits_a_permission_request_for_a_safe_tool() {
    let db = open_db();
    let thread_id = Uuid::new_v4();
    let user_message_id = Uuid::new_v4();
    let assistant_message_id = Uuid::new_v4();
    seed_thread(&db, thread_id, user_message_id);
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

    let (handle, mut rx) = spawn_turn(
        db.clone(),
        chat_state.clone(),
        session,
        thread_id,
        user_message_id,
        assistant_message_id,
        request,
        stream,
    );

    let (name, payload) = rx.recv().await.expect("an event must arrive");
    assert_eq!(name, "tool-permission-request");
    assert_eq!(payload["riskClass"], "safe");
    assert_eq!(payload["toolName"], "echo");

    respond(
        &chat_state,
        extract_request_id(&payload),
        ApprovalDecision::Allow,
    );
    handle.await.unwrap();

    let rows = query::read(&db, |r| msg_store::list_messages(r, thread_id)).unwrap();
    let tool_result = rows
        .iter()
        .find(|m| m.role == MessageRole::ToolResult)
        .expect("tool_result row");
    assert_eq!(tool_result.tool_is_error, Some(false));
}

/// Regression for a leaked mistralrs reasoning-mode `<tool_call>` tag
/// (Qwen text convention): a reasoning-capable local model's `content` can
/// carry the raw tag verbatim alongside a correctly-parsed `tool_calls`
/// entry (`mistralrs-core` 0.8.1 does not run its reasoning-mode content
/// path through its own tool-call-tag stripping). The interim assistant row
/// persisted before the tool call must never show that raw markup.
#[tokio::test]
async fn a_leaked_tool_call_tag_is_stripped_from_the_interim_assistant_text() {
    let db = open_db();
    let thread_id = Uuid::new_v4();
    let user_message_id = Uuid::new_v4();
    let assistant_message_id = Uuid::new_v4();
    seed_thread(&db, thread_id, user_message_id);
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

    let adapter = StubAdapter::new(vec![
        vec![
            Ok(StreamChunk::Delta {
                content:
                    "<tool_call>\n{\"name\": \"echo\", \"arguments\": {\"x\": 1}}\n</tool_call>"
                        .to_string(),
                reasoning: None,
            }),
            Ok(StreamChunk::ToolCalls(vec![LlmToolCall {
                id: "call-1".to_string(),
                name: "echo".to_string(),
                input: serde_json::json!({ "x": 1 }),
            }])),
        ],
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

    let (handle, mut rx) = spawn_turn(
        db.clone(),
        chat_state.clone(),
        session,
        thread_id,
        user_message_id,
        assistant_message_id,
        request,
        stream,
    );

    // The raw tag streams live as a `chat-token` before the turn even knows
    // it was a tool call (mirrors the reported bug: a brief flash while
    // generating is a known, accepted residual — only the interim row this
    // test guards below must never carry the leaked markup).
    let (leak_name, leak_payload) = rx.recv().await.expect("an event must arrive");
    assert_eq!(leak_name, "chat-token");
    assert!(leak_payload["delta"]
        .as_str()
        .unwrap()
        .contains("<tool_call>"));

    let (name, payload) = rx.recv().await.expect("an event must arrive");
    assert_eq!(name, "tool-permission-request");

    respond(
        &chat_state,
        extract_request_id(&payload),
        ApprovalDecision::Allow,
    );
    handle.await.unwrap();

    let rows = query::read(&db, |r| msg_store::list_messages(r, thread_id)).unwrap();
    // The tag carried no real text of its own, so once stripped there is
    // nothing left to show — no stray interim `Assistant` row at all, and
    // certainly none containing the raw tag.
    assert!(
        rows.iter()
            .all(|m| m.role != MessageRole::Assistant || !m.content.contains("<tool_call>")),
        "{rows:#?}"
    );
    assert!(
        !rows
            .iter()
            .any(|m| m.role == MessageRole::Assistant && m.id != assistant_message_id),
        "no interim assistant row should exist once the leaked tag is stripped to nothing: {rows:#?}"
    );
}

#[tokio::test]
async fn manual_mode_emits_a_permission_request_for_the_risky_cli_tool() {
    let db = open_db();
    let thread_id = Uuid::new_v4();
    let user_message_id = Uuid::new_v4();
    let assistant_message_id = Uuid::new_v4();
    seed_thread(&db, thread_id, user_message_id);
    set_permission_mode(&db, "manual");

    // `ChatState::new()` registers the host-CLI tool unconditionally.
    let chat_state = Arc::new(ChatState::new());

    let adapter = StubAdapter::new(vec![
        vec![Ok(StreamChunk::ToolCalls(vec![LlmToolCall {
            id: "call-1".to_string(),
            name: "run_command".to_string(),
            input: serde_json::json!({ "command": "echo hi" }),
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

    let (handle, mut rx) = spawn_turn(
        db.clone(),
        chat_state.clone(),
        session,
        thread_id,
        user_message_id,
        assistant_message_id,
        request,
        stream,
    );

    let (name, payload) = rx.recv().await.expect("an event must arrive");
    assert_eq!(name, "tool-permission-request");
    assert_eq!(payload["riskClass"], "risky");
    assert_eq!(payload["toolName"], "run_command");

    respond(
        &chat_state,
        extract_request_id(&payload),
        ApprovalDecision::Allow,
    );
    handle.await.unwrap();

    let rows = query::read(&db, |r| msg_store::list_messages(r, thread_id)).unwrap();
    let tool_result = rows
        .iter()
        .find(|m| m.role == MessageRole::ToolResult)
        .expect("tool_result row");
    assert_eq!(tool_result.tool_is_error, Some(false));
    assert_eq!(tool_result.content.trim(), "hi");
}

#[tokio::test]
async fn denying_a_request_produces_an_error_result_and_the_turn_continues() {
    let db = open_db();
    let thread_id = Uuid::new_v4();
    let user_message_id = Uuid::new_v4();
    let assistant_message_id = Uuid::new_v4();
    seed_thread(&db, thread_id, user_message_id);
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

    let (handle, mut rx) = spawn_turn(
        db.clone(),
        chat_state.clone(),
        session,
        thread_id,
        user_message_id,
        assistant_message_id,
        request,
        stream,
    );

    let (_name, payload) = rx.recv().await.expect("an event must arrive");
    respond(
        &chat_state,
        extract_request_id(&payload),
        ApprovalDecision::Deny,
    );
    handle.await.unwrap();

    let rows = query::read(&db, |r| msg_store::list_messages(r, thread_id)).unwrap();
    let tool_result = rows
        .iter()
        .find(|m| m.role == MessageRole::ToolResult)
        .expect("tool_result row");
    assert_eq!(tool_result.tool_is_error, Some(true));
    let final_row = rows
        .iter()
        .find(|m| m.id == assistant_message_id)
        .expect("terminal assistant row");
    assert_eq!(final_row.finish_reason, Some(FinishReason::Complete));
}

#[tokio::test]
async fn an_unanswered_request_leaves_the_turn_waiting_indefinitely() {
    let db = open_db();
    let thread_id = Uuid::new_v4();
    let user_message_id = Uuid::new_v4();
    let assistant_message_id = Uuid::new_v4();
    seed_thread(&db, thread_id, user_message_id);
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

    let adapter = StubAdapter::new(vec![vec![Ok(StreamChunk::ToolCalls(vec![LlmToolCall {
        id: "call-1".to_string(),
        name: "echo".to_string(),
        input: serde_json::json!({}),
    }]))]]);
    let session = session_with(adapter).await;
    let request = base_request();
    let stream = session.adapter.stream_chat(request.clone()).await.unwrap();

    let (mut handle, mut rx) = spawn_turn(
        db.clone(),
        chat_state.clone(),
        session,
        thread_id,
        user_message_id,
        assistant_message_id,
        request,
        stream,
    );

    let (name, _payload) = rx.recv().await.expect("an event must arrive");
    assert_eq!(name, "tool-permission-request");

    // Never respond — the turn must still be running after a bounded
    // wait, not auto-decided one way or the other (spec.md Acceptance
    // Scenario 5).
    let outcome = tokio::time::timeout(std::time::Duration::from_millis(200), &mut handle).await;
    assert!(
        outcome.is_err(),
        "an unanswered request must not be auto-decided"
    );
    handle.abort();
}

#[tokio::test]
async fn plan_mode_blocks_a_risky_tool_without_any_prompt() {
    let db = open_db();
    let thread_id = Uuid::new_v4();
    let user_message_id = Uuid::new_v4();
    let assistant_message_id = Uuid::new_v4();
    seed_thread(&db, thread_id, user_message_id);
    set_permission_mode(&db, "plan");

    // `ChatState::new()` registers the host-CLI tool, always `Risky`.
    let chat_state = ChatState::new();

    let adapter = StubAdapter::new(vec![
        vec![Ok(StreamChunk::ToolCalls(vec![LlmToolCall {
            id: "call-1".to_string(),
            name: "run_command".to_string(),
            input: serde_json::json!({ "command": "echo hi" }),
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

    let (rows, events) = run_scripted_turn(
        &db,
        &chat_state,
        &session,
        thread_id,
        user_message_id,
        assistant_message_id,
    )
    .await;

    assert!(
        !events.iter().any(|(n, _)| n == "tool-permission-request"),
        "plan mode must never prompt for a blocked risky action"
    );
    let tool_result = rows
        .iter()
        .find(|m| m.role == MessageRole::ToolResult)
        .expect("tool_result row");
    assert_eq!(tool_result.tool_is_error, Some(true));
    assert_eq!(tool_result.content, "blocked_by_plan_mode");
    let final_row = rows
        .iter()
        .find(|m| m.id == assistant_message_id)
        .expect("terminal assistant row");
    assert_eq!(final_row.finish_reason, Some(FinishReason::Complete));
}

/// T030 (US3): aborting while the host-CLI tool is actually executing (as
/// opposed to waiting on approval) kills the OS process and ends the turn
/// as `Cancelled`, with no rows persisted for the interrupted round. Only
/// one step is scripted — if the loop incorrectly proceeded to a further
/// step after cancellation (T033), `StubAdapter` would panic on running out
/// of scripted steps, and `handle.await.unwrap()` below would fail.
#[tokio::test]
async fn aborting_during_tool_execution_kills_the_process_and_ends_the_turn() {
    let db = open_db();
    let thread_id = Uuid::new_v4();
    let user_message_id = Uuid::new_v4();
    let assistant_message_id = Uuid::new_v4();
    seed_thread(&db, thread_id, user_message_id);
    // `auto` still asks for a `Risky` call (the CLI tool is always Risky,
    // spec.md FR-015) — this only avoids a second, irrelevant prompt for
    // the mode switcher itself; `manual` would behave identically here.
    set_permission_mode(&db, "auto");

    let chat_state = Arc::new(ChatState::new());

    let adapter = StubAdapter::new(vec![vec![Ok(StreamChunk::ToolCalls(vec![LlmToolCall {
        id: "call-1".to_string(),
        name: "run_command".to_string(),
        input: serde_json::json!({ "command": "sleep 5" }),
    }]))]]);
    let session = session_with(adapter).await;
    let request = base_request();
    let stream = session.adapter.stream_chat(request.clone()).await.unwrap();

    let (handle, mut rx) = spawn_turn(
        db.clone(),
        chat_state.clone(),
        session,
        thread_id,
        user_message_id,
        assistant_message_id,
        request,
        stream,
    );

    let (name, payload) = rx.recv().await.expect("an event must arrive");
    assert_eq!(name, "tool-permission-request");
    respond(
        &chat_state,
        extract_request_id(&payload),
        ApprovalDecision::Allow,
    );

    // Give the approved call a moment to actually spawn `sleep 5` before
    // aborting, so this exercises "kill a running process", not "cancel
    // before it ever started".
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    let started = std::time::Instant::now();
    abort_turn(&chat_state).unwrap();

    handle.await.unwrap();
    let elapsed = started.elapsed();
    assert!(
        elapsed < std::time::Duration::from_secs(2),
        "abort must kill the sleeping process rather than waiting it out: took {elapsed:?}"
    );

    let rows = query::read(&db, |r| msg_store::list_messages(r, thread_id)).unwrap();
    assert!(
        rows.iter()
            .all(|m| m.role != MessageRole::ToolCall && m.role != MessageRole::ToolResult),
        "an interrupted round must leave no tool_call/tool_result rows"
    );
    let final_row = rows
        .iter()
        .find(|m| m.id == assistant_message_id)
        .expect("terminal assistant row");
    assert_eq!(final_row.finish_reason, Some(FinishReason::Cancelled));
    assert!(
        !events_after(&mut rx)
            .iter()
            .any(|(n, _)| n == "chat-tool-call" || n == "chat-tool-result"),
        "no tool-call/tool-result event for an interrupted round"
    );
}

/// T031 (US3): aborting while a `tool-permission-request` is still pending
/// (never approved or denied) resolves the wait as cancelled, distinct
/// from a user `Deny` — no `tool_result` row with a denial reason, and the
/// turn ends as `Cancelled` rather than continuing (spec.md Acceptance
/// Scenario, US3).
#[tokio::test]
async fn aborting_a_pending_permission_request_cancels_the_turn_not_denies_it() {
    let db = open_db();
    let thread_id = Uuid::new_v4();
    let user_message_id = Uuid::new_v4();
    let assistant_message_id = Uuid::new_v4();
    seed_thread(&db, thread_id, user_message_id);
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

    // Only one step scripted — see the T030 test above for why that also
    // covers T033 (no further step after cancellation).
    let adapter = StubAdapter::new(vec![vec![Ok(StreamChunk::ToolCalls(vec![LlmToolCall {
        id: "call-1".to_string(),
        name: "echo".to_string(),
        input: serde_json::json!({}),
    }]))]]);
    let session = session_with(adapter).await;
    let request = base_request();
    let stream = session.adapter.stream_chat(request.clone()).await.unwrap();

    let (handle, mut rx) = spawn_turn(
        db.clone(),
        chat_state.clone(),
        session,
        thread_id,
        user_message_id,
        assistant_message_id,
        request,
        stream,
    );

    let (name, _payload) = rx.recv().await.expect("an event must arrive");
    assert_eq!(name, "tool-permission-request");

    // Never respond — abort instead.
    abort_turn(&chat_state).unwrap();
    handle.await.unwrap();

    let rows = query::read(&db, |r| msg_store::list_messages(r, thread_id)).unwrap();
    assert!(
        rows.iter()
            .all(|m| m.role != MessageRole::ToolCall && m.role != MessageRole::ToolResult),
        "an interrupted round must leave no tool_call/tool_result rows — no denial row either"
    );
    let final_row = rows
        .iter()
        .find(|m| m.id == assistant_message_id)
        .expect("terminal assistant row");
    assert_eq!(final_row.finish_reason, Some(FinishReason::Cancelled));
}

/// Drains whatever is left in `rx` right now without blocking further —
/// used after a turn has already finished to inspect the full event tail.
fn events_after(
    rx: &mut tokio::sync::mpsc::UnboundedReceiver<(String, Value)>,
) -> Vec<(String, Value)> {
    let mut out = Vec::new();
    while let Ok(event) = rx.try_recv() {
        out.push(event);
    }
    out
}

#[tokio::test]
async fn two_independent_risky_calls_each_get_their_own_pending_request() {
    let db = open_db();
    let thread_id = Uuid::new_v4();
    let user_message_id = Uuid::new_v4();
    let assistant_message_id = Uuid::new_v4();
    seed_thread(&db, thread_id, user_message_id);
    set_permission_mode(&db, "manual");

    let chat_state = Arc::new(ChatState::new());
    {
        let mut registry = chat_state.tool_registry.lock().unwrap();
        registry.register(Arc::new(ScriptedTool {
            name: "tool_a",
            risk_class: RiskClass::Risky,
            fails: false,
        }));
        registry.register(Arc::new(ScriptedTool {
            name: "tool_b",
            risk_class: RiskClass::Risky,
            fails: false,
        }));
    }

    let adapter = StubAdapter::new(vec![
        vec![Ok(StreamChunk::ToolCalls(vec![
            LlmToolCall {
                id: "call-a".to_string(),
                name: "tool_a".to_string(),
                input: serde_json::json!({}),
            },
            LlmToolCall {
                id: "call-b".to_string(),
                name: "tool_b".to_string(),
                input: serde_json::json!({}),
            },
        ]))],
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

    let (mut handle, mut rx) = spawn_turn(
        db.clone(),
        chat_state.clone(),
        session,
        thread_id,
        user_message_id,
        assistant_message_id,
        request,
        stream,
    );

    let (name1, payload1) = rx.recv().await.expect("first request");
    assert_eq!(name1, "tool-permission-request");
    let (name2, payload2) = rx.recv().await.expect("second request");
    assert_eq!(name2, "tool-permission-request");

    let id1 = extract_request_id(&payload1);
    let id2 = extract_request_id(&payload2);
    assert_ne!(id1, id2, "each call must mint its own request id");

    respond(&chat_state, id1, ApprovalDecision::Allow);
    let outcome = tokio::time::timeout(std::time::Duration::from_millis(150), &mut handle).await;
    assert!(
        outcome.is_err(),
        "approving one request must not resolve or affect the other"
    );

    respond(&chat_state, id2, ApprovalDecision::Deny);
    handle.await.unwrap();

    let rows = query::read(&db, |r| msg_store::list_messages(r, thread_id)).unwrap();
    let result_a = rows
        .iter()
        .find(|m| m.role == MessageRole::ToolResult && m.tool_call_id.as_deref() == Some("call-a"))
        .expect("result for call-a");
    let result_b = rows
        .iter()
        .find(|m| m.role == MessageRole::ToolResult && m.tool_call_id.as_deref() == Some("call-b"))
        .expect("result for call-b");
    assert_eq!(result_a.tool_is_error, Some(false));
    assert_eq!(result_b.tool_is_error, Some(true));
}

#[tokio::test]
async fn a_mode_change_while_pending_only_affects_the_next_tool_use() {
    let db = open_db();
    let thread_id = Uuid::new_v4();
    let user_message_id = Uuid::new_v4();
    let assistant_message_id = Uuid::new_v4();
    seed_thread(&db, thread_id, user_message_id);
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

    // Round 1 (Manual -> Ask) then round 2 (mode flips to Auto in
    // between -> must not ask again for the same Safe tool).
    let adapter = StubAdapter::new(vec![
        vec![Ok(StreamChunk::ToolCalls(vec![LlmToolCall {
            id: "call-1".to_string(),
            name: "echo".to_string(),
            input: serde_json::json!({}),
        }]))],
        vec![Ok(StreamChunk::ToolCalls(vec![LlmToolCall {
            id: "call-2".to_string(),
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

    let (handle, mut rx) = spawn_turn(
        db.clone(),
        chat_state.clone(),
        session,
        thread_id,
        user_message_id,
        assistant_message_id,
        request,
        stream,
    );

    let (name1, payload1) = rx.recv().await.expect("round 1's request");
    assert_eq!(name1, "tool-permission-request");
    let id1 = extract_request_id(&payload1);

    // Flip the mode while round 1's request is still pending.
    set_permission_mode(&db, "auto");
    respond(&chat_state, id1, ApprovalDecision::Allow);

    handle.await.unwrap();
    rx.close();
    let mut further_requests = 0;
    while let Ok((name, _)) = rx.try_recv() {
        if name == "tool-permission-request" {
            further_requests += 1;
        }
    }
    assert_eq!(
        further_requests, 0,
        "round 2 must observe the new mode and execute without asking"
    );

    let rows = query::read(&db, |r| msg_store::list_messages(r, thread_id)).unwrap();
    let tool_call_rows = rows
        .iter()
        .filter(|m| m.role == MessageRole::ToolCall)
        .count();
    assert_eq!(tool_call_rows, 2, "both rounds must have executed");
}

/// CI-stable E2E coverage for the complete vault → loaded-model-session →
/// real host CLI → follow-up response path. The adapter is scripted so CI
/// tests the command/tool-loop wiring without depending on sampling behavior
/// or a network-hosted model artifact; the ignored test below covers the
/// additional real-GGUF boundary.
#[tokio::test]
async fn ci_e2e_loaded_model_can_request_and_process_a_cli_command() {
    let db = open_db();
    let thread_id = Uuid::new_v4();
    let user_message_id = Uuid::new_v4();
    let assistant_message_id = Uuid::new_v4();
    seed_thread(&db, thread_id, user_message_id);
    set_permission_mode(&db, "manual");

    let marker = "HOLZI_CI_E2E_CLI_OK";
    #[cfg(not(windows))]
    let expected_command = format!("printf {marker}");
    #[cfg(windows)]
    let expected_command = format!("echo {marker}");

    let adapter = StubAdapter::new(vec![
        vec![Ok(StreamChunk::ToolCalls(vec![LlmToolCall {
            id: "ci-cli-call".to_string(),
            name: "run_command".to_string(),
            input: serde_json::json!({"command": expected_command.clone()}),
        }]))],
        vec![
            Ok(StreamChunk::Delta {
                content: format!("CLI output: {marker}"),
                reasoning: None,
            }),
            Ok(StreamChunk::Done {
                finish_reason: Some("end_turn".to_string()),
                prompt_tokens: Some(1),
                completion_tokens: Some(4),
                ttft_ms: Some(1),
                total_ms: 1,
            }),
        ],
    ]);
    let session = session_with(adapter).await;
    assert_eq!(session.model_id, "stub-model");

    let request = ChatRequest {
        sampling: Default::default(),
        model_id: session.model_id.clone(),
        thread_id: None,
        system_prompt: Some("Use run_command exactly once.".to_string()),
        messages: Vec::new(),
        reasoning_requested: false,
        max_new_tokens: Some(16),
        tools: vec![ToolSpec {
            name: "run_command".to_string(),
            description: "Runs a shell command and returns its output.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {"command": {"type": "string"}},
                "required": ["command"]
            }),
        }],
        autonomy_mode: Default::default(),
        reasoning_option: Default::default(),
        capabilities: Default::default(),
    };
    let stream = session
        .adapter
        .stream_chat(request.clone())
        .await
        .expect("the loaded test model must start streaming");
    let chat_state = Arc::new(ChatState::new());
    let (handle, mut events) = spawn_turn(
        db.clone(),
        chat_state.clone(),
        session,
        thread_id,
        user_message_id,
        assistant_message_id,
        request,
        stream,
    );

    let mut permission_request_seen = false;
    while let Some((event_name, payload)) = events.recv().await {
        if event_name != "tool-permission-request" {
            continue;
        }
        assert!(
            !permission_request_seen,
            "the model requested more than one command"
        );
        permission_request_seen = true;
        assert_eq!(payload["toolName"], "run_command");
        assert_eq!(payload["riskClass"], "risky");
        assert_eq!(
            payload["toolInput"]["command"], expected_command,
            "the fixture must approve only the expected harmless command"
        );
        respond(
            &chat_state,
            extract_request_id(&payload),
            ApprovalDecision::Allow,
        );
    }
    handle.await.expect("the CI E2E turn task must not panic");
    assert!(permission_request_seen);

    let rows = query::read(&db, |r| msg_store::list_messages(r, thread_id))
        .expect("the encrypted vault must contain the completed turn");
    let tool_call = rows
        .iter()
        .find(|message| message.role == MessageRole::ToolCall)
        .expect("the CLI call must be persisted");
    assert_eq!(tool_call.tool_source.as_deref(), Some("cli"));

    let tool_result = rows
        .iter()
        .find(|message| message.role == MessageRole::ToolResult)
        .expect("the CLI result must be persisted");
    assert_eq!(tool_result.tool_source, None);
    assert_eq!(tool_result.tool_is_error, Some(false));
    assert!(tool_result.content.contains(marker));

    let assistant = rows
        .iter()
        .find(|message| message.id == assistant_message_id)
        .expect("the final model response must be persisted");
    assert_eq!(assistant.finish_reason, Some(FinishReason::Complete));
    assert!(assistant.content.contains(marker));
}

/// Real-model acceptance coverage for the complete local tool path. This is
/// intentionally ignored in ordinary test runs: it loads a GGUF and may
/// download the tokenizer on first use. The command is approved only after the
/// test has verified the exact harmless fixture command proposed by the model.
#[cfg(feature = "llm-cpu")]
#[tokio::test]
#[ignore = "requires HOLZI_TEST_GGUF pointing at a tool-capable GGUF"]
async fn a_real_local_model_can_request_and_process_a_cli_command() {
    let configured_model_path = env::var("HOLZI_TEST_GGUF")
        .expect("HOLZI_TEST_GGUF must point at a tool-capable GGUF file");
    let model_path = if let Some(rest) = configured_model_path.strip_prefix("~/") {
        env::var_os("HOME")
            .map(|home| PathBuf::from(home).join(rest))
            .unwrap_or_else(|| PathBuf::from(configured_model_path))
    } else {
        PathBuf::from(configured_model_path)
    };
    let tokenizer =
        env::var("HOLZI_TEST_GGUF_TOKENIZER").unwrap_or_else(|_| "Qwen/Qwen3-4B".to_string());
    let model = LocalModel::load(&model_path, Some(&tokenizer))
        .await
        .expect("real local model must load");

    let marker = "HOLZI_REAL_LLM_CLI_OK";
    #[cfg(not(windows))]
    let expected_command = format!("printf {marker}");
    #[cfg(windows)]
    let expected_command = format!("echo {marker}");

    let db = open_db();
    let thread_id = Uuid::new_v4();
    let user_message_id = Uuid::new_v4();
    let assistant_message_id = Uuid::new_v4();
    seed_thread(&db, thread_id, user_message_id);

    let chat_state = Arc::new(ChatState::new());
    let session = ActiveSession {
        model_id: "real-local-test-model".to_string(),
        provider_id: None,
        provider_kind: ProviderKind::Local,
        adapter: Arc::new(LocalAdapter::new(model)),
        tokenizer_repo: tokenizer,
        context_window: None,
        engine: None,
    };
    let request = ChatRequest {
        sampling: Default::default(),
        model_id: String::new(),
        thread_id: None,
        system_prompt: Some("You are an agent with a run_command tool. Use the tool when the user asks you to run a command. After the tool result, answer with the command output and nothing else.".to_string()),
        messages: vec![LlmMessage {
            role: ChatRole::User,
            attachments: Vec::new(),
            content: format!("Use run_command to execute exactly this harmless command: {expected_command}"),
        }],
        reasoning_requested: false,
        max_new_tokens: Some(256),
        tools: vec![ToolSpec {
            name: "run_command".to_string(),
            description: "Runs a shell command on the user's device and returns its output."
                .to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "command": { "type": "string" }
                },
                "required": ["command"]
            }),
        }],
        autonomy_mode: Default::default(),
        reasoning_option: Default::default(),
        capabilities: Default::default(),
    };
    let stream = session
        .adapter
        .stream_chat(request.clone())
        .await
        .expect("real local model must start streaming");

    let (handle, mut events) = spawn_turn(
        db.clone(),
        chat_state.clone(),
        session,
        thread_id,
        user_message_id,
        assistant_message_id,
        request,
        stream,
    );

    let mut approved_one_expected_command = false;
    let mut generated_text = String::new();
    let mut observed_events = Vec::new();
    let mut observed_errors = Vec::new();
    tokio::time::timeout(std::time::Duration::from_secs(600), async {
        while let Some((event_name, payload)) = events.recv().await {
            observed_events.push(event_name.clone());
            if event_name == "chat-message-error" {
                observed_errors.push(payload.clone());
            }
            if event_name == "chat-token" {
                if let Some(delta) = payload["delta"].as_str() {
                    generated_text.push_str(delta);
                }
            }
            if event_name != "tool-permission-request" {
                if event_name == "chat-turn-complete" {
                    break;
                }
                continue;
            }

            assert!(
                !approved_one_expected_command,
                "the model requested more than one command"
            );
            assert_eq!(payload["toolName"], "run_command");
            assert_eq!(payload["riskClass"], "risky");
            let request_id = extract_request_id(&payload);
            let command = payload["toolInput"]["command"]
                .as_str()
                .expect("run_command input must contain a command string");
            assert_eq!(
                command, expected_command,
                "refusing to approve an unexpected model-generated command"
            );
            respond(&chat_state, request_id, ApprovalDecision::Allow);
            approved_one_expected_command = true;
        }
    })
    .await
    .expect("real local tool turn must finish within ten minutes");

    handle
        .await
        .expect("real local tool turn task must not panic");
    assert!(
        approved_one_expected_command,
        "the local model did not request run_command; events={observed_events:?}; errors={observed_errors:?}; generated={generated_text:?}"
    );

    let rows = query::read(&db, |r| msg_store::list_messages(r, thread_id)).unwrap();
    let tool_call = rows
        .iter()
        .find(|message| message.role == MessageRole::ToolCall)
        .expect("the approved CLI call must be persisted");
    assert_eq!(tool_call.tool_name.as_deref(), Some("run_command"));
    assert_eq!(tool_call.tool_source.as_deref(), Some("cli"));
    assert_eq!(
        serde_json::from_str::<Value>(tool_call.tool_input.as_deref().unwrap()).unwrap()["command"],
        expected_command
    );

    let tool_result = rows
        .iter()
        .find(|message| message.role == MessageRole::ToolResult)
        .expect("the CLI result must be persisted");
    assert_eq!(tool_result.tool_is_error, Some(false));
    assert!(tool_result.content.contains(marker));

    let assistant = rows
        .iter()
        .find(|message| message.id == assistant_message_id)
        .expect("the model's post-tool answer must be persisted");
    assert_eq!(assistant.finish_reason, Some(FinishReason::Complete));
    assert!(
        assistant.content.contains(marker),
        "the model must process the CLI output in its final answer: {:?}",
        assistant.content
    );
}
