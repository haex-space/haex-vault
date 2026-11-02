//! Unit tests for what remains in `chat/commands.rs`: thread-title
//! validation, model reasoning-capability detection, the operation
//! reservation/abort interaction, and tool-permission resolution.

use super::*;
use crate::adapters::types::ToolSpec;
use crate::chat::send::{reasoning_requested_for, validated_reasoning_option};
use crate::chat::thread_commands::validate_thread_title;
use crate::model_capabilities::{ModelCapabilities, ReasoningControl, ReasoningOption};

#[test]
fn thread_title_validation_trims_and_enforces_visible_length() {
    assert_eq!(
        validate_thread_title("  A useful title  ").unwrap(),
        "A useful title"
    );
    assert!(matches!(
        validate_thread_title("   "),
        Err(HolziError::InvalidInput { .. })
    ));
    assert!(matches!(
        validate_thread_title(&"x".repeat(121)),
        Err(HolziError::InvalidInput { .. })
    ));
}

#[test]
fn thread_title_validation_counts_grapheme_clusters() {
    let title = "e\u{301}".repeat(120);
    assert_eq!(validate_thread_title(&title).unwrap(), title);
    assert!(matches!(
        validate_thread_title(&"e\u{301}".repeat(121)),
        Err(HolziError::InvalidInput { .. })
    ));
}

fn record(reasoning: Option<ReasoningControl>) -> ModelCapabilities {
    ModelCapabilities {
        reasoning,
        ..ModelCapabilities::default()
    }
}

fn presets(ids: &[&str]) -> ReasoningControl {
    ReasoningControl::presets(
        ids.iter()
            .map(|id| ReasoningOption {
                id: id.to_string(),
                label: id.to_string(),
            })
            .collect(),
    )
}

fn run_command_tool() -> Vec<ToolSpec> {
    vec![ToolSpec {
        name: "run_command".to_string(),
        description: "Runs a shell command.".to_string(),
        input_schema: serde_json::json!({"type": "object"}),
    }]
}

#[test]
fn reasoning_is_requested_when_the_cached_record_says_the_model_reasons() {
    let selectable = record(Some(presets(&["low", "high"])));
    let managed = record(Some(ReasoningControl::ModelManaged));

    assert!(reasoning_requested_for(
        Some(&selectable),
        "claude-opus-5",
        &[]
    ));
    assert!(reasoning_requested_for(
        Some(&managed),
        "Qwen/Qwen3-4B",
        &[]
    ));
}

#[test]
fn reasoning_is_not_requested_for_unavailable_or_undetermined_models() {
    let unavailable = record(Some(ReasoningControl::Unavailable));
    let undetermined = record(None);

    assert!(!reasoning_requested_for(
        Some(&unavailable),
        "claude-3-5-sonnet",
        &[]
    ));
    assert!(!reasoning_requested_for(
        Some(&undetermined),
        "claude-opus-5",
        &[]
    ));
    assert!(!reasoning_requested_for(None, "claude-opus-5", &[]));
}

#[test]
fn tool_requests_disable_reasoning_for_qwen_tool_call_compatibility() {
    let managed = record(Some(ReasoningControl::ModelManaged));

    assert!(!reasoning_requested_for(
        Some(&managed),
        "Qwen/Qwen3-4B",
        &run_command_tool()
    ));
    assert!(reasoning_requested_for(
        Some(&managed),
        "Qwen/Qwen3-4B",
        &[]
    ));
    assert!(reasoning_requested_for(
        Some(&managed),
        "deepseek-r1-distill",
        &run_command_tool()
    ));
}

/// Deliberate: the rule matches every id that contains `qwen3`, so `qwen3.5-…` is covered too. The
/// evaluation of Qwen3.5 through the llama.cpp engine (spec 070, 60 of 83 sentences) ran with thinking
/// off for tool requests, which is how the chat runs it; a Qwen3.5 file that thinks before every tool
/// call would not have measured the same. Change the rule and the measurement together.
#[test]
fn tool_requests_disable_reasoning_for_qwen3_5_as_well() {
    let managed = record(Some(ReasoningControl::ModelManaged));

    assert!(!reasoning_requested_for(
        Some(&managed),
        "qwen3.5-4b-instruct-q4_k_m",
        &run_command_tool()
    ));
    assert!(reasoning_requested_for(
        Some(&managed),
        "qwen3.5-4b-instruct-q4_k_m",
        &[]
    ));
}

#[test]
fn a_reasoning_option_is_kept_only_when_the_model_offers_it() {
    let caps = record(Some(presets(&["low", "high"])));

    assert_eq!(
        validated_reasoning_option(Some(&caps), Some("high".to_string())),
        Some("high".to_string())
    );
    assert_eq!(
        validated_reasoning_option(Some(&caps), Some("xhigh".to_string())),
        None,
        "an option the provider no longer offers is dropped, not sent"
    );
    assert_eq!(validated_reasoning_option(Some(&caps), None), None);
}

#[test]
fn a_reasoning_option_is_dropped_without_selectable_options() {
    let requested = || Some("high".to_string());

    assert_eq!(validated_reasoning_option(None, requested()), None);
    assert_eq!(
        validated_reasoning_option(Some(&record(None)), requested()),
        None
    );
    assert_eq!(
        validated_reasoning_option(
            Some(&record(Some(ReasoningControl::ModelManaged))),
            requested()
        ),
        None
    );
    assert_eq!(
        validated_reasoning_option(
            Some(&record(Some(ReasoningControl::Unavailable))),
            requested()
        ),
        None
    );
}

#[tokio::test]
async fn operation_reservation_survives_transfer_to_the_turn_task() {
    let chat = std::sync::Arc::new(ChatState::new());
    let operation = chat.acquire_operation().unwrap();
    let (finish, wait) = tokio::sync::oneshot::channel();
    let task = tokio::spawn(async move {
        let _operation = operation;
        wait.await.unwrap();
    });
    assert!(
        chat.acquire_operation().is_err(),
        "a second send/load/vault transition must fail"
    );
    abort_turn(&chat).unwrap();
    assert!(
        chat.acquire_operation().is_err(),
        "abort must wait for turn persistence/cleanup"
    );
    finish.send(()).unwrap();
    task.await.unwrap();
    assert!(chat.acquire_operation().is_ok());
}

#[test]
fn late_permission_reply_after_cancellation_is_a_noop_but_unknown_id_is_rejected() {
    let chat = ChatState::new();
    let request_id = Uuid::new_v4();
    let (sender, mut receiver) = tokio::sync::oneshot::channel();
    chat.pending_tool_approvals
        .lock()
        .unwrap()
        .insert(request_id, sender);
    abort_turn(&chat).unwrap();
    assert!(matches!(
        receiver.try_recv(),
        Err(tokio::sync::oneshot::error::TryRecvError::Closed)
    ));
    assert!(resolve_tool_permission(
        &chat,
        RespondToolPermissionArgs {
            request_id,
            decision: ApprovalDecisionWire::Allow,
        }
    )
    .is_ok());
    assert!(matches!(
        resolve_tool_permission(
            &chat,
            RespondToolPermissionArgs {
                request_id: Uuid::new_v4(),
                decision: ApprovalDecisionWire::Allow,
            }
        ),
        Err(HolziError::InvalidInput { .. })
    ));
}
