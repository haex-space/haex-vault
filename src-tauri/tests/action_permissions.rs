//! The approval modes over action tools (spec 032 US2, FR-007, SC-003): every mode × effect cell,
//! what a denied request looks like to the model, and that a guardrail action — never registered
//! as a tool — cannot be reached by guessing its name. The matrix itself is also a unit test of
//! `permission::decide`; this file proves the loop honors it for `ActionTool`.

#[allow(dead_code)]
#[path = "common/tool_loop_fixture.rs"]
mod tool_loop_fixture;
use tool_loop_fixture::*;

#[allow(dead_code)]
#[path = "common/action_fixture.rs"]
mod action_fixture;
use action_fixture::*;

use std::sync::Arc;

use holzi_lib::adapters::types::ToolCall as LlmToolCall;
use holzi_lib::adapters::StreamChunk;
use holzi_lib::chat::session::ChatState;
use holzi_lib::chat::tools::action_bridge::ActionReply;
use holzi_lib::chat::tools::offer::core_offer;
use holzi_lib::chat::tools::ApprovalDecision;
use holzi_lib::storage::chat_messages::{self as msg_store, MessageRole};
use holzi_lib::storage::query;
use serde_json::json;
use uuid::Uuid;

/// What one scripted call of a tool turned into.
struct Observed {
    /// The approval dialog was requested.
    asked: bool,
    /// The call reached the webview's action runner.
    ran: bool,
    /// The content of the tool result the model got.
    result: String,
}

fn call_of(tool: &str) -> Vec<Result<StreamChunk, holzi_lib::adapters::StreamError>> {
    vec![Ok(StreamChunk::ToolCalls(vec![LlmToolCall {
        id: "call-1".to_string(),
        name: tool.to_string(),
        input: json!({}),
    }]))]
}

fn finish() -> Vec<Result<StreamChunk, holzi_lib::adapters::StreamError>> {
    vec![Ok(StreamChunk::Done {
        finish_reason: Some("end_turn".to_string()),
        prompt_tokens: Some(1),
        completion_tokens: Some(1),
        ttft_ms: Some(1),
        total_ms: 1,
    })]
}

/// Runs one turn in which the model calls `tool` once under `mode`; the person answers a dialog
/// with `decision`, the webview answers an action call with success.
async fn observe(mode: &str, effect: &str, tool: &str, decision: ApprovalDecision) -> Observed {
    let db = open_db();
    let thread_id = Uuid::new_v4();
    let user_message_id = Uuid::new_v4();
    seed_thread(&db, thread_id, user_message_id);
    set_permission_mode(&db, mode);

    let chat_state = Arc::new(ChatState::new());
    register_action(
        &chat_state,
        action_def("target_action", "target.action", effect),
    );
    let mut actions = attach_action_events(&chat_state);

    let session = session_with(StubAdapter::new(vec![call_of(tool), finish()])).await;
    let mut request = base_request();
    request.tools = core_offer(&chat_state.tool_registry.lock().unwrap());
    let stream = session.adapter.stream_chat(request.clone()).await.unwrap();
    let (handle, mut events) = spawn_turn(
        db.clone(),
        Arc::clone(&chat_state),
        session,
        thread_id,
        user_message_id,
        Uuid::new_v4(),
        request,
        stream,
    );

    let (mut asked, mut ran) = (false, false);
    loop {
        tokio::select! {
            biased;
            event = events.recv() => match event {
                Some((name, payload)) if name == "tool-permission-request" => {
                    asked = true;
                    respond(&chat_state, extract_request_id(&payload), decision);
                }
                Some(_) => {}
                None => break,
            },
            call = actions.recv() => if let Some((_, payload)) = call {
                ran = true;
                answer(&chat_state, &payload, ActionReply::Ok { result: json!({}) });
            },
        }
    }
    handle.await.unwrap();

    let rows = query::read(&db, |r| msg_store::list_messages(r, thread_id)).unwrap();
    let result = rows
        .iter()
        .find(|m| m.role == MessageRole::ToolResult)
        .map(|m| m.content.clone())
        .unwrap_or_default();
    Observed { asked, ran, result }
}

/// What the matrix says: (asked, runs) for a mode and an effect, with the dialog allowed.
fn expected(mode: &str, effect: &str) -> (bool, bool) {
    match (mode, effect) {
        ("manual", _) => (true, true),
        ("auto", "destructive") => (true, true),
        ("auto", _) => (false, true),
        ("plan", "read") => (false, true),
        ("plan", _) => (false, false),
        _ => unreachable!("{mode} {effect}"),
    }
}

#[tokio::test]
async fn every_mode_and_effect_follows_the_matrix() {
    for mode in ["manual", "auto", "plan"] {
        for effect in ["read", "write", "destructive"] {
            let seen = observe(mode, effect, "target_action", ApprovalDecision::Allow).await;
            let (asked, runs) = expected(mode, effect);
            assert_eq!(seen.asked, asked, "{mode} {effect}: asked");
            assert_eq!(seen.ran, runs, "{mode} {effect}: ran");
            if mode == "plan" && effect != "read" {
                assert_eq!(seen.result, "blocked_by_plan_mode", "{mode} {effect}");
            }
        }
    }
}

#[tokio::test]
async fn a_denied_request_reaches_the_model_as_denied_by_user_and_nothing_runs() {
    for effect in ["write", "destructive"] {
        let seen = observe("manual", effect, "target_action", ApprovalDecision::Deny).await;
        assert!(seen.asked, "{effect}");
        assert!(!seen.ran, "{effect}: a denied action must not run");
        // The marker first, then what to do next (spec 082 FR-014).
        assert!(
            seen.result.starts_with("denied_by_user: "),
            "{effect}: {}",
            seen.result
        );
    }
}

#[tokio::test]
async fn a_guardrail_tool_name_is_unknown_in_every_mode() {
    // Guardrail actions are never pushed by the frontend, so the registry does not hold them.
    for mode in ["manual", "auto", "plan"] {
        let seen = observe(
            mode,
            "write",
            "settings_sync_servers_set",
            ApprovalDecision::Allow,
        )
        .await;
        assert!(!seen.asked, "{mode}");
        assert!(!seen.ran, "{mode}");
        assert!(
            seen.result.contains("unknown tool"),
            "{mode}: {}",
            seen.result
        );
    }
}
